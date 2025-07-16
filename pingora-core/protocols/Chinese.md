# Pingora 协议层学习笔记

## 先理解下层级关系

```bash
HTTP应用层
    ↓
TLS安全层 (可选OpenSSL/BoringSSL/Rustls，或no-op版本)
    ↓  
L4传输层 (TCP/UDP)
    ↓
操作系统网络栈 (IP/以太网等)
硬件网络接口
```

## IO trait - 组合式设计

```rust
pub trait IO: AsyncRead + AsyncWrite + Shutdown + UniqueID + Ssl + 
              GetTimingDigest + GetProxyDigest + GetSocketDigest + 
              Peek + Unpin + Debug + Send + Sync
```

第一眼看到这个trait定义时有点小震撼，组合了这么多trait。AI总结了下好处：
- 不是定义一个巨大的trait，而是把能力拆分成小trait
- 每个小trait都有具体职责：Ssl管TLS信息，UniqueID管连接标识...
- 最后用`Stream = Box<dyn IO>`做类型擦除，上层代码就不用关心具体是TCP还是TLS连接了

## 跨平台的条件编译策略

```rust
#[cfg(unix)]
pub type UniqueIDType = i32;
#[cfg(windows)] 
pub type UniqueIDType = usize;
```

Windows那段WinSock封装就是这个思路的体现，本质就是把C的API包装成Rust安全接口。

## OnceCell

```rust
pub peer_addr: OnceCell<Option<SocketAddr>>,
```

- 地址信息需要系统调用获取，有开销
- 但大部分时候可能都不会用到
- OnceCell完美解决：需要时才获取，获取后就缓存

## CONNECT协议的实现细节

看代码时发现几个有意思的点：

IPv6地址处理：
```rust
let authority = if host.parse::<std::net::Ipv6Addr>().is_ok() {
    format!("[{host}]:{port}")  // 自动加方括号
} else {
    format!("{host}:{port}")
}
```

CONNECT请求格式特殊：
```rust
// 只需要authority，不需要完整URI路径
if let Some(path) = req.uri.authority() {
    buf.put_slice(path.as_str().as_bytes());
}
```

直接拒绝Transfer-Encoding，"内部使用要严格"。

## Windows API封装的类型安全实践

```rust
let sockaddr = *(storage as *const _ as *const SOCKADDR_IN);
(
    sockaddr.sin_addr.S_un.S_addr.to_ne_bytes(),
    sockaddr.sin_port.to_be(),
)
```

这段代码可以看到怎么安全地处理C FFI：
- 先检查内存大小 `assert!(len >= mem::size_of::<SOCKADDR_IN>())`
- 再做类型转换
- 处理字节序：网络序→主机序，注意端口号的大端序处理

## 诊断系统的设计

```rust
pub struct Digest {
    pub ssl_digest: Option<Arc<SslDigest>>,
    pub timing_digest: Vec<Option<TimingDigest>>,  // 这个Vec很有意思
    pub proxy_digest: Option<Arc<ProxyDigest>>,
    pub socket_digest: Option<Arc<SocketDigest>>,
}
```

timing_digest用Vec是因为协议栈有多层，每层都可能有自己的时序信息。
用Arc是为了多处共享而不拷贝。
trait接口设计也很实用：Get/Set分离，默认返回None，让不需要诊断的类型可以零成本实现。

## 错误处理的上下文保留

```rust
pub struct ConnectProxyError {
    pub response: Box<ResponseHeader>,  // 保留完整响应信息
}
```

不是简单返回"连接失败"，而是把代理服务器的响应完整保留下来：

- 错误信息足够丰富，方便调试
- 自定义错误类型比通用错误更有意义

## WebSocket升级的技术细节

WebSocket必须通过HTTP握手建立连接，这解决了：
- 端口复用（80/443）
- 防火墙穿透
- 现有负载均衡器兼容性

升级后同一TCP连接的数据按WebSocket帧格式解析。

## 错误重试的分层策略

```rust
if true_io_error {
    err.retry = RetryType::ReusedOnly;
}
```

区分网络IO错误（可重试）和TLS/证书错误（不可重试）。复用连接失败通常是中间设备超时，新连接可能成功。

## HTTP/2连接健康检测

```rust
const PING_TIMEOUT: Duration = Duration::from_secs(5);
```

HTTP/2连接可能"假活"：TCP连接存在但H2帧无法传输。ping-pong机制检测实际可用性。

## 协议切换的状态清理

```rust
if self.upgraded && !self.body_reader.body_done() {
    self.body_reader.init_content_length(0, b"");
}
```

WebSocket升级成功后强制终止HTTP body解析器，防止协议混乱。

## Chunk编码的流式解析详解

### 问题场景

HTTP chunked encoding格式：
```
3\r\n
abc\r\n
0\r\n
\r\n
```

chunk大小行可能被TCP分包：
```
第一次read(): "3\r"
第二次read(): "\nabc\r\n0\r\n\r\n"
```

### 代码解析

```rust
body_buf.copy_within(existing_buf_end - expecting_from_io..existing_buf_end, 0);
let new_bytes = stream.read(&mut body_buf[expecting_from_io..]).await?;
```

**参数含义**：
- `existing_buf_end`: 当前缓冲区数据末尾位置
- `expecting_from_io`: 不完整chunk头的长度

**操作步骤**：
1. `copy_within()`: 把不完整的chunk头移到缓冲区开头
2. `read()`: 在移动后的数据后面继续读取
3. 拼接成完整的chunk头进行解析

**具体例子**：
```
初始状态: buf = ['3', '\r', 'x', 'x', 'x']
          existing_buf_end = 2, expecting_from_io = 2

copy_within: buf = ['3', '\r', 'x', 'x', 'x']
            移动范围 [0..2] 到位置0 (无变化)

read新数据: buf = ['3', '\r', '\n', 'a', 'b']
          在位置2开始写入新数据

解析完整: "3\r\n" -> chunk大小3
```

这样避免了缓冲区重新分配，保持高效的流式解析。


## HTTP压缩算法的工程选择

### 算法特性对比

**Brotli**: 压缩率高，CPU消耗大
**Gzip**: 兼容性好，速度中等，标准算法
**Zstd**: 速度最快，压缩率介于两者间，支持有限

### 内存管理的DoS防护

```rust
const MAX_INIT_COMPRESSED_SIZE_CAP: usize = 4 * 1024;
const ESTIMATED_COMPRESSION_RATIO: usize = 4;

let reserve_size = if input.len() < MAX_INIT_COMPRESSED_SIZE_CAP {
    input.len() * ESTIMATED_COMPRESSION_RATIO
} else {
    input.len()
};
```

防止恶意输入导致内存放大攻击。

### 流式压缩的状态处理

```rust
if end {
    self.compress.flush()
        .or_err(COMPRESSION_ERROR, "while decompress Brotli")?;
}
Ok(std::mem::take(self.compress.get_mut()).into())
```

`end`标志触发最终刷新，`mem::take()`避免内存拷贝。

### 线程安全的设计权衡

```rust
// Zstd需要Mutex因为Encoder不是Sync
compress: Mutex<Encoder<'static, Vec<u8>>>,
```

部分压缩库线程安全性不足，需要运行时保护。

### 实际应用场景

代理服务器面临格式转换需求：客户端要求gzip但服务器返回brotli，或CDN只支持gzip需要重新压缩。统一的`Encode` trait让格式转换透明化。