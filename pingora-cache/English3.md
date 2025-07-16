# Pingora LRU Implementation Core Analysis

## Architecture Hierarchy

```
EvictionManager trait
├── simple_lru::Manager (simple implementation)
└── lru::Manager (sharded implementation)
    └── pingora_lru::Lru<T, N>
        └── LruUnit<T>
            ├── HashMap<u64, Box<LruNode<T>>>
            └── LinkedList (custom doubly-linked list)
```

## Core Design Principles

### 1. Sharded Architecture
```rust
pub struct Lru<T, const N: usize> {
    units: [RwLock<LruUnit<T>>; N],  // N shards
    weight: AtomicUsize,             // Global weight
    len: AtomicUsize,                // Global length
}
```

**Sharding Strategy:**
- `get_shard(key, N) = key % N`
- Reduces lock contention, improves concurrency
- Atomic counters maintain global statistics

### 2. Custom Doubly-Linked List Optimization

**Traditional Linked List Problems:**
- Memory fragmentation
- Pointer chasing overhead
- Cache-unfriendly

**Pingora Solution:**
```rust
struct LinkedList {
    nodes: Nodes,           // Contiguous memory array
    free: Vec<Index>,       // Free node recycling
}

struct Nodes {
    head: Node,             // Head sentinel
    tail: Node,             // Tail sentinel  
    data_nodes: Vec<Node>,  // Data node array
}
```

**Key Optimizations:**
- Pre-allocate contiguous memory, avoid fragmentation
- Indices instead of pointers, cache-friendly
- Node recycling mechanism, reduce allocation overhead
- Sentinel nodes simplify boundary handling

### 3. LRU Unit Design

```rust
struct LruUnit<T> {
    lookup_table: HashMap<u64, Box<LruNode<T>>>,  // O(1) lookup
    order: LinkedList,                            // Maintain access order
    used_weight: usize,                           // Current weight
}

struct LruNode<T> {
    data: T,
    list_index: usize,    // Index in linked list
    weight: usize,        // Node weight
}
```

## Core Algorithms

### admit() - Admission Algorithm
```rust
pub fn admit(&mut self, key: u64, data: T, weight: usize) -> usize {
    if let Some(node) = self.lookup_table.get_mut(&key) {
        // Update existing node
        let old_weight = node.weight;
        node.data = data;
        node.weight = weight;
        self.order.promote(node.list_index);  // Promote to head
        return old_weight;
    }
    
    // Insert new node
    let list_index = self.order.push_head(key);
    let node = Box::new(LruNode { data, list_index, weight });
    self.lookup_table.insert(key, node);
    0
}
```

### promote() - Promotion Algorithm
```rust
pub fn promote(&mut self, index: Index) {
    if self.nodes.head().next == index {
        return; // Already at head
    }
    self.lift(index);           // Remove from current position
    self.insert_after(index, HEAD); // Insert at head
}
```

### evict_to_limit() - Eviction Algorithm
```rust
pub fn evict_to_limit(&self) -> Vec<(T, usize)> {
    let mut evicted = vec![];
    let mut shard_seed = rand::random(); // Random starting shard
    
    while self.weight() > self.weight_limit {
        if let Some(item) = self.evict_shard(shard_seed) {
            evicted.push(item);
        }
        shard_seed += 1; // Poll next shard
    }
    evicted
}
```

## Performance Optimization Techniques

### 1. promote_top_n() - Reduce Write Locks
```rust
pub fn promote_top_n(&self, key: u64, top: usize) -> bool {
    let unit = &self.units[get_shard(key, N)];
    if !unit.read().need_promote(key, top) {  // Read lock check
        return true;
    }
    unit.write().access(key)  // Acquire write lock only when needed
}
```

### 2. Memory Growth Control
```rust
if self.data_nodes.capacity() > VEC_EXP_GROWTH_CAP
    && self.data_nodes.capacity() - self.data_nodes.len() < 2 {
    // Limit memory waste to within 10%
    self.data_nodes.reserve_exact(self.data_nodes.capacity() / 10)
}
```

### 3. Serialization Optimization
**Parallel Shard Serialization:**
```rust
async fn save(&self, dir_path: &str) -> Result<()> {
    for i in 0..N {
        let data = self.serialize_shard(i)?;
        tokio::task::spawn_blocking(move || {
            // Atomic write: tmp -> final
            let temp_path = format!("{}.{:08x}.tmp", FILE_NAME, random_suffix);
            write_and_rename(temp_path, final_path, data)
        }).await?
    }
}
```

