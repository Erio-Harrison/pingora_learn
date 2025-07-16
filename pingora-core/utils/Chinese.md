# 零拷贝缓冲区引用设计学习笔记

## 遇到的问题

解析器不好的写法：

```rust
// 反面“教材”
let headers: Vec<(String, String)> = vec![
    ("Host".to_string(), "example.com".to_string()),
    ("User-Agent".to_string(), "curl".to_string()),
];
```

每个头部都要分配字符串，原始数据就在那里，我们不需要复制一遍？

## BufRef的思路

```rust
pub struct BufRef(pub usize, pub usize);  // 只存索引，不存数据
```

- 原始HTTP请求在一个连续的`Bytes`里
- BufRef只记录"Host字段在第23-27位置，值在29-40位置"
- 需要用时再切片：`buffer.slice(23..27)`

实际例子：
```rust
// 整个请求在一个buffer里
let buffer = "GET /api HTTP/1.1\r\nHost: example.com\r\nUser-Agent: curl\r\n\r\n";

// KVRef只存位置信息
let host_ref = KVRef::new(23, 4, 29, 11);  // Host: example.com的位置

// 用的时候才切出来，零拷贝
let host_value = host_ref.get_value_bytes(&buffer);
```

## 证书解析的相同思路

`WrappedX509`有同样的模式：

```rust
#[self_referencing]
struct WrappedX509 {
    raw_cert: Vec<u8>,           // 原始证书字节
    cert: X509Certificate<'this>, // 解析结构，借用raw_cert
}
```

证书的组织名、序列号这些字段其实都在原始DER字节里，解析器只是找到对应位置，不需要复制出来单独存储。

## 设计的核心思考

这种设计解决了两个痛点：

**1. 避开生命周期地狱**
以前写自引用结构总是各种生命周期报错，用索引就没这个问题。

**2. 内存效率**
一个HTTP请求可能有几十个头部，全部复制vs全部引用，内存差距很大。