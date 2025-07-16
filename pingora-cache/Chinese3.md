# Pingora LRU 实现核心解析

## 架构层次

```
EvictionManager trait
├── simple_lru::Manager (简单实现)
└── lru::Manager (分片实现)
    └── pingora_lru::Lru<T, N>
        └── LruUnit<T>
            ├── HashMap<u64, Box<LruNode<T>>>
            └── LinkedList (自定义双链表)
```

## 核心设计原理

### 1. 分片架构
```rust
pub struct Lru<T, const N: usize> {
    units: [RwLock<LruUnit<T>>; N],  // N个分片
    weight: AtomicUsize,             // 全局权重
    len: AtomicUsize,                // 全局长度
}
```

**分片策略：**
- `get_shard(key, N) = key % N`
- 减少锁竞争，提高并发性
- 原子计数器维护全局统计

### 2. 自定义双链表优化

**传统链表问题：**
- 内存碎片化
- 指针追踪开销
- 缓存不友好

**Pingora方案：**
```rust
struct LinkedList {
    nodes: Nodes,           // 连续内存数组
    free: Vec<Index>,       // 空闲节点回收
}

struct Nodes {
    head: Node,             // 头哨兵
    tail: Node,             // 尾哨兵  
    data_nodes: Vec<Node>,  // 数据节点数组
}
```

**关键优化：**
- 预分配连续内存，避免碎片
- 索引代替指针，缓存友好
- 节点回收机制，减少分配开销
- 哨兵节点简化边界处理

### 3. LRU单元设计

```rust
struct LruUnit<T> {
    lookup_table: HashMap<u64, Box<LruNode<T>>>,  // O(1)查找
    order: LinkedList,                            // 维护访问顺序
    used_weight: usize,                           // 当前权重
}

struct LruNode<T> {
    data: T,
    list_index: usize,    // 链表中的索引
    weight: usize,        // 节点权重
}
```

## 核心算法

### admit() - 准入算法
```rust
pub fn admit(&mut self, key: u64, data: T, weight: usize) -> usize {
    if let Some(node) = self.lookup_table.get_mut(&key) {
        // 更新现有节点
        let old_weight = node.weight;
        node.data = data;
        node.weight = weight;
        self.order.promote(node.list_index);  // 提升到头部
        return old_weight;
    }
    
    // 插入新节点
    let list_index = self.order.push_head(key);
    let node = Box::new(LruNode { data, list_index, weight });
    self.lookup_table.insert(key, node);
    0
}
```

### promote() - 提升算法
```rust
pub fn promote(&mut self, index: Index) {
    if self.nodes.head().next == index {
        return; // 已在头部
    }
    self.lift(index);           // 从当前位置移除
    self.insert_after(index, HEAD); // 插入到头部
}
```

### evict_to_limit() - 驱逐算法
```rust
pub fn evict_to_limit(&self) -> Vec<(T, usize)> {
    let mut evicted = vec![];
    let mut shard_seed = rand::random(); // 随机起始分片
    
    while self.weight() > self.weight_limit {
        if let Some(item) = self.evict_shard(shard_seed) {
            evicted.push(item);
        }
        shard_seed += 1; // 轮询下一分片
    }
    evicted
}
```

## 性能优化技巧

### 1. promote_top_n() - 减少写锁
```rust
pub fn promote_top_n(&self, key: u64, top: usize) -> bool {
    let unit = &self.units[get_shard(key, N)];
    if !unit.read().need_promote(key, top) {  // 读锁检查
        return true;
    }
    unit.write().access(key)  // 需要时才获取写锁
}
```

### 2. 内存增长控制
```rust
if self.data_nodes.capacity() > VEC_EXP_GROWTH_CAP
    && self.data_nodes.capacity() - self.data_nodes.len() < 2 {
    // 限制内存浪费在10%以内
    self.data_nodes.reserve_exact(self.data_nodes.capacity() / 10)
}
```

### 3. 序列化优化
**分片并行序列化：**
```rust
async fn save(&self, dir_path: &str) -> Result<()> {
    for i in 0..N {
        let data = self.serialize_shard(i)?;
        tokio::task::spawn_blocking(move || {
            // 原子写入：tmp -> final
            let temp_path = format!("{}.{:08x}.tmp", FILE_NAME, random_suffix);
            write_and_rename(temp_path, final_path, data)
        }).await?
    }
}
```

