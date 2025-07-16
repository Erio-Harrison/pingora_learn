# Pingora连接池学习笔记

## 数据结构关系

```
ConnectionPool
├── pool: RwLock<HashMap<GroupKey, Arc<PoolNode>>>
│   └── PoolNode (每个group一个)
│       ├── hot_queue: ArrayQueue<(ID, T)>        // 热点连接
│       └── connections: Mutex<HashMap<ID, T>>    // 冷存储
└── lru: Lru<ID, ConnectionMeta>                  // 全局LRU
    └── ThreadLocal<RefCell<LruCache<K, Node<T>>>>
```

## 设计思路

**双层存储**：热点走无锁队列，冷连接用HashMap
**分组隔离**：不同目标独立管理
**ThreadLocal**：每线程独立避免竞争

## 使用流程

**存放连接**：
```rust
let (notify_close, watch_use) = pool.put(&meta, connection);
// 1. LRU添加，可能淘汰旧连接
// 2. 优先放hot_queue，满了放HashMap
```

**获取连接**：
```rust
let conn = pool.get(&group_key);
// 1. 先从hot_queue弹出
// 2. 再从HashMap查找
// 3. 从LRU中移除
```

**健康监控**：
```rust
pool.idle_poll(connection, meta, timeout, notify_close, watch_use);
// 监听：连接断开/超时/被复用/被淘汰
```

## 核心优化

**ArrayQueue无锁**：基于CPU原子指令，避免锁开销
**双检查锁**：读锁检查，写锁创建，减少竞争
**异步监控**：主动检测连接状态，及时清理

适合高并发代理场景，频繁连接复用。