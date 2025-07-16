# Pingora Cache Study Notes (2)

## Rust Option Chaining Study Notes

Two typical Option chaining patterns observed.

### Example 1: Computing stale configuration

```rust
let serve_stale_while_revalidate_sec = cache_control
    .and_then(|cc| cc.serve_stale_while_revalidate_sec())
    .unwrap_or_else(|| defaults.serve_stale_while_revalidate_sec());
```

Logic:
1. If cache_control exists, try to get stale-while-revalidate value from it
2. If not, use default configuration

`unwrap_or_else` guarantees final return of u32, since stale configuration always needs a value (even if 0).

### Example 2: Computing expiration time

```rust
cache_control
    .and_then(|cc| cc.fresh_sec().and_then(|ttl| freshness_ttl_to_time(now, ttl)))
    .or_else(|| calculate_expires_header_time(resp_header))
    .or_else(|| defaults.fresh_sec(resp_header.status).and_then(|ttl| freshness_ttl_to_time(now, ttl)))
```

Three-level priority:
1. Cache-Control header's max-age
2. Expires header
3. Default TTL based on status code

Returns `Option<SystemTime>`, may be None (indicating non-cacheable).

### Key Points

- `and_then`: continues operation on Some value, short-circuits on None
- `or_else`: provides fallback for None
- Closure parameters (like `|cc|`) are unwrapped values from Option

`|cc|` is the `&CacheControl` passed by `and_then`, much cleaner than nested match or if let.

Difference between examples: stale configuration must have value, while cache expiration time can be None (no caching).

## set_tags Source Code Study Notes

### Function Signature Analysis

```rust
pub fn set_tags<F, I>(&mut self, f: F)
where
    F: FnOnce() -> I,
    I: IntoIterator<Item = Tag>,
```

First time seeing this signature was confusing - why use closure? Why not pass array directly?

### Actual Usage

```rust
span.set_tags(|| {
    [
        Tag::new("created", ts2epoch(internal.created)),
        // ... 6 tags
    ]
});
```

Here `|| { [...] }` returns a `[Tag; 6]` array.

### Core Implementation

```rust
if let Some(inner) = self.0.as_mut() {
    for tag in f() {
        inner.tags.retain(|x| x.name() != tag.name());
        inner.tags.push(tag);
    }
}
```

Key points:
1. Only active spans set tags (`self.0` is `Option<SpanInner>`)
2. Remove same-name tags first, then add new ones (avoid duplicates)

### Implicit Conversion in for Loop

```rust
for tag in f()  // f() returns [Tag; 6]
```

Implicit `into_iter()` call here. Arrays implement `IntoIterator`, so can be used directly in for loops.

Looking at `IntoIterator` trait:
```rust
pub trait IntoIterator {
    type Item;
    type IntoIter: Iterator<Item = Self::Item>;
    fn into_iter(self) -> Self::IntoIter;
}
```

`[T; N]` implementation consumes array, returns iterator.

### Why Use Closure?

1. Lazy execution - no tag array creation when span inactive
2. Flexibility - can pass closures returning Vec, arrays, or other iterable types

### Tag Deduplication Mechanism

```rust
inner.tags.retain(|x| x.name() != tag.name());
inner.tags.push(tag);
```

Before adding new tag, remove all old tags with same name. This ensures:
- Each tag name has only one value
- New value overwrites old value
- Tag order may change (old tags removed, new tags added to end)

### Performance Considerations

- Closures and arrays are stack-allocated, no heap allocation
- For 6 elements, `retain` overhead is minimal
- Compiler should inline closure calls

# Streaming Write Mechanism Deep Analysis

## Core Design Pattern: Producer-Consumer + Async Notification

### Data Flow Diagram
```
Writer(MissHandler) → TempObject.body → Reader(PartialHit)
        ↓                    ↓              ↑
   write_body()         RwLock<Vec<u8>>  watch::Receiver
        ↓                    ↓              ↑
   bytes_written     watch::Sender → Notify new data
```

## Key Component Analysis

### 1. TempObject - Temporary Object Design Essence

```rust
pub(crate) struct TempObject {
    pub meta: BinaryMeta,
    pub body: Arc<RwLock<Vec<u8>>>,           // Shared buffer
    bytes_written: Arc<watch::Sender<PartialState>>, // Progress notifier
}
```

