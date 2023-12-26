# ilgda-ipc
IPC standard used for ilgda

temp readme will fix soon™

# Example
```rust
use tokio::join;
use ilgda_ipc::async_channels::{AsyncIpcReceiver, AsyncIpcSender};

#[tokio::main]
async fn main() {
let (tx, rx) = ipc_channel::ipc::channel().unwrap();

    let mut tx = AsyncIpcSender::new(tx);
    let mut rx = AsyncIpcReceiver::new(rx);

    join!(
        async { tx.send(u128::MAX).await.expect("failed to send data") },
        async { assert_eq!(u128::MAX, rx.recv().await.expect("failed to receive")) }
    );
}
```