# Pingora Timeout Learning Notes ([pingora_timeout](https://docs.rs/pingora-timeout/latest/pingora_timeout/))

## Interesting Design Points

### Lazy Initialization
```rust
if let Poll::Ready(v) = me.value.poll(cx) {
    return Poll::Ready(Ok(v)); // immediate return, no timer overhead
}
```
Straightforward approach - operations that complete quickly don't need timer creation at all.

### Time Bucketing Trade-off
```rust
// 1001ms, 1002ms, 1009ms -> all become 1010ms, shared Timer
fn round_to(raw: u128, resolution: u128) -> u128 {
    raw - 1 + resolution - (raw - 1) % resolution
}
```
Trading 10ms precision for massive timer reuse - reasonable compromise for network scenarios.

### ThreadLocal Avoids Lock Contention
```rust
timers: ThreadLocal<RwLock<BTreeMap<Time, Timer>>>
```
Per-thread timer trees, only clock thread needs write locks for deletion. Smarter than global locks.

### Arc Ensures Notification Delivery
```rust
TimerStub(Arc<Notify>, Arc<AtomicBool>)
```
Even if Timer gets deleted, subscribers can still receive notifications.

### Race Condition Handling
```rust
if self.1.load(Ordering::SeqCst) {
    return; // check if already fired first
}
self.0.notified().await;
```
Prevents missing already-fired timers during wait.

### Double-Checked Locking Pattern
```rust
let timers = self.timers.get_or(...).read();
if let Some(t) = timers.get(&now) {
    return t.subscribe(); // reuse under read lock
}
// release read lock, acquire write lock for creation
```
Minimizes lock hold time.

## Performance Gains Are Significant

- Creation: 343ns → 78ns
- Destruction: 106ns → 13ns  
- Overall: 107ns → 4ns

26x improvement, primarily from timer sharing and lock-free design.

## When to Use

Based on call frequency:
- High frequency (>100/sec): use fast_timeout, clock thread overhead amortizes
- Low frequency: use tokio native, avoid extra thread overhead

Proxy servers fit well - multiple timeout points per connection, high call frequency.