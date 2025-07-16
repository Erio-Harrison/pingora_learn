# 学习Pingora Runtime的一些心得（[pingora_runtime](https://docs.rs/pingora-runtime/latest/pingora_runtime/)）

## 问题的起点

Tokio有两种运行时：
- 单线程：效率高但用不了多核
- 多线程工作窃取：能用多核但有调度开销

工作窃取就是一个线程没活干了，会去偷别的线程的任务。听起来很聪明，但实际上这个"偷"的过程有不少开销 - 要同步，要迁移任务，还会搞乱缓存。

## Pingora的思路

既然工作窃取有开销，那就不要窃取了。创建多个独立的单线程运行时，每个跑在不同的OS线程上。

说白了就是：
```rust
// 不是这样的真正多线程
Builder::new_multi_thread()

// 而是多个这样的单线程
for _ in 0..threads {
    Builder::new_current_thread()
}
```

任务分发就用随机选择，简单粗暴但够用。

## 代码里发现的资源浪费

`get_pools()`里有一个TODO（TODO: use a mutex to avoid creating a lot threads only to drop them）：

```rust
// 原始代码
let (pools, controls) = self.init_pools(); // 已经创建OS线程了
match self.pools.try_insert(pools) {
    Ok(p) => { /* 成功 */ }
    Err((p, _my_pools)) => p, // 失败了，但OS线程已经在跑了
}
```

问题是`init_pools()`里的`std::thread::spawn()`会立即创建OS线程。如果多个线程同时调用，只有一个能成功插入到`OnceCell`，其他的被丢弃。

虽然被丢弃的线程最终会收到`Err`信号正常退出，但这浪费了创建和销毁线程的开销。

## 修复方案

用double-checked locking避免重复创建：

```rust
fn get_pools(&self) -> &[Handle] {
    if let Some(p) = self.pools.get() {
        p
    } else {
        let _guard = self.init_lock.lock().unwrap();
        if let Some(p) = self.pools.get() {
            return p; // 可能在等锁时被初始化了
        }
        let (pools, controls) = self.init_pools();
        self.pools.set(pools).unwrap();
        self.controls.set(controls).unwrap();
        self.pools.get().unwrap()
    }
}
```

这样就保证只有一个线程会真正执行初始化，避免了不必要的资源开销。

## 为什么这样做有效

对于代理这种场景，连接处理比较均匀，不需要复杂的负载均衡。随机分发就够了，还避免了工作窃取的各种开销。

Cloudflare的数据显示效果确实不错 - 延迟降了，资源用得也少了。

## 一些思考

有时候简单的方案反而更有效。复杂的调度算法在特定场景下可能得不偿失。关键是要理解自己的工作负载特点，然后针对性优化。

## 附上数据传递链路

### 1. 创建阶段
```
Runtime::new_no_steal(threads, name) 
-> NoStealRuntime::new(threads, name)
-> pools: Arc::new(OnceCell::new()) (空)
-> controls: OnceCell::new() (空)
```

### 2. 首次使用
```
runtime.get_handle()
-> NoStealRuntime::get_runtime()
-> self.get_runtime_at(random_index)
-> self.get_pools()
-> init_lock.lock() (获得锁)
-> self.init_pools()
```

### 3. 初始化过程
```
init_pools()
-> 循环创建threads个:
   -> tokio::runtime::Builder::new_current_thread().build()
   -> rt.handle().clone() -> handler
   -> oneshot::channel() -> (tx, rx)
   -> pools_ref = self.pools.clone()
   -> std::thread::spawn(move ||{
        CURRENT_HANDLE.get_or(|| pools_ref)
        rt.block_on(rx) // 等待关闭信号
      })
   -> pools.push(handler)
   -> controls.push((tx, join))
-> return (pools, controls)
```

### 4. 设置共享状态
```
init_pools() 返回 (pools, controls)
-> self.pools.set(pools)
-> self.controls.set(controls)
-> pools传播到所有工作线程的CURRENT_HANDLE
```

### 5. 任务分发
```
current_handle()
-> CURRENT_HANDLE.get() (在工作线程中)
-> pools.get().unwrap()
-> 随机选择index
-> pools[index].clone() -> Handle
-> handle.spawn(task) -> 任务提交到随机线程
```

### 6. 关闭流程
```
runtime.shutdown_timeout(timeout)
-> self.controls.take() -> Vec<(Sender, JoinHandle)>
-> 分离为(txs, joins)
-> 遍历txs: tx.send(timeout) -> 工作线程收到关闭信号
-> 工作线程: rt.block_on(rx) 收到Ok(timeout)
-> 工作线程: rt.shutdown_timeout(timeout)
-> 主线程: join.join() 等待所有线程退出
```

核心是`Arc<OnceCell<Box<[Handle]>>>`在主线程和工作线程间共享句柄池。