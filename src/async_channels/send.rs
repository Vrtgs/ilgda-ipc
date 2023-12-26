use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::thread;
use crossbeam::channel::TrySendError;
use ipc_channel::ipc::IpcSender;
use serde::{Deserialize, Serialize};
use super::impl_future;

enum SenderMessage<T> {
    Send(T, tokio::sync::oneshot::Sender<Result<(), ipc_channel::Error>>),
    Shutdown
}

pub struct AsyncIpcSender<T> {
    sender: crossbeam::channel::Sender<SenderMessage<T>>,
}

impl<T> AsyncIpcSender<T>
    where T: 'static + Send + for<'de> Deserialize<'de> + Serialize
{
    pub fn new(channel: IpcSender<T>) -> Self {
        let (sender, receiver) =
            // this should be enough for, shutdown
            // and any outgoing future we have
            crossbeam::channel::bounded(2);

        thread::spawn(move || {
            while let Ok(SenderMessage::Send(data, result_send)) = receiver.recv() {
                let _ = result_send.send(channel.send(data));
            }
        });

        Self { sender }
    }

    pub fn send(&mut self, data: T) -> IpcSendFuture<T> {
        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();

        match self.sender.try_send(SenderMessage::Send(data, result_sender)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => unreachable!(
                "the previous future needs to have been completed \
                or else we cant borrow our selves again"
            ),
            Err(TrySendError::Disconnected(_)) => unreachable!("ipc thread died unexpectedly")
        }

        IpcSendFuture {
            receiver: result_receiver,
            parent: PhantomData,
        }
    }
}


impl<T> Drop for AsyncIpcSender<T> {
    fn drop(&mut self) {
        let _ = self.sender.try_send(SenderMessage::Shutdown);
    }
}

impl_future! { AsyncIpcSender |> IpcSendFuture |> Result<(), ipc_channel::Error> }