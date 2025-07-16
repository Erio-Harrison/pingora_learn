# Learning Pingora Runtime: Key Insights（[pingora_runtime](https://docs.rs/pingora-runtime/latest/pingora_runtime/)）

## The Problem

Tokio has two runtime types:
- Single-threaded: Efficient but can't use multiple cores
- Multi-threaded work-stealing: Uses multiple cores but has scheduling overhead

Work-stealing means idle threads steal tasks from busy ones. While clever, the "stealing" process creates overhead - synchronization, task migration, and cache disruption.

## Pingora's Approach

Instead of work-stealing, create multiple independent single-threaded runtimes, each on separate OS threads.

```rust
// Not multi-threaded like this
Builder::new_multi_thread()

// But multiple single-threaded like this
for _ in 0..threads {
    Builder::new_current_thread()
}
```

Task distribution uses random selection - simple but effective.

## Resource Waste in Code

Found a TODO in `get_pools()`: "use a mutex to avoid creating a lot threads only to drop them"

```rust
// Original code
let (pools, controls) = self.init_pools(); // OS threads already created
match self.pools.try_insert(pools) {
    Ok(p) => { /* success */ }
    Err((p, _my_pools)) => p, // failed, but OS threads already running
}
```

Problem: `init_pools()` calls `std::thread::spawn()` which immediately creates OS threads. When multiple threads call simultaneously, only one succeeds in inserting to `OnceCell`, others are discarded.

Discarded threads eventually receive `Err` signals and exit cleanly, but this wastes thread creation/destruction overhead.

## Fix Solution

Use double-checked locking to avoid duplicate creation:

```rust
fn get_pools(&self) -> &[Handle] {
    if let Some(p) = self.pools.get() {
        p
    } else {
        let _guard = self.init_lock.lock().unwrap();
        if let Some(p) = self.pools.get() {
            return p; // might have been initialized while waiting for lock
        }
        let (pools, controls) = self.init_pools();
        self.pools.set(pools).unwrap();
        self.controls.set(controls).unwrap();
        self.pools.get().unwrap()
    }
}
```

This ensures only one thread performs initialization, avoiding unnecessary resource overhead.

## Why This Works

For proxy scenarios, connection handling is relatively uniform - no complex load balancing needed. Random distribution suffices while avoiding work-stealing overhead.

Cloudflare's data shows good results - lower latency and reduced resource usage.

## Key Takeaway

Simple solutions are often more effective. Complex scheduling algorithms may not pay off in specific scenarios. The key is understanding your workload characteristics and optimizing accordingly.

## Data Flow Chain

### 1. Creation Phase
```
Runtime::new_no_steal(threads, name)
-> NoStealRuntime::new(threads, name)
-> pools: Arc::new(OnceCell::new()) (empty)
-> controls: OnceCell::new() (empty)
```

### 2. First Use
```
runtime.get_handle()
-> NoStealRuntime::get_runtime()
-> self.get_runtime_at(random_index)
-> self.get_pools()
-> init_lock.lock() (acquire lock)
-> self.init_pools()
```

### 3. Initialization Process
```
init_pools()
-> Loop creating threads count:
   -> tokio::runtime::Builder::new_current_thread().build()
   -> rt.handle().clone() -> handler
   -> oneshot::channel() -> (tx, rx)
   -> pools_ref = self.pools.clone()
   -> std::thread::spawn(move ||{
        CURRENT_HANDLE.get_or(|| pools_ref)
        rt.block_on(rx) // wait for shutdown signal
      })
   -> pools.push(handler)
   -> controls.push((tx, join))
-> return (pools, controls)
```

### 4. Set Shared State
```
init_pools() returns (pools, controls)
-> self.pools.set(pools)
-> self.controls.set(controls)
-> pools propagate to all worker threads' CURRENT_HANDLE
```

### 5. Task Distribution
```
current_handle()
-> CURRENT_HANDLE.get() (in worker thread)
-> pools.get().unwrap()
-> random select index
-> pools[index].clone() -> Handle
-> handle.spawn(task)
-> task submitted to random thread
```

### 6. Shutdown Flow
```
runtime.shutdown_timeout(timeout)
-> self.controls.take()
-> Vec<(Sender, JoinHandle)>
-> separate into (txs, joins)
-> iterate txs: tx.send(timeout)
-> worker thread receives shutdown signal
-> worker thread: rt.block_on(rx) receives Ok(timeout)
-> worker thread: rt.shutdown_timeout(timeout)
-> main thread: join.join() wait for all threads to exit
```

Core concept: `Arc<OnceCell<Box<[Handle]>>>` shares handle pool between main and worker threads.