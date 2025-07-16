# 学习导图(Learning map)

**基础概念层(Basic concept layer)**
`key.rs` -> `meta.rs` -> `cache_control.rs`

**核心逻辑层(Core logic layer)** 
`filters.rs` -> `lib.rs`

**存储实现层(Storage implementation layer)**
`memory.rs` -> `storage/`

**并发优化层(Concurrency optimization layer)**
`hashtable.rs` -> `lock.rs` -> `eviction/`

**高级特性层(Advanced feature layer)**
`variance.rs` -> `predictor.rs` -> `put.rs`

**辅助工具层(Auxiliary tool layer)**
`max_file_size.rs` -> `trace.rs`

- Start with data structure to build basic concepts
- Then learn core cache logic and state machine
- Then understand storage abstraction
- Finally master performance optimization techniques

- 从数据结构开始建立基础概念
- 再学核心缓存逻辑和状态机
- 然后理解存储抽象
- 最后掌握性能优化技巧