# Pingora Protocol Layer Study Notes

## Understanding the Layer Hierarchy

```bash
HTTP Application Layer
    ↓
TLS Security Layer (Optional OpenSSL/BoringSSL/Rustls, or no-op version)
    ↓  
L4 Transport Layer (TCP/UDP)
    ↓
OS Network Stack (IP/Ethernet etc.)
Hardware Network Interface
```

## IO trait - Compositional Design

```rust
pub trait IO: AsyncRead + AsyncWrite + Shutdown + UniqueID + Ssl + 
              GetTimingDigest + GetProxyDigest + GetSocketDigest + 
              Peek + Unpin + Debug + Send + Sync
```

First impression of this trait definition was quite striking, combining so many traits. Benefits:
- Not defining one giant trait, but splitting capabilities into small traits
- Each small trait has specific responsibilities: Ssl manages TLS info, UniqueID manages connection identity...
- Finally using `Stream = Box<dyn IO>` for type erasure, so upper layer code doesn't need to care whether it's TCP or TLS connection

## Cross-platform Conditional Compilation Strategy

```rust
#[cfg(unix)]
pub type UniqueIDType = i32;
#[cfg(windows)] 
pub type UniqueIDType = usize;
```

The Windows WinSock wrapper exemplifies this approach - essentially wrapping C APIs into safe Rust interfaces.

## OnceCell

```rust
pub peer_addr: OnceCell<Option<SocketAddr>>,
```

- Address info requires system calls, which have overhead
- But most of the time it might not be used
- OnceCell solves this perfectly: fetch when needed, then cache

## CONNECT Protocol Implementation Details

Found several interesting points in the code:

IPv6 address handling:
```rust
let authority = if host.parse::<std::net::Ipv6Addr>().is_ok() {
    format!("[{host}]:{port}")  // Auto-add brackets
} else {
    format!("{host}:{port}")
}
```

CONNECT request format is special:
```rust
// Only needs authority, not complete URI path
if let Some(path) = req.uri.authority() {
    buf.put_slice(path.as_str().as_bytes());
}
```

Directly rejects Transfer-Encoding, "internal use must be strict".

## Type Safety Practices for Windows API Wrapping

```rust
let sockaddr = *(storage as *const _ as *const SOCKADDR_IN);
(
    sockaddr.sin_addr.S_un.S_addr.to_ne_bytes(),
    sockaddr.sin_port.to_be(),
)
```

This code shows how to safely handle C FFI:
- First check memory size `assert!(len >= mem::size_of::<SOCKADDR_IN>())`
- Then do type conversion
- Handle byte order: network order→host order, note big-endian handling for port numbers

## Diagnostic System Design

```rust
pub struct Digest {
    pub ssl_digest: Option<Arc<SslDigest>>,
    pub timing_digest: Vec<Option<TimingDigest>>,  // This Vec is interesting
    pub proxy_digest: Option<Arc<ProxyDigest>>,
    pub socket_digest: Option<Arc<SocketDigest>>,
}
```

timing_digest uses Vec because the protocol stack has multiple layers, each may have its own timing info.
Uses Arc for sharing across multiple places without copying.
Trait interface design is also practical: Get/Set separation, default returns None, allowing types that don't need diagnostics to have zero-cost implementation.

## Error Handling with Context Preservation

```rust
pub struct ConnectProxyError {
    pub response: Box<ResponseHeader>,  // Preserve complete response info
}
```

Instead of simply returning "connection failed", it preserves the proxy server's complete response:

- Error info is rich enough for debugging
- Custom error types are more meaningful than generic errors

## WebSocket Upgrade Technical Details

WebSocket must establish connection through HTTP handshake, solving:
- Port reuse (80/443)
- Firewall penetration
- Existing load balancer compatibility

After upgrade, data on the same TCP connection is parsed according to WebSocket frame format.

## Layered Error Retry Strategy

```rust
if true_io_error {
    err.retry = RetryType::ReusedOnly;
}
```

Distinguishes network IO errors (retryable) from TLS/certificate errors (non-retryable). Reused connection failures are usually due to intermediate device timeouts; new connections might succeed.

## HTTP/2 Connection Health Detection

```rust
const PING_TIMEOUT: Duration = Duration::from_secs(5);
```

HTTP/2 connections might be "fake alive": TCP connection exists but H2 frames cannot transmit. ping-pong mechanism detects actual availability.

## Protocol Switch State Cleanup

```rust
if self.upgraded && !self.body_reader.body_done() {
    self.body_reader.init_content_length(0, b"");
}
```

After WebSocket upgrade succeeds, forcibly terminate HTTP body parser to prevent protocol confusion.

## Chunk Encoding Streaming Parse Details

### Problem Scenario

HTTP chunked encoding format:
```
3\r\n
abc\r\n
0\r\n
\r\n
```

Chunk size line might be split by TCP packets:
```
First read(): "3\r"
Second read(): "\nabc\r\n0\r\n\r\n"
```

### Code Analysis

```rust
body_buf.copy_within(existing_buf_end - expecting_from_io..existing_buf_end, 0);
let new_bytes = stream.read(&mut body_buf[expecting_from_io..]).await?;
```

**Parameter meanings**:
- `existing_buf_end`: Current buffer data end position
- `expecting_from_io`: Length of incomplete chunk header

**Operation steps**:
1. `copy_within()`: Move incomplete chunk header to buffer beginning
2. `read()`: Continue reading after the moved data
3. Concatenate into complete chunk header for parsing

**Specific example**:
```
Initial state: buf = ['3', '\r', 'x', 'x', 'x']
               existing_buf_end = 2, expecting_from_io = 2

copy_within: buf = ['3', '\r', 'x', 'x', 'x']
             Move range [0..2] to position 0 (no change)

read new data: buf = ['3', '\r', '\n', 'a', 'b']
               Start writing new data at position 2

Parse complete: "3\r\n" -> chunk size 3
```

This avoids buffer reallocation, maintaining efficient streaming parse.

## HTTP Compression Algorithm Engineering Choices

### Algorithm Characteristics Comparison

**Brotli**: High compression ratio, high CPU consumption
**Gzip**: Good compatibility, medium speed, standard algorithm
**Zstd**: Fastest speed, compression ratio between the two, limited support

### Memory Management DoS Protection

```rust
const MAX_INIT_COMPRESSED_SIZE_CAP: usize = 4 * 1024;
const ESTIMATED_COMPRESSION_RATIO: usize = 4;

let reserve_size = if input.len() < MAX_INIT_COMPRESSED_SIZE_CAP {
    input.len() * ESTIMATED_COMPRESSION_RATIO
} else {
    input.len()
};
```

Prevents memory amplification attacks from malicious input.

### Streaming Compression State Handling

```rust
if end {
    self.compress.flush()
        .or_err(COMPRESSION_ERROR, "while decompress Brotli")?;
}
Ok(std::mem::take(self.compress.get_mut()).into())
```

`end` flag triggers final flush, `mem::take()` avoids memory copying.

### Thread Safety Design Trade-offs

```rust
// Zstd needs Mutex because Encoder is not Sync
compress: Mutex<Encoder<'static, Vec<u8>>>,
```

Some compression libraries lack sufficient thread safety, requiring runtime protection.

### Practical Application Scenarios

Proxy servers face format conversion needs: clients request gzip but servers return brotli, or CDNs only support gzip requiring recompression. The unified `Encode` trait makes format conversion transparent.