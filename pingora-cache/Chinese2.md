# Pingora Cache 学习笔记(2)

## Rust Option 链式调用学习笔记

看到两个典型的 Option 链式调用模式。

### 例子1：计算 stale 配置

```rust
let serve_stale_while_revalidate_sec = cache_control
    .and_then(|cc| cc.serve_stale_while_revalidate_sec())
    .unwrap_or_else(|| defaults.serve_stale_while_revalidate_sec());
```

这里的逻辑：
1. 如果有 cache_control，尝试从中获取 stale-while-revalidate 值
2. 如果没有，使用默认配置

`unwrap_or_else` 保证最终一定返回 u32，因为 stale 配置总需要一个值（哪怕是 0）。

### 例子2：计算过期时间

```rust
cache_control
    .and_then(|cc| cc.fresh_sec().and_then(|ttl| freshness_ttl_to_time(now, ttl)))
    .or_else(|| calculate_expires_header_time(resp_header))
    .or_else(|| defaults.fresh_sec(resp_header.status).and_then(|ttl| freshness_ttl_to_time(now, ttl)))
```

三级优先级：
1. Cache-Control 头的 max-age
2. Expires 头
3. 基于状态码的默认 TTL

返回 `Option<SystemTime>`，可能为 None（表示不可缓存）。

### 关键点

- `and_then`：对 Some 值继续操作，遇到 None 就短路
- `or_else`：为 None 提供备选方案
- 闭包参数（如 `|cc|`）是 Option 解包后的值

`|cc|` 就是`and_then` 传进来的 `&CacheControl`，比嵌套的 match 或 if let的写法简洁很多。

两个例子的区别在于业务需求：stale 配置必须有值，而缓存过期时间可以没有（不缓存）。

## set_tags 源码学习笔记

### 函数签名分析

```rust
pub fn set_tags<F, I>(&mut self, f: F)
where
    F: FnOnce() -> I,
    I: IntoIterator<Item = Tag>,
```

第一次看到这个签名有点懵，为什么要用闭包？直接传数组不行吗？

### 实际调用

```rust
span.set_tags(|| {
    [
        Tag::new("created", ts2epoch(internal.created)),
        // ... 6个标签
    ]
});
```

这里 `|| { [...] }` 返回一个 `[Tag; 6]` 数组。

### 核心实现

```rust
if let Some(inner) = self.0.as_mut() {
    for tag in f() {
        inner.tags.retain(|x| x.name() != tag.name());
        inner.tags.push(tag);
    }
}
```

关键点：
1. 只有激活的 span 才设置标签（`self.0` 是 `Option<SpanInner>`）
2. 先删除同名标签，再添加新的（避免重复）

### for 循环的隐式转换

```rust
for tag in f()  // f() 返回 [Tag; 6]
```

这里有个隐式的 `into_iter()` 调用。数组实现了 `IntoIterator`，所以可以直接用在 for 循环中。

查看 `IntoIterator` trait：
```rust
pub trait IntoIterator {
    type Item;
    type IntoIter: Iterator<Item = Self::Item>;
    fn into_iter(self) -> Self::IntoIter;
}
```

`[T; N]` 的实现会消耗数组，返回一个迭代器。

### 为什么用闭包？

1. 延迟执行 - span 未激活时不会创建标签数组
2. 灵活性 - 可以传入返回 Vec、数组或其他可迭代类型的闭包

### 标签去重机制

```rust
inner.tags.retain(|x| x.name() != tag.name());
inner.tags.push(tag);
```

每次添加新标签前，先删除所有同名的旧标签。这保证了：
- 每个标签名只有一个值
- 新值会覆盖旧值
- 标签顺序可能会变（旧标签被删除，新标签添加到末尾）

### 性能考虑

- 闭包和数组都在栈上，没有堆分配
- 对于6个元素，`retain` 的开销很小
- 编译器应该能内联闭包调用

# 流式写入机制深度解析

## 核心设计模式：生产者-消费者 + 异步通知

### 数据流向图
```
Writer(MissHandler) → TempObject.body → Reader(PartialHit)
        ↓                    ↓              ↑
   write_body()         RwLock<Vec<u8>>  watch::Receiver
        ↓                    ↓              ↑
   bytes_written     watch::Sender → 通知新数据
```

## 关键组件详解

### 1. TempObject - 临时对象的设计精髓

```rust
pub(crate) struct TempObject {
    pub meta: BinaryMeta,
    pub body: Arc<RwLock<Vec<u8>>>,           // 共享的缓冲区
    bytes_written: Arc<watch::Sender<PartialState>>, // 进度通知器
}
```

**设计要点：**
- `Arc<RwLock<Vec<u8>>>` 实现多读一写的并发访问
- `watch::Sender` 作为"进度条"，实时通知写入状态
- `Arc` 确保在 `TempObject` 被移除后，reader 仍可访问数据

### 2. PartialState - 状态机设计

```rust
#[derive(Copy, Clone)]
enum PartialState {
    Partial(usize),   // 部分写入，参数为当前字节数
    Complete(usize),  // 写入完成，参数为总字节数
}
```

**状态转换：**
```
Partial(0) → Partial(n) → ... → Complete(total)
```

### 3. 流式写入的核心：PartialHit.read()

```rust
async fn read(&mut self) -> Option<Bytes> {
    loop {
        let bytes_written = *self.bytes_written.borrow_and_update();
        let bytes_end = match bytes_written {
            PartialState::Partial(s) => s,
            PartialState::Complete(c) => {
                if c == self.bytes_read {
                    return None; // 全部读取完毕
                }
                c
            }
        };
        
        // 有新数据可读
        if bytes_end > self.bytes_read {
            let new_bytes = Bytes::copy_from_slice(
                &self.body.read()[self.bytes_read..bytes_end]
            );
            self.bytes_read = bytes_end;
            return Some(new_bytes);
        }
        
        // 等待新数据
        if self.bytes_written.changed().await.is_err() {
            return None; // writer 已断开
        }
    }
}
```

## 并发写入机制

### 唯一ID生成
```rust
// 原子递增生成唯一ID
let temp_id = self.last_temp_id.fetch_add(1, Ordering::Relaxed);

// 双层HashMap支持并发写入
// key -> (temp_id -> TempObject)
temp: Arc<RwLock<HashMap<String, HashMap<u64, TempObject>>>>
```

### 查找优先级策略
```rust
// lookup() 中的优先级：temp > cached
if let Some((_, temp_obj)) = self.temp.read().get(&hash)
    .and_then(|map| map.iter().next()) {
    // 优先返回正在写入的对象
    hit_from_temp_obj(temp_obj)
} else if let Some(obj) = self.cached.read().get(&hash) {
    // 返回完整缓存对象
    // ...
}
```

## 内存管理的精妙之处

### 1. 零拷贝共享
```rust
// 写入完成后的转换
fn make_cache_object(&self) -> CacheObject {
    let body = Arc::new(self.body.read().clone()); // 最后一次拷贝
    CacheObject { meta: self.meta.clone(), body }
}
```

### 2. 自动清理机制
```rust
impl Drop for MemMissHandler {
    fn drop(&mut self) {
        // 异常退出时自动清理临时对象
        self.temp.write()
            .get_mut(&self.key)
            .and_then(|map| map.remove(&self.temp_id.into()));
    }
}
```

## 时序分析：完整的写入-读取流程

### 场景：客户端请求正在写入的缓存对象

```
时刻 T1: Writer 开始写入
├── 创建 TempObject
├── 插入到 temp HashMap
└── 发送 PartialState::Partial(0)

时刻 T2: Reader 请求数据
├── lookup() 发现 temp 中有对象
├── 创建 PartialHit
└── 开始监听 watch::Receiver

时刻 T3: Writer 写入第一块数据
├── 扩展 body: Vec<u8>
├── 发送 PartialState::Partial(1024)
└── PartialHit.read() 被唤醒

时刻 T4: Reader 读取数据
├── 从 body[0..1024] 复制数据
├── 更新 bytes_read = 1024
└── 返回 Bytes 给客户端

时刻 T5: Writer 完成写入
├── 发送 PartialState::Complete(2048)
├── 调用 finish() 转移到 cached
└── 清理 temp 中的对象

时刻 T6: Reader 读取剩余数据
├── 读取 body[1024..2048]
├── 返回最后的 Bytes
└── 下次调用返回 None
```

## 性能优化细节

### 1. 避免忙等待
```rust
// 使用 tokio::sync::watch 避免轮询
if self.bytes_written.changed().await.is_err() {
    return None;
}
```

### 2. 最小化锁争用
```rust
// 读取时只需读锁，写入时需要写锁
let new_bytes = Bytes::copy_from_slice(&self.body.read()[self.bytes_read..bytes_end]);
```

### 3. 原子操作减少同步开销
```rust
// 使用原子操作生成ID，避免互斥锁
let temp_id = self.last_temp_id.fetch_add(1, Ordering::Relaxed);
```
