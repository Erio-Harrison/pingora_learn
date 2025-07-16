# HTTP Header Compression Implementation Details

## 1. Memory Optimization

### ThreadLocal Buffer Pool
```rust
buf: ThreadLocal<RefCell<Vec<u8>>>

// Reuse buffers, avoid repeated allocation
let mut buf = self.buf
    .get_or(|| RefCell::new(Vec::with_capacity(MAX_HEADER_SIZE)))
    .borrow_mut();
buf.clear(); // Reset instead of reallocate
```

### Pre-allocated Capacity
```rust
const MAX_HEADER_SIZE: usize = 64 * 1024;
Vec::with_capacity(MAX_HEADER_SIZE) // One-time allocation, avoid expansion
```

### Compression Context Reuse
```rust
// One compressor per thread, avoid repeated creation
fn get_com_context(&self) -> RefMut<CCtx<'static>> {
    self.com_context
        .get_or(|| RefCell::new(CCtx::create()))
        .borrow_mut()
}
```

## 2. Reduce Network Transmission

### Dictionary Compression Optimization
```rust
pub struct CompressionWithDict {
    com_dict: CDict<'static>,  // Pre-trained compression dictionary
    de_dict: DDict<'static>,   // Pre-trained decompression dictionary
}

// Dictionary training identifies repeated patterns
pub fn train(dir_path: P) -> Vec<u8> {
    dict::from_files(files, 64 * 1024 * 1024).unwrap()
}
```

### Compression Ratio Optimization
```rust
const COMPRESS_LEVEL: i32 = 3; // Balance compression ratio and speed

// Tests show approximately 1/3 compression ratio
assert!(compressed.len() < uncompressed);
assert!(compressed.len() < compressed_no_dict.len());
```

### Serialization Format
```rust
fn resp_header_to_buf(resp: &ResponseHeader, buf: &mut Vec<u8>) {
    // HTTP/1.1 format serialization
    buf.put_slice(version.as_bytes());
    buf.put_slice(status.as_str().as_bytes());
    resp.header_to_h1_wire(buf);
}
```

## 3. Improve Cache Efficiency

### Unified Compression Interface
```rust
enum ZstdCompression {
    Default(Compression, i32),     // Without dictionary
    WithDict(CompressionWithDict), // With dictionary
}

// Unified API simplifies usage
impl HeaderSerde {
    pub fn serialize(&self, header: &ResponseHeader) -> Result<Vec<u8>>
    pub fn deserialize(&self, data: &[u8]) -> Result<ResponseHeader>
}
```

### Error Handling
```rust
fn into_error(e: &'static str, context: &'static str) -> Box<Error> {
    Error::because(ErrorType::InternalError, context, e)
}

// Unified error conversion
.map_err(|e| into_error(e, "decompress header"))
```

### Parsing Optimization
```rust
const MAX_HEADERS: usize = 256;
let mut headers = vec![httparse::EMPTY_HEADER; MAX_HEADERS];

// Fast HTTP parsing
match resp.parse(buf) {
    Ok(httparse::Status::Complete(_)) => parsed_to_header(&resp),
}
```

## Core Advantages
- **Memory Reuse**: ThreadLocal avoids allocation overhead
- **Dictionary Optimization**: Pre-trained patterns improve compression ratio
- **Cache Friendly**: Unified API reduces integration complexity