**Design Points:**
- `Arc<RwLock<Vec<u8>>>` enables multi-reader single-writer concurrent access
- `watch::Sender` acts as "progress bar", real-time notification of write status
- `Arc` ensures reader can still access data after `TempObject` removal

### 2. PartialState - State Machine Design

```rust
#[derive(Copy, Clone)]
enum PartialState {
    Partial(usize),   // Partial write, parameter is current byte count
    Complete(usize),  // Write complete, parameter is total byte count
}
```

**State Transitions:**
```
Partial(0) → Partial(n) → ... → Complete(total)
```

### 3. Streaming Write Core: PartialHit.read()

```rust
async fn read(&mut self) -> Option<Bytes> {
    loop {
        let bytes_written = *self.bytes_written.borrow_and_update();
        let bytes_end = match bytes_written {
            PartialState::Partial(s) => s,
            PartialState::Complete(c) => {
                if c == self.bytes_read {
                    return None; // All data read
                }
                c
            }
        };
        
        // New data available to read
        if bytes_end > self.bytes_read {
            let new_bytes = Bytes::copy_from_slice(
                &self.body.read()[self.bytes_read..bytes_end]
            );
            self.bytes_read = bytes_end;
            return Some(new_bytes);
        }
        
        // Wait for new data
        if self.bytes_written.changed().await.is_err() {
            return None; // Writer disconnected
        }
    }
}
```

## Concurrent Write Mechanism

### Unique ID Generation
```rust
// Atomic increment generates unique ID
let temp_id = self.last_temp_id.fetch_add(1, Ordering::Relaxed);

// Double-layer HashMap supports concurrent writes
// key -> (temp_id -> TempObject)
temp: Arc<RwLock<HashMap<String, HashMap<u64, TempObject>>>>
```

### Lookup Priority Strategy
```rust
// Priority in lookup(): temp > cached
if let Some((_, temp_obj)) = self.temp.read().get(&hash)
    .and_then(|map| map.iter().next()) {
    // Prioritize returning objects being written
    hit_from_temp_obj(temp_obj)
} else if let Some(obj) = self.cached.read().get(&hash) {
    // Return complete cache object
    // ...
}
```

## Memory Management Sophistication

### 1. Zero-copy Sharing
```rust
// Conversion after write completion
fn make_cache_object(&self) -> CacheObject {
    let body = Arc::new(self.body.read().clone()); // Final copy
    CacheObject { meta: self.meta.clone(), body }
}
```

### 2. Automatic Cleanup Mechanism
```rust
impl Drop for MemMissHandler {
    fn drop(&mut self) {
        // Auto cleanup temp object on abnormal exit
        self.temp.write()
            .get_mut(&self.key)
            .and_then(|map| map.remove(&self.temp_id.into()));
    }
}
```

## Timing Analysis: Complete Write-Read Flow

### Scenario: Client requests cache object being written

```
Time T1: Writer starts writing
├── Create TempObject
├── Insert into temp HashMap
└── Send PartialState::Partial(0)

Time T2: Reader requests data
├── lookup() finds object in temp
├── Create PartialHit
└── Start listening to watch::Receiver

Time T3: Writer writes first data chunk
├── Extend body: Vec<u8>
├── Send PartialState::Partial(1024)
└── PartialHit.read() awakened

Time T4: Reader reads data
├── Copy data from body[0..1024]
├── Update bytes_read = 1024
└── Return Bytes to client

Time T5: Writer completes writing
├── Send PartialState::Complete(2048)
├── Call finish() transfer to cached
└── Clean up object in temp

Time T6: Reader reads remaining data
├── Read body[1024..2048]
├── Return final Bytes
└── Next call returns None
```

## Performance Optimization Details

### 1. Avoid Busy Waiting
```rust
// Use tokio::sync::watch to avoid polling
if self.bytes_written.changed().await.is_err() {
    return None;
}
```

### 2. Minimize Lock Contention
```rust
// Reading only needs read lock, writing needs write lock
let new_bytes = Bytes::copy_from_slice(&self.body.read()[self.bytes_read..bytes_end]);
```

### 3. Atomic Operations Reduce Sync Overhead
```rust
// Use atomic operations to generate ID, avoid mutex
let temp_id = self.last_temp_id.fetch_add(1, Ordering::Relaxed);
```