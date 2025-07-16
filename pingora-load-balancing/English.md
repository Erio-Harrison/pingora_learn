# Pingora Load Balancing Data Flow Analysis

## Complete Data Flow

```
Request → Selection Algorithm → Weighted Backend → Iterator → Backend
   ↓              ↓                    ↓              ↓         ↓
[bytes]      [u64 index]      [weight expansion]  [fallback]  [target]
```

## Layer 1: Algorithm Selection

### Hash-based (Sticky Sessions)
```rust
Request: "user123" → Hash → 0x1a2b3c4d → Backend[1] (consistent)
Request: "user123" → Hash → 0x1a2b3c4d → Backend[1] (same)
```

### Round Robin (Fair Distribution)
```rust
Request 1 → Counter(0) → Backend[0]
Request 2 → Counter(1) → Backend[1]  
Request 3 → Counter(2) → Backend[2]
Request 4 → Counter(3) → Backend[0] (wrap around)
```

### Random (Load Spreading)
```rust
Request 1 → Random(0x4f2a) → Backend[2]
Request 2 → Random(0x8b1c) → Backend[0]
Request 3 → Random(0x3e5d) → Backend[1]
```

## Layer 2: Weight Expansion

### Backend Configuration
```rust
Backend A: weight=1  → [A]
Backend B: weight=3  → [B, B, B]  
Backend C: weight=2  → [C, C]
// Weighted array: [A, B, B, B, C, C]
```

### First Selection (Weighted)
```rust
Algorithm → index=4 → weighted[4 % 6] → Backend C
```

## Layer 3: Fallback Strategy

### Iterator Pattern
```rust
WeightedIterator {
    first: true,     // Use weighted selection
    index: 0x1a2b,   // From algorithm
    backend: Arc<Weighted<H>>
}
```

### Selection Flow
```rust
// First call - weighted selection
next() → first=true → weighted[index % weighted.len()] → Backend B

// Subsequent calls - uniform fallback  
next() → first=false → backends[new_index % backends.len()] → Backend A
next() → first=false → backends[new_index % backends.len()] → Backend C
```

## Layer 4: Consistent Hashing Special Case

### Ketama Ring Structure
```rust
Hash Ring: [0x1111→A, 0x3333→B, 0x5555→C, 0x7777→A, 0x9999→B]
```

### Ring Traversal
```rust
Key "user123" → Hash 0x4000 → Find next: 0x5555→C
If C fails → Continue ring: 0x7777→A  
If A fails → Continue ring: 0x9999→B
```

## Performance Characteristics

| Algorithm | Time Complexity | Memory | Consistency |
|-----------|----------------|--------|-------------|
| Hash      | O(1)          | O(n)   | High        |
| RoundRobin| O(1)          | O(n)   | None        |
| Random    | O(1)          | O(n)   | None        |
| Consistent| O(log n)      | O(n×k) | Medium      |

## Failure Handling Data Flow

```
Primary Selection → Health Check → Failure
        ↓              ↓           ↓
    Iterator.next() → Backend B → Unavailable
        ↓              ↓           ↓
    Iterator.next() → Backend A → Success
        ↓              ↓           ↓
    Route Request  → Process   → Response
```

## Key Design Benefits

1. **Separation of Concerns**: Algorithm generates index, Weighted handles distribution
2. **Graceful Degradation**: Weighted first selection, uniform fallback
3. **Pluggable Algorithms**: Any Hasher can be used as selection algorithm
4. **Atomic Operations**: Thread-safe without locks (RoundRobin)
5. **Consistent Interface**: All algorithms return u64 for uniform handling