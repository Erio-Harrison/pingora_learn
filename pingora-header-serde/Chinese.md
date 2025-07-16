# HTTP Header压缩实现细节

## 1. 节约内存

### ThreadLocal缓冲池
```rust
buf: ThreadLocal<RefCell<Vec<u8>>>

// 复用缓冲区，避免重复分配
let mut buf = self.buf
    .get_or(|| RefCell::new(Vec::with_capacity(MAX_HEADER_SIZE)))
    .borrow_mut();
buf.clear(); // 重置而非重新分配
```

### 预分配容量
```rust
const MAX_HEADER_SIZE: usize = 64 * 1024;
Vec::with_capacity(MAX_HEADER_SIZE) // 一次性分配，避免扩容
```

### 压缩上下文复用
```rust
// 每线程一个压缩器，避免重复创建
fn get_com_context(&self) -> RefMut<CCtx<'static>> {
    self.com_context
        .get_or(|| RefCell::new(CCtx::create()))
        .borrow_mut()
}
```

## 2. 减少网络传输

### 字典压缩优化
```rust
pub struct CompressionWithDict {
    com_dict: CDict<'static>,  // 预训练压缩字典
    de_dict: DDict<'static>,   // 预训练解压字典
}

// 字典训练识别重复模式
pub fn train(dir_path: P) -> Vec<u8> {
    dict::from_files(files, 64 * 1024 * 1024).unwrap()
}
```

### 压缩比优化
```rust
const COMPRESS_LEVEL: i32 = 3; // 平衡压缩率和速度

// 测试显示约1/3压缩比
assert!(compressed.len() < uncompressed);
assert!(compressed.len() < compressed_no_dict.len());
```

### 序列化格式
```rust
fn resp_header_to_buf(resp: &ResponseHeader, buf: &mut Vec<u8>) {
    // HTTP/1.1格式序列化
    buf.put_slice(version.as_bytes());
    buf.put_slice(status.as_str().as_bytes());
    resp.header_to_h1_wire(buf);
}
```

## 3. 提高缓存效率

### 统一压缩接口
```rust
enum ZstdCompression {
    Default(Compression, i32),     // 无字典
    WithDict(CompressionWithDict), // 带字典
}

// 统一API简化使用
impl HeaderSerde {
    pub fn serialize(&self, header: &ResponseHeader) -> Result<Vec<u8>>
    pub fn deserialize(&self, data: &[u8]) -> Result<ResponseHeader>
}
```

### 错误处理
```rust
fn into_error(e: &'static str, context: &'static str) -> Box<Error> {
    Error::because(ErrorType::InternalError, context, e)
}

// 统一错误转换
.map_err(|e| into_error(e, "decompress header"))
```

### 解析优化
```rust
const MAX_HEADERS: usize = 256;
let mut headers = vec![httparse::EMPTY_HEADER; MAX_HEADERS];

// 快速HTTP解析
match resp.parse(buf) {
    Ok(httparse::Status::Complete(_)) => parsed_to_header(&resp),
}
```

## 核心优势
- **内存复用**：ThreadLocal避免分配开销
- **字典优化**：预训练模式提升压缩率
- **缓存友好**：统一API降低集成复杂度