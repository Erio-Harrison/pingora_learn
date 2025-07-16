# HTTP Header 函数封装目的

## 核心构造函数

**build() vs build_no_case()**
- `build()`: 代理场景，保持原始大小写
- `build_no_case()`: HTTP/2场景，节省内存

## Header操作函数

**append_header() vs insert_header()**
- `append_header()`: 添加多值header (如Set-Cookie)
- `insert_header()`: 替换同名header (如Content-Length)

**remove_header()**
- 同时清理case_map和value_map，保持一致性

## 路径处理函数

**set_uri() vs set_raw_path()**
- `set_uri()`: 标准UTF-8路径
- `set_raw_path()`: 支持非UTF-8字节，代理透明转发

**raw_path()**
- 返回原始字节，优先fallback再取标准路径

## 序列化函数

**header_to_h1_wire()**
- HTTP/1.1格式输出，保持原始大小写
- 无case_map时使用预定义标题化格式

## HTTP/2特有函数

**set_send_end_stream() / send_end_stream()**
- 控制HTTP/2 HEADERS帧是否带END_STREAM标志

## 响应特有函数

**set_reason_phrase() / get_reason_phrase()**
- 自定义状态码描述，默认使用标准描述节省内存

**set_content_length()**
- 便捷设置Content-Length header

## 兼容性函数

**as_owned_parts() / Deref实现**
- 与标准http crate无缝集成

**From/Into trait实现**
- 类型转换，保持API兼容性

所有函数都围绕"透明代理"这一核心需求设计, 目标是让HTTP流量完全"穿透"代理，上下游无法察觉代理的存在。。