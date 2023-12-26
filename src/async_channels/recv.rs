use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::thread;
use crossbeam::channel::TrySendError;
use ipc_channel::ipc::{IpcError, IpcReceiver, TryRecvError};
use serde::{Deserialize, Serialize};
use super::impl_future;

enum ReceiverMessage<T> {
    Receive(tokio::sync::oneshot::Sender<Result<T, IpcError>>),
    Shutdown
}

pub struct AsyncIpcReceiver<T> {
    sender: crossbeam::channel::Sender<ReceiverMessage<T>>,
}

impl<T> AsyncIpcReceiver<T>
    where T: 'static + Send + for<'de> Deserialize<'de> + Serialize
{
    pub fn new(channel: IpcReceiver<T>) -> Self {
        let (sender, receiver) =
            // this should be enough for us to handle
            // any outgoing future we have, plus 1 slot for if the thread isn't currently receiving
            // and drop cant run at the same time as another future
            crossbeam::channel::bounded(1);

        thread::spawn(move || {
            while let Ok(ReceiverMessage::Receive(send)) = receiver.recv() {
                while !send.is_closed() {
                    match channel.try_recv() {
                        Ok(t) => {
                            let _ = send.send(Ok(t)); break
                        }
                        Err(t) => match t {
                            TryRecvError::Empty => continue,
                            TryRecvError::IpcError(e) => {
                                let _ = send.send(Err(e)); break
                            }
                        }
                    }
                }
            }
        });


        Self { sender }
    }

    pub fn recv(&mut self) -> IpcReceiveFuture<T> {
        let (value_sender, value_receiver) = tokio::sync::oneshot::channel();

        match self.sender.try_send(ReceiverMessage::Receive(value_sender)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => unreachable!(
                "the previous future needs to have been completed \
                or else we cant borrow our selves again"
            ),
            Err(TrySendError::Disconnected(_)) => unreachable!("ipc thread died unexpectedly")
        }

        IpcReceiveFuture {
            receiver: value_receiver,
            parent: PhantomData,
        }
    }
}

impl<T> Drop for AsyncIpcReceiver<T> {
    fn drop(&mut self) {
        let _ = self.sender.try_send(ReceiverMessage::Shutdown);
    }
}

impl_future! { AsyncIpcReceiver |> IpcReceiveFuture |> Result<T, IpcError> }