## 并发安全保证

1. **分片锁定**：每个分片独立的RwLock
2. **原子计数**：AtomicUsize维护全局状态
3. **无锁读取**：peek()操作只需读锁
4. **批量操作**：减少锁获取次数

## 内存效率

1. **连续分配**：LinkedList避免内存碎片
2. **节点回收**：free列表重用释放的节点
3. **预分配容量**：避免频繁扩容
4. **权重追踪**：精确的内存使用统计


# 缓存管理模块分析

## 三大核心模块

### 1. 可缓存性预测器 (Predictor)
**目的**: 防止无效缓存请求，提高系统效率

```rust
pub struct Predictor<const N_SHARDS: usize> {
    uncacheable_keys: ConcurrentLruCache<(), N_SHARDS>,  // 记住不可缓存的key
    skip_custom_reasons_fn: Option<CustomReasonPredicate>, // 自定义跳过规则
}
```

**核心算法**:
- `cacheable_prediction()`: 检查key是否在黑名单中
- `mark_uncacheable()`: 根据原因决定是否记录为不可缓存
- `mark_cacheable()`: 从黑名单中移除

**智能过滤**:
```rust
match reason {
    InternalError | StorageError | CacheLockTimeout => None, // 不记录
    OriginNotCache | ResponseTooLarge => Some(true),         // 记录
    Custom(reason) => 用户自定义判断
}
```

### 2. 缓存写入器 (CachePutCtx)
**目的**: 流式处理HTTP响应，决定是否缓存

```rust
pub struct CachePutCtx<C: CachePut> {
    cache_put: C,                    // 用户自定义缓存策略
    storage: &'static Storage,       // 存储后端
    miss_handler: Option<MissHandler>, // 写入处理器
    parser: ResponseParse,           // HTTP响应解析器
}
```

**处理流程**:
1. **解析头部**: 检查Cache-Control等头部
2. **可缓存性判断**: 调用用户定义的`cacheable()`
3. **流式写入**: 边解析边写入存储
4. **大小限制**: 支持Content-Length预检查

### 3. 变化键构建器 (VarianceBuilder)
**目的**: 为同一URL的不同版本生成唯一标识

```rust
pub struct VarianceBuilder<'a> {
    values: BTreeMap<Cow<'a, str>, Cow<'a, [u8]>>, // 有序键值对
}
```

**特性**:
- **顺序无关**: BTreeMap确保相同variance产生相同hash
- **零拷贝**: 使用Cow避免不必要的内存分配
- **Blake2b哈希**: 快速且冲突概率低

## 关键设计思想

### 预测式缓存优化
传统做法：每次都尝试缓存，失败后丢弃
Pingora做法：记住历史失败，提前跳过

### 流式处理
```rust
async fn cache_put(&mut self, session: &mut ServerSession) -> Result<Option<NoCacheReason>> {
    while let Some(data) = session.read_request_body().await? {
        no_cache_reason = self.do_cache_put(&data).await?
    }
}
```
- 边读边写，降低内存使用
- 支持大文件缓存
- 异常情况下仍需排空body确保连接复用

### 变化键的妙用
**场景**: 同一URL根据Accept-Encoding返回不同内容
```rust
let mut variance = VarianceBuilder::new();
variance.add_value("encoding", "gzip");
let variance_key = variance.finalize(); // Some(hash)
```

## 模块协作流程

```
1. Predictor.cacheable_prediction() 
   ↓ (如果可能可缓存)
2. CachePutCtx.cache_put()
   ↓ (解析响应头)
3. 检查Cache-Control、大小限制
   ↓ (决定缓存)
4. 流式写入Storage
   ↓ (完成或失败)
5. Predictor.mark_cacheable/uncacheable()
```

## 性能优化点

1. **分片LRU**: Predictor使用分片减少锁竞争
2. **预分配**: ResponseParse预分配4KB缓冲区
3. **原子写入**: 临时文件+重命名确保一致性
4. **内存友好**: VarianceBuilder支持零拷贝操作