## Concurrency Safety Guarantees

1. **Shard Locking**: Independent RwLock per shard
2. **Atomic Counting**: AtomicUsize maintains global state
3. **Lock-free Reading**: peek() operations only need read locks
4. **Batch Operations**: Reduce lock acquisition frequency

## Memory Efficiency

1. **Contiguous Allocation**: LinkedList avoids memory fragmentation
2. **Node Recycling**: Free list reuses released nodes
3. **Pre-allocated Capacity**: Avoid frequent expansions
4. **Weight Tracking**: Precise memory usage statistics

# Cache Management Module Analysis

## Three Core Modules

### 1. Cacheability Predictor
**Purpose**: Prevent invalid cache requests, improve system efficiency

```rust
pub struct Predictor<const N_SHARDS: usize> {
    uncacheable_keys: ConcurrentLruCache<(), N_SHARDS>,  // Remember uncacheable keys
    skip_custom_reasons_fn: Option<CustomReasonPredicate>, // Custom skip rules
}
```

**Core Algorithms**:
- `cacheable_prediction()`: Check if key is in blacklist
- `mark_uncacheable()`: Decide whether to record as uncacheable based on reason
- `mark_cacheable()`: Remove from blacklist

**Smart Filtering**:
```rust
match reason {
    InternalError | StorageError | CacheLockTimeout => None, // Don't record
    OriginNotCache | ResponseTooLarge => Some(true),         // Record
    Custom(reason) => User-defined judgment
}
```

### 2. Cache Writer (CachePutCtx)
**Purpose**: Stream-process HTTP responses, decide whether to cache

```rust
pub struct CachePutCtx<C: CachePut> {
    cache_put: C,                    // User-defined cache policy
    storage: &'static Storage,       // Storage backend
    miss_handler: Option<MissHandler>, // Write handler
    parser: ResponseParse,           // HTTP response parser
}
```

**Processing Flow**:
1. **Parse Headers**: Check Cache-Control and other headers
2. **Cacheability Decision**: Call user-defined `cacheable()`
3. **Streaming Write**: Parse and write to storage simultaneously
4. **Size Limits**: Support Content-Length pre-checking

### 3. Variance Key Builder (VarianceBuilder)
**Purpose**: Generate unique identifiers for different versions of same URL

```rust
pub struct VarianceBuilder<'a> {
    values: BTreeMap<Cow<'a, str>, Cow<'a, [u8]>>, // Ordered key-value pairs
}
```

**Features**:
- **Order-independent**: BTreeMap ensures same variance produces same hash
- **Zero-copy**: Uses Cow to avoid unnecessary memory allocation
- **Blake2b Hash**: Fast with low collision probability

## Key Design Philosophy

### Predictive Cache Optimization
Traditional approach: Try caching every time, discard on failure
Pingora approach: Remember historical failures, skip proactively

### Stream Processing
```rust
async fn cache_put(&mut self, session: &mut ServerSession) -> Result<Option<NoCacheReason>> {
    while let Some(data) = session.read_request_body().await? {
        no_cache_reason = self.do_cache_put(&data).await?
    }
}
```
- Read and write simultaneously, reduce memory usage
- Support large file caching
- Still need to drain body in exceptional cases to ensure connection reuse

### Variance Key Applications
**Scenario**: Same URL returns different content based on Accept-Encoding
```rust
let mut variance = VarianceBuilder::new();
variance.add_value("encoding", "gzip");
let variance_key = variance.finalize(); // Some(hash)
```

## Module Collaboration Flow

```
1. Predictor.cacheable_prediction() 
   ↓ (if potentially cacheable)
2. CachePutCtx.cache_put()
   ↓ (parse response headers)
3. Check Cache-Control, size limits
   ↓ (decide to cache)
4. Stream write to Storage
   ↓ (complete or fail)
5. Predictor.mark_cacheable/uncacheable()
```

## Performance Optimization Points

1. **Sharded LRU**: Predictor uses sharding to reduce lock contention
2. **Pre-allocation**: ResponseParse pre-allocates 4KB buffer
3. **Atomic Writes**: Temporary file + rename ensures consistency
4. **Memory-friendly**: VarianceBuilder supports zero-copy operations