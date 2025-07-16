# HTTP Header Function Encapsulation Purpose

## Core Constructors

**build() vs build_no_case()**
- `build()`: Proxy scenarios, preserve original case
- `build_no_case()`: HTTP/2 scenarios, save memory

## Header Operation Functions

**append_header() vs insert_header()**
- `append_header()`: Add multi-value headers (e.g., Set-Cookie)
- `insert_header()`: Replace same-name headers (e.g., Content-Length)

**remove_header()**
- Clean both case_map and value_map, maintain consistency

## Path Handling Functions

**set_uri() vs set_raw_path()**
- `set_uri()`: Standard UTF-8 paths
- `set_raw_path()`: Support non-UTF-8 bytes for transparent proxy forwarding

**raw_path()**
- Return raw bytes, prioritize fallback then standard path

## Serialization Functions

**header_to_h1_wire()**
- HTTP/1.1 format output, preserve original case
- Use predefined title-case format when no case_map

## HTTP/2 Specific Functions

**set_send_end_stream() / send_end_stream()**
- Control whether HTTP/2 HEADERS frame carries END_STREAM flag

## Response Specific Functions

**set_reason_phrase() / get_reason_phrase()**
- Custom status code descriptions, default to standard descriptions to save memory

**set_content_length()**
- Convenient Content-Length header setting

## Compatibility Functions

**as_owned_parts() / Deref implementation**
- Seamless integration with standard http crate

**From/Into trait implementations**
- Type conversion, maintain API compatibility

All functions designed around "transparent proxy" core requirement - allowing HTTP traffic to completely "pass through" the proxy with upstream/downstream unable to detect proxy presence.