# Pingora Timeout 学习笔记([pingora_timeout](https://docs.rs/pingora-timeout/latest/pingora_timeout/))

## 我觉得有意思的设计点

### 懒初始化思路
```rust
if let Poll::Ready(v) = me.value.poll(cx) {
    return Poll::Ready(Ok(v)); // 立即返回，无timer开销
}
```
这个很直接，对于那些本来就能快速完成的操作，根本不需要创建timer。

### 时间分片的权衡
```rust
// 1001ms, 1002ms, 1009ms -> 都变成1010ms，共享Timer
fn round_to(raw: u128, resolution: u128) -> u128 {
    raw - 1 + resolution - (raw - 1) % resolution
}
```
用10ms的精度损失换来大量timer复用，这个权衡在网络场景下很合理。

### ThreadLocal避免锁竞争
```rust
timers: ThreadLocal<RwLock<BTreeMap<Time, Timer>>>
```
每线程独立timer树，只有时钟线程删除时才需要写锁。比全局锁聪明。

### Arc保证通知可达
```rust
TimerStub(Arc<Notify>, Arc<AtomicBool>)
```
即使Timer被删除，订阅者仍能收到通知。

### 竞态条件的处理
```rust
if self.1.load(Ordering::SeqCst) {
    return; // 先检查是否已触发
}
self.0.notified().await;
```
避免在等待过程中错过已触发的timer。

### 双检查锁模式
```rust
let timers = self.timers.get_or(...).read();
if let Some(t) = timers.get(&now) {
    return t.subscribe(); // 读锁下复用
}
// 释放读锁，获取写锁创建
```
最小化锁持有时间。

## 性能提升确实明显

- 创建：343ns → 78ns
- 销毁：106ns → 13ns  
- 总体：107ns → 4ns

26倍的提升，主要来自timer共享和无锁设计。

## 何时使用

看调用频率：
- 高频(>100次/秒)：用fast_timeout，时钟线程开销能摊薄
- 低频：用tokio原生，避免额外线程开销

代理服务器这种场景很适合，每个连接多个超时点，调用频率高。