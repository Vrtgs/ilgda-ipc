use std::io;
use std::io::Error;
use std::io::ErrorKind::WriteZero;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncWrite};
use crate::serde::{Stage, StagePoller, resolve_stage};
use core::marker::PhantomData;
use std::mem::size_of;
use crate::serde::sealed::Primitive;

mod primitives {
    use crate::serde::AsyncSer;
    use super::*;

    macro_rules! serializer {
    ($name:ident $ty:ty) => {
        serializer!($name, $ty, std::mem::size_of::<$ty>());
    };
    ($name:ident, $ty:ty, $bytes:expr) => {
        paste::paste!{
            pub type $name<'a, W> = crate::serde::StageFuture<'a, [<$name Stage>]<W>>;

            #[must_use = "futures do nothing unless you `.await` or poll them"]
            pub struct [<$name Stage>]<W> {
                buf: [u8; $bytes],
                written: u8,
                // used so we make sure we always get passed in a writer of the same type
                _shared_mark: PhantomData<W>
            }

            impl<W: AsyncWrite + Unpin> [<$name Stage>]<W> {
                #[inline(always)]
                pub fn new(value: $ty) -> Self {
                    Self {
                        buf: value.to_ne_bytes(),
                        written: 0,

                        _shared_mark: PhantomData
                    }
                }
            }

            impl<W: AsyncWrite + Unpin> StagePoller for [<$name Stage>]<W> {
                type Shared = W;
                type Output = io::Result<()>;

                fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>)
                    -> Poll<Self::Output> {
                    let this = Pin::into_inner(self);
                    let shared = Pin::into_inner(shared);

                    while this.written < $bytes as u8 {
                        this.written += match Pin::new(&mut *shared).poll_write(cx, &this.buf[this.written as usize..]) {
                            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                            Poll::Ready(Ok(0)) => {
                                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
                            }
                            Poll::Ready(Ok(n)) => n as u8,
                            Poll::Pending => return Poll::Pending,
                        };
                    }
                    Poll::Ready(Ok(()))
                }
            }
        }
    };
    }
    macro_rules! serializer8 {
    ($name:ident $ty:ty) => {
        const _: () = assert!(size_of::<$ty>() == 1);

        paste::paste!{
            pub type $name<'a, W> = crate::serde::StageFuture<'a, [<$name Stage>]<W>>;

            #[must_use = "futures do nothing unless you `.await` or poll them"]
            #[repr(transparent)]
            pub struct [<$name Stage>]<W> {
                byte: $ty,
                // used so we make sure we always get passed in a writer of the same type
                _shared_mark: PhantomData<W>
            }

            impl<W: AsyncWrite + Unpin> [<$name Stage>]<W> {
                #[inline(always)]
                pub fn new(byte: $ty) -> Self {
                    Self {byte, _shared_mark: PhantomData}
                }
            }

            impl<W: AsyncWrite + Unpin> StagePoller for [<$name Stage>]<W> {
                type Shared = W;
                type Output = io::Result<()>;

                fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>)
                    -> Poll<Self::Output>
                {
                    let this = Pin::into_inner(self);

                    let buf = [this.byte as u8];

                    match shared.poll_write(cx, &buf) {
                        Poll::Ready(Ok(0)) => Poll::Ready(Err(io::ErrorKind::WriteZero.into())),
                        Poll::Ready(Ok(1)) => Poll::Ready(Ok(())),

                        Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                        Poll::Pending => Poll::<_>::Pending,

                        Poll::Ready(Ok(_)) => unreachable!(),
                    }
                }
            }
        }
    };
    }
    macro_rules! floating_serializer {
        ($name:ident $bits:ident $ty:ident) => {
            paste::paste!{
            pub type $name<'a, W> = crate::serde::StageFuture<'a, [<$name Stage>]<W>>;

            #[must_use = "futures do nothing unless you `.await` or poll them"]
            #[repr(transparent)]
            pub struct [<$name Stage>]<W> {
                inner: [<$bits Stage>]<W>
            }

            impl<W: AsyncWrite + Unpin> [<$name Stage>]<W> {
                #[inline(always)]
                pub fn new(value: $ty) -> Self {
                    Self {inner: [<$bits Stage>]::new(value.to_bits())}
                }
            }

            impl<W: AsyncWrite + Unpin> StagePoller for [<$name Stage>]<W> {
                type Shared = W;
                type Output = io::Result<()>;

                fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>)
                    -> Poll<Self::Output> {
                    Pin::new(&mut Pin::into_inner(self).inner).poll_stage(shared, cx)
                }
            }
        }
        }
    }


    serializer8!(SerializeI8 i8);
    serializer8!(SerializeU8 u8);

    serializer!(SerializeI16 i16);
    serializer!(SerializeU16 u16);

    serializer!(SerializeI32 i32);
    serializer!(SerializeU32 u32);

    serializer!(SerializeI64 i64);
    serializer!(SerializeU64 u64);

    serializer!(SerializeI128 i128);
    serializer!(SerializeU128 u128);

    serializer!(SerializeIsize isize);
    serializer!(SerializeUsize usize);

    floating_serializer!(SerializeF64 SerializeU64 f64);
    floating_serializer!(SerializeF32 SerializeU32 f32);

    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct SerializeBytes<'a, W: AsyncWrite + Unpin> {
        len_fut: Stage<SerializeUsizeStage<W>, W, (), Error>,
        buf: &'a [u8],
    }

    impl<'a, W: AsyncWrite + Unpin> SerializeBytes<'a, W> {
        #[inline(always)]
        pub fn new(buf: &'a [u8]) -> Self {
            Self {
                len_fut: Stage::Pending(SerializeUsizeStage::new(buf.len())),
                buf,
            }
        }
    }

    impl<'a, W: AsyncWrite + Unpin> StagePoller for SerializeBytes<'a, W> {
        type Shared = W;
        type Output = io::Result<()>;

        fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = Pin::into_inner(self);

            let writer = Pin::into_inner(shared);

            resolve_stage!(this.len_fut, cx, Pin::new(&mut *writer));

            while !this.buf.is_empty() {
                match Pin::new(&mut *writer).poll_write(cx, this.buf) {
                    // Unlikely
                    Poll::Ready(Ok(0)) => return Poll::Ready(Err(WriteZero.into())),
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),

                    Poll::Ready(Ok(len)) => this.buf = &this.buf[len..],
                    Poll::Pending => return Poll::<_>::Pending,
                }
            }

            Poll::Ready(Ok(()))
        }
    }

    macro_rules! slice_type_alias {
        ($($t:tt),* $(,)?) => {$(paste::paste!{
            pub type [<SerializeI $t Slice>]<'a, W> =
                SerializePrimitiveSlice<'a, [<i $t>], W>;

            pub type [<SerializeU $t Slice>]<'a, W> =
                SerializePrimitiveSlice<'a, [<u $t>], W>;
        })*};
    }

    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct SerializePrimitiveSlice<'a, T: Primitive, W: AsyncWrite + Unpin + 'a> {
        inner: SerializeBytes<'a, W>,
        __pod_marker: PhantomData<T>
    }
    impl<'a, T: Primitive, W: AsyncWrite + Unpin> SerializePrimitiveSlice<'a, T, W> {
        #[inline(always)]
        pub fn new(buf: &'a [T]) -> Self {
            Self {
                inner: SerializeBytes {
                    len_fut: Stage::Pending(SerializeUsizeStage::new(buf.len())),
                    buf: bytemuck::must_cast_slice(buf),
                },
                __pod_marker: PhantomData
            }
        }
    }
    impl<'a, T: Primitive, W: AsyncWrite + Unpin> StagePoller for SerializePrimitiveSlice<'a, T, W> {
        type Shared = W;
        type Output = io::Result<()>;

        #[inline(always)]
        fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            Pin::new(&mut Pin::into_inner(self).inner).poll_stage(shared, cx)
        }
    }

    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct SerializeOption<'a, T: AsyncSer<'a, W> + 'a, W: AsyncWrite + Unpin + 'a> {
        completed_state_stage: bool,
        data: Option<T::SerializeStage>
    }

    impl<'a, T: AsyncSer<'a, W> + 'a, W: AsyncWrite + Unpin + 'a> SerializeOption<'a, T, W> {
        #[inline(always)]
        pub fn new(data: Option<&'a T>) -> Self {
            Self {
                completed_state_stage: false,
                data: data.map(T::write_to_stage)
            }
        }
    }
    impl<'a, T: AsyncSer<'a, W> + 'a, W: AsyncWrite + Unpin + 'a> StagePoller for SerializeOption<'a, T, W> {
        type Shared = W;
        type Output = io::Result<()>;

        fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>) -> Poll<Self::Output> {

            let shared = Pin::into_inner(shared);
            let this = Pin::into_inner(self);

            if !this.completed_state_stage {
                // We can do this and not save state as SerializeU8Stage
                // is guaranteed to either fail succeed or await and not save state
                let mut ser = SerializeU8Stage::new(this.data.is_some() as u8);

                match Pin::new(&mut ser).poll_stage(Pin::new(&mut *shared), cx) {
                    Poll::Ready(Ok(())) => this.completed_state_stage = true,
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Pending => return Poll::Pending,
                }
            }

            match this.data {
                None => Poll::Ready(Ok(())),
                Some(ref mut to_ser) => {
                    Pin::new(to_ser).poll_stage(Pin::new(shared), cx)
                }
            }
        }
    }

    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct SerializeResult<'a, T: AsyncSer<'a, W> + 'a, E: AsyncSer<'a, W>+ 'a, W: AsyncWrite + Unpin + 'a> {
        completed_state_stage: bool,
        data: Result<T::SerializeStage, E::SerializeStage>
    }

    impl<'a, T: AsyncSer<'a, W> + 'a, E: AsyncSer<'a, W>+ 'a, W: AsyncWrite + Unpin + 'a> SerializeResult<'a, T, E, W> {
        #[inline(always)]
        pub fn new(data: Result<&'a T, &'a E>) -> Self {
            Self {
                completed_state_stage: false,
                data: data.map(T::write_to_stage).map_err(E::write_to_stage)
            }
        }
    }
    impl<'a, T: AsyncSer<'a, W> + 'a, E: AsyncSer<'a, W>+ 'a, W: AsyncWrite + Unpin + 'a> StagePoller for SerializeResult<'a, T, E, W> {
        type Shared = W;
        type Output = io::Result<()>;

        fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let shared = Pin::into_inner(shared);
            let this = Pin::into_inner(self);

            if !this.completed_state_stage {
                // We can do this and not save state as SerializeU8Stage
                // is guaranteed to either fail succeed or await and not save state
                let mut ser = SerializeU8Stage::new(this.data.is_ok() as u8);

                match Pin::new(&mut ser).poll_stage(Pin::new(&mut *shared), cx) {
                    Poll::Ready(Ok(())) => this.completed_state_stage = true,
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Pending => return Poll::Pending,
                }
            }

            match this.data {
                Err(ref mut to_ser) => {
                    Pin::new(to_ser).poll_stage(Pin::new(shared), cx)
                }
                Ok(ref mut to_ser) => {
                    Pin::new(to_ser).poll_stage(Pin::new(shared), cx)
                }
            }
        }
    }

    slice_type_alias!{
        128, 64, 32, 16, 8, size
    }
}
pub use primitives::*;