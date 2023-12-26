mod recv;
mod send;

pub use recv::*;
pub use send::*;

macro_rules! impl_future {
    ($parent:ident |> $name:ident |> $ty:ty) => {
        #[must_use = "futures do nothing unless you `.await` or poll them"]
        pub struct $name<'a, T> {
            receiver: tokio::sync::oneshot::Receiver<$ty>,
            parent: PhantomData<&'a mut $parent<T>>
        }

        impl<'a, T> Future for $name<'a, T> {
            type Output = $ty;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let this = Pin::into_inner(self);

                match Pin::new(&mut this.receiver).poll(cx) {
                    Poll::Ready(res) => match res {
                        Ok(res) => Poll::Ready(res),
                        Err(_) => unreachable!("ipc thread died unexpectedly")
                    },
                    Poll::Pending => Poll::Pending
                }
            }
        }
    };
}

pub(crate) use impl_future;