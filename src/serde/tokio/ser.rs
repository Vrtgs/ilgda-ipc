mod primitives {
    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::AsyncWrite;

    macro_rules! serializer {
    ($name:ident $ty:ty) => {
        serializer!($name, $ty, std::mem::size_of::<$ty>());
    };
    ($name:ident, $ty:ty, $bytes:expr) => {
        #[must_use = "futures do nothing unless you `.await` or poll them"]
        pub struct $name<W> {
            writer: W,
            buf: [u8; $bytes],
            written: u8,
        }

        impl<W> $name<W> {
            pub fn new(w: W, value: $ty) -> Self {
                Self {
                    writer: w,
                    buf: value.to_ne_bytes(),
                    written: 0
                }
            }
        }

        impl<W: AsyncWrite + Unpin> Future for $name<W> {
            type Output = io::Result<()>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let this = Pin::into_inner(self);

                if this.written == $bytes as u8 {
                    return Poll::Ready(Ok(()));
                }

                while this.written < $bytes as u8 {
                    this.written += match Pin::new(&mut this.writer).poll_write(cx, &this.buf[this.written as usize..]) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                        Poll::Ready(Ok(0)) => {
                            return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
                        }
                        Poll::Ready(Ok(n)) => n as u8,
                    };
                }
                Poll::Ready(Ok(()))
            }
        }
    };
}
    macro_rules! serializer8 {
    ($name:ident $ty:ty) => {
        #[must_use = "futures do nothing unless you `.await` or poll them"]
        pub struct $name<W> {
            writer: W,
            byte: $ty,
        }

        impl<W> $name<W> {
            pub(crate) fn new(writer: W, byte: $ty) -> Self {
                Self {writer,byte}
            }
        }

        impl<W: AsyncWrite + Unpin> Future for $name<W> {
            type Output = io::Result<()>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let this = Pin::into_inner(self);

                let buf = [this.byte as u8];

                match Pin::new(&mut this.writer).poll_write(cx, &buf) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                    Poll::Ready(Ok(0)) => Poll::Ready(Err(io::ErrorKind::WriteZero.into())),
                    Poll::Ready(Ok(1)) => Poll::Ready(Ok(())),
                    Poll::Ready(Ok(_)) => unreachable!(),
                }
            }
        }
    };
}


    serializer8!(SerializeI8 i8);
    serializer8!(SerializeU8 u8);

    serializer!(SerializeI16 i16);
    serializer!(SerializeU16 u16);

    serializer!(SerializeI32 i32);
    serializer!(SerializeU32 u32);

    serializer!(SerializeI64 i64);
    serializer!(SerializeU64 u64);

    serializer!(SerializeIsize isize);
    serializer!(SerializeUsize usize);
}
pub use primitives::*;