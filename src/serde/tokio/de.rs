mod primitives {
    use std::future::Future;
    use std::io;
    use std::io::ErrorKind::UnexpectedEof;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, ReadBuf};

    macro_rules! deserialize {
    ($name:ident $ty:ty) => {
        deserialize!($name, $ty, std::mem::size_of::<$ty>());
    };
    ($name:ident, $ty:ty, $bytes:expr) => {
        #[must_use = "futures do nothing unless you `.await` or poll them"]
        pub struct $name<R> {
            src: R,
            buf: [u8; $bytes],
            read: u8,
        }

        impl<R> $name<R> {
            pub(crate) fn new(src: R) -> Self {
                $name {
                    src,
                    buf: [0; $bytes],
                    read: 0,
                }
            }
        }

        impl<R: AsyncRead + Unpin> Future for $name<R> {
            type Output = io::Result<$ty>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let this = Pin::into_inner(self);

                while this.read < $bytes as u8 {
                    let mut buf = ReadBuf::new(&mut this.buf[this.read as usize..]);

                    this.read += match Pin::new(&mut this.src).poll_read(cx, &mut buf) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                        Poll::Ready(Ok(())) => {
                            let n = buf.filled().len();
                            if n == 0 {
                                return Poll::Ready(Err(UnexpectedEof.into()));
                            }

                            n as u8
                        }
                    };
                }

                Poll::Ready(Ok(<$ty>::from_ne_bytes(this.buf)))
            }
        }
    };
}

    macro_rules! deserialize8 {
    ($name:ident $ty:ty) => {
        #[must_use = "futures do nothing unless you `.await` or poll them"]
        pub struct $name<R: AsyncRead + Unpin> {
            reader: R,
        }

        impl<R: AsyncRead + Unpin> $name<R> {
            pub fn new(reader: R) -> Self {
                Self {reader}
            }
        }

        impl<R: AsyncRead + Unpin> Future  for $name<R> {
            type Output = io::Result<$ty>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let this = Pin::into_inner(self);

                let mut buf = [0; 1];
                let mut buffer = ReadBuf::new(&mut buf);
                match Pin::new(&mut this.reader).poll_read(cx, &mut buffer) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                    Poll::Ready(Ok(())) => {
                        if buffer.filled().is_empty() {
                            return Poll::Ready(Err(UnexpectedEof.into()));
                        }

                        Poll::Ready(Ok(buf[0] as $ty))
                    }
                }
            }
        }
    };
}

    deserialize8!(DeserializeI8 i8);
    deserialize8!(DeserializeU8 u8);

    deserialize!(DeserializeI16 i16);
    deserialize!(DeserializeU16 u16);

    deserialize!(DeserializeI32 i32);
    deserialize!(DeserializeU32 u32);

    deserialize!(DeserializeI64 i64);
    deserialize!(DeserializeU64 u64);

    deserialize!(DeserializeIsize isize);
    deserialize!(DeserializeUsize usize);
}
pub use primitives::*;

