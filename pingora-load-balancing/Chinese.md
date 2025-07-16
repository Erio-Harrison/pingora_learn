# Pingora 负载均衡数据流分析

## 完整数据流程

```
HTTP请求 → 选择算法 → 权重后端 → 迭代器 → 目标后端
   ↓          ↓         ↓        ↓       ↓
[字节]    [u64索引]   [权重展开]  [故障切换]  [目标]
```

## 第1层：算法选择

### 哈希算法（会话保持）
```rust
请求: "user123" → Hash → 0x1a2b3c4d → Backend[1] (一致)
请求: "user123" → Hash → 0x1a2b3c4d → Backend[1] (相同)
```

### 轮询算法（均匀分布）
```rust
请求1 → Counter(0) → Backend[0]
请求2 → Counter(1) → Backend[1]  
请求3 → Counter(2) → Backend[2]
请求4 → Counter(3) → Backend[0] (循环)
```

### 随机算法（负载分散）
```rust
请求1 → Random(0x4f2a) → Backend[2]
请求2 → Random(0x8b1c) → Backend[0]
请求3 → Random(0x3e5d) → Backend[1]
```

## 第2层：权重展开

### 后端配置
```rust
Backend A: weight=1  → [A]
Backend B: weight=3  → [B, B, B]  
Backend C: weight=2  → [C, C]
// 权重数组: [A, B, B, B, C, C]
```

### 首选选择（权重）
```rust
算法 → index=4 → weighted[4 % 6] → Backend C
```

## 第3层：故障切换策略

### 迭代器模式
```rust
WeightedIterator {
    first: true,     // 使用权重选择
    index: 0x1a2b,   // 来自算法
    backend: Arc<Weighted<H>>
}
```

### 选择流程
```rust
// 第一次调用 - 权重选择
next() → first=true → weighted[index % weighted.len()] → Backend B

// 后续调用 - 均匀回退  
next() → first=false → backends[new_index % backends.len()] → Backend A
next() → first=false → backends[new_index % backends.len()] → Backend C
```

## 第4层：一致性哈希特殊情况

### Ketama环结构
```rust
哈希环: [0x1111→A, 0x3333→B, 0x5555→C, 0x7777→A, 0x9999→B]
```

### 环遍历
```rust
Key "user123" → Hash 0x4000 → 找到下一个: 0x5555→C
如果C失败 → 继续环: 0x7777→A  
如果A失败 → 继续环: 0x9999→B
```

## 性能特征

| 算法 | 时间复杂度 | 内存 | 一致性 |
|------|-----------|------|--------|
| 哈希 | O(1)      | O(n) | 高     |
| 轮询 | O(1)      | O(n) | 无     |
| 随机 | O(1)      | O(n) | 无     |
| 一致性| O(log n) | O(n×k)| 中     |

## 故障处理数据流

```
主选择 → 健康检查 → 失败
   ↓        ↓       ↓
Iterator.next() → Backend B → 不可用
   ↓        ↓       ↓
Iterator.next() → Backend A → 成功
   ↓        ↓       ↓
路由请求 → 处理 → 响应
```

## 关键设计优势

1. **关注点分离**：算法生成索引，Weighted处理分布
2. **优雅降级**：权重首选，均匀回退
3. **可插拔算法**：任何Hasher都可用作选择算法
4. **原子操作**：线程安全无锁（RoundRobin）
5. **一致接口**：所有算法返回u64统一处理

## 实际数据变换示例

### 权重后端处理
```rust
// 配置
backends: [A(weight=1), B(weight=3), C(weight=2)]

// 构建阶段
weighted: [A, B, B, B, C, C]  // 索引: 0,1,2,3,4,5

// 运行时
key="user123" → hash=0x1a2b → index=1 → Backend B
key="user456" → hash=0x3f4e → index=4 → Backend C
```

### 故障切换序列
```rust
// 第一次尝试（权重）
iter.next() → weighted[hash % 6] → Backend B → 失败

// 第二次尝试（均匀）
iter.next() → backends[new_hash % 3] → Backend A → 成功
```