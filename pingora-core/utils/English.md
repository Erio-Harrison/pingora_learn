# Zero-Copy Buffer Reference Design Study Notes

## Problems Encountered

Poor parser implementation:

```rust
// Anti-pattern example
let headers: Vec<(String, String)> = vec![
    ("Host".to_string(), "example.com".to_string()),
    ("User-Agent".to_string(), "curl".to_string()),
];
```

Each header requires string allocation, even though the original data is already available. Why copy it?

## BufRef Approach

```rust
pub struct BufRef(pub usize, pub usize);  // Store only indices, not data
```

- The original HTTP request is in a contiguous `Bytes` buffer.
- `BufRef` only records positions, e.g., "Host field at positions 23-27, value at 29-40".
- Slice the buffer only when needed: `buffer.slice(23..27)`.

Example:
```rust
// Entire request in one buffer
let buffer = "GET /api HTTP/1.1\r\nHost: example.com\r\nUser-Agent: curl\r\n\r\n";

// KVRef stores only position info
let host_ref = KVRef::new(23, 4, 29, 11);  // Positions for Host: example.com

// Slice out data on demand, zero-copy
let host_value = host_ref.get_value_bytes(&buffer);
```

## Certificate Parsing with the Same Approach

`WrappedX509` follows a similar pattern:

```rust
#[self_referencing]
struct WrappedX509 {
    raw_cert: Vec<u8>,           // Raw certificate bytes
    cert: X509Certificate<'this>, // Parsed structure, borrows raw_cert
}
```

Fields like organization name or serial number are in the raw DER bytes; the parser only locates them without copying.

## Core Design Considerations

This design addresses two pain points:

**1. Avoiding Lifetime Hell**  
Self-referential structs often cause lifetime errors. Using indices eliminates this issue.

**2. Memory Efficiency**  
An HTTP request may have dozens of headers. Copying all vs. referencing them makes a significant memory difference.