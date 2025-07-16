# Pingora Connection Pool Learning Notes

## Data Structure Relationships

```
ConnectionPool
├── pool: RwLock<HashMap<GroupKey, Arc<PoolNode>>>
│   └── PoolNode (one per group)
│       ├── hot_queue: ArrayQueue<(ID, T)>        // hot connections
│       └── connections: Mutex<HashMap<ID, T>>    // cold storage
└── lru: Lru<ID, ConnectionMeta>                  // global LRU
    └── ThreadLocal<RefCell<LruCache<K, Node<T>>>>
```

## Design Philosophy

**Dual-layer storage**: Hot connections use lock-free queue, cold connections use HashMap
**Group isolation**: Different targets managed independently  
**ThreadLocal**: Per-thread independence avoids contention

## Usage Flow

**Store connection**:
```rust
let (notify_close, watch_use) = pool.put(&meta, connection);
// 1. Add to LRU, may evict old connections
// 2. Prefer hot_queue, fallback to HashMap when full
```

**Get connection**:
```rust
let conn = pool.get(&group_key);
// 1. Pop from hot_queue first
// 2. Search HashMap next
// 3. Remove from LRU
```

**Health monitoring**:
```rust
pool.idle_poll(connection, meta, timeout, notify_close, watch_use);
// Listen for: connection break/timeout/reuse/eviction
```

## Core Optimizations

**ArrayQueue lock-free**: Based on CPU atomic instructions, avoids lock overhead
**Double-checked locking**: Read lock check, write lock create, reduces contention
**Async monitoring**: Proactive connection health detection and cleanup

Suitable for high-concurrency proxy scenarios with frequent connection reuse.