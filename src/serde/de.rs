use std::io;
use std::io::Error;
use std::io::ErrorKind::UnexpectedEof;
use std::pin::{Pin};
use std::task::{Context, Poll};
use std::marker::PhantomData;
use tokio::io::{AsyncRead, ReadBuf};
use crate::serde::{Stage, StagePoller};
use crate::serde::sealed::{Primitive};

mod primitives {
    use std::io::ErrorKind::InvalidData;
    use std::mem::MaybeUninit;
    use bytemuck::{AnyBitPattern, NoUninit};
    use crate::debug_unreachable;
    use crate::serde::AsyncDe;
    use super::*;

    fn uninit_vec<T>(len: usize) -> Vec<MaybeUninit<T>> {
        let mut vec = Vec::<MaybeUninit<T>>::with_capacity(len);
        // SAFETY: `MaybeUninit` allows being uninitialized
        unsafe {
            vec.set_len(len);
        }
        vec
    }

    struct Cast<A, B>(A, B);
    impl<A, B> Cast<A, B> {
        const ASSERT_ALIGN_GREATER_THAN_EQUAL: () =
            assert!(std::mem::align_of::<A>() >= std::mem::align_of::<B>());
        const ASSERT_SIZE_MULTIPLE_OF: () = assert!(
            (std::mem::size_of::<A>() == 0) == (std::mem::size_of::<B>() == 0)
                && (std::mem::size_of::<A>() % std::mem::size_of::<B>() == 0)
        );
    }

    #[inline(always)]
    fn must_cast_uninit_slice_mut<
        A: NoUninit + AnyBitPattern,
    >(a: &mut [MaybeUninit<A>]) -> &mut [MaybeUninit<u8>] {
        #[allow(clippy::let_unit_value)]
        let _ = Cast::<A, u8>::ASSERT_SIZE_MULTIPLE_OF;
        #[allow(clippy::let_unit_value)]
        let _ = Cast::<A, u8>::ASSERT_ALIGN_GREATER_THAN_EQUAL;


        let new_len = if std::mem::size_of::<A>() == std::mem::size_of::<u8>() {
            a.len()
        } else {
            a.len() * (std::mem::size_of::<A>() / std::mem::size_of::<u8>())
        };
        unsafe { core::slice::from_raw_parts_mut(a.as_mut_ptr().cast(), new_len) }
    }

    #[inline(always)]
    unsafe fn assume_vec_innit<T>(uninit: Vec<MaybeUninit<T>>) -> Vec<T> {
        let mut uninit = std::mem::ManuallyDrop::new(uninit);
        let ptr = uninit.as_mut_ptr().cast();
        let length = uninit.len();
        let capacity = uninit.capacity();

        Vec::from_raw_parts(ptr, length, capacity)
    }


    macro_rules! deserializer {
    ($name:ident $ty:ty) => {
        deserializer!($name, $ty, std::mem::size_of::<$ty>());
    };
    ($name:ident, $ty:ty, $bytes:expr) => {
        paste::paste!{
            pub type $name<'a, R> = crate::serde::StageFuture<'a, [<$name Stage>]<R>>;

            #[must_use = "futures do nothing unless you `.await` or poll them"]
            pub struct [<$name Stage>]<R> {
                buf: [u8; $bytes],
                read: u8,
                // used so we make sure we always get passed in a writer of the same type
                _shared_mark: PhantomData<R>
            }
            impl<R> [<$name Stage>]<R> {
                #[inline(always)]
                pub fn new() -> Self {
                    [<$name Stage>] {
                        buf: [0; $bytes],
                        read: 0,
                        _shared_mark: PhantomData
                    }
                }
            }
            impl<R> Default for [<$name Stage>]<R> {
                #[inline(always)]
                fn default() -> Self {
                    Self::new()
                }
            }


            impl<R: AsyncRead + Unpin> StagePoller for [<$name Stage>]<R> {
                type Shared = R;
                type Output = io::Result<$ty>;

                fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>)
                    -> Poll<Self::Output> {
                    let this = Pin::into_inner(self);

                    let shared = Pin::into_inner(shared);

                    while this.read < $bytes as u8 {
                        let mut buf = ReadBuf::new(&mut this.buf[this.read as usize..]);

                        this.read += match Pin::new(&mut *shared).poll_read(cx, &mut buf) {
                            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                            Poll::Ready(Ok(())) => {
                                let n = buf.filled().len();
                                if n == 0 {
                                    return Poll::Ready(Err(UnexpectedEof.into()));
                                }

                                n as u8
                            }
                            Poll::Pending => return Poll::<_>::Pending,
                        };
                    }

                    Poll::Ready(Ok(<$ty>::from_ne_bytes(this.buf)))
                }
            }


        }
    };
    }
    macro_rules! deserializer8 {
    ($name:ident $ty:ty) => {
        paste::paste!{
            pub type $name<'a, R> = crate::serde::StageFuture<'a, [<$name Stage>]<R>>;

            #[must_use = "futures do nothing unless you `.await` or poll them"]
            #[repr(transparent)]
            pub struct [<$name Stage>]<R>(PhantomData<R>);

            impl<R> [<$name Stage>]<R> {
                #[inline(always)]
                pub fn new() -> Self {
                    Self(PhantomData)
                }
            }

            impl<R> Default for [<$name Stage>]<R> {
                #[inline(always)]
                fn default() -> Self {
                    Self::new()
                }
            }

            impl<R: AsyncRead + Unpin> StagePoller for [<$name Stage>]<R> {
                type Shared = R;
                type Output = io::Result<$ty>;

                fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>)
                    -> Poll<Self::Output>
                {
                    let mut buf = [0; 1];
                    let mut buffer = ReadBuf::new(&mut buf);
                    match shared.poll_read(cx, &mut buffer) {
                        Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                        Poll::Ready(Ok(())) => {
                            if buffer.filled().is_empty() {
                                return Poll::Ready(Err(UnexpectedEof.into()));
                            }

                            Poll::Ready(Ok(buf[0] as $ty))
                        }
                        Poll::Pending => Poll::Pending,
                    }
                }
            }
        }
    };
    }
    macro_rules! floating_deserializer {
        ($name:ident $bits:ident $ty:ty) => {
            paste::paste!{
            pub type $name<'a, R> = crate::serde::StageFuture<'a, [<$name Stage>]<R>>;

            #[must_use = "futures do nothing unless you `.await` or poll them"]
            #[repr(transparent)]
            pub struct [<$name Stage>]<R> {
                inner: [<$bits Stage>]<R>
            }

            impl<R: AsyncRead + Unpin> [<$name Stage>]<R> {
                #[inline(always)]
                pub fn new() -> Self {
                    Self{inner: [<$bits Stage>]::<R>::new()}
                }
            }
            impl<R: AsyncRead + Unpin> Default for [<$name Stage>]<R> {
                #[inline(always)]
                fn default() -> Self {
                    Self::new()
                }
            }

            impl<R: AsyncRead + Unpin> StagePoller for [<$name Stage>]<R> {
                type Shared = R;
                type Output = io::Result<$ty>;

                #[inline(always)]
                fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>)
                    -> Poll<Self::Output> {
                    let this = Pin::into_inner(self);

                    match Pin::new(&mut this.inner).poll_stage(shared, cx) {
                        Poll::Ready(Ok(bits)) => Poll::Ready(Ok(<$ty>::from_bits(bits))),
                        Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                        Poll::Pending => Poll::<_>::Pending
                    }
                }
            }
            }
        };
    }

    deserializer8!(DeserializeI8 i8);
    deserializer8!(DeserializeU8 u8);

    deserializer!(DeserializeI16 i16);
    deserializer!(DeserializeU16 u16);

    deserializer!(DeserializeI32 i32);
    deserializer!(DeserializeU32 u32);

    deserializer!(DeserializeI64 i64);
    deserializer!(DeserializeU64 u64);

    deserializer!(DeserializeI128 i128);
    deserializer!(DeserializeU128 u128);

    deserializer!(DeserializeIsize isize);
    deserializer!(DeserializeUsize usize);

    floating_deserializer!(DeserializeF32 DeserializeU32 f32);
    floating_deserializer!(DeserializeF64 DeserializeU64 f64);


    macro_rules! ready_up_data {
        ($this: ident, $reader:expr, $cx: ident) => {
            match &mut $this.len_fut {
                Stage::Pending(ft) => {
                    match Pin::new(ft).poll_stage($reader, $cx) {
                        Poll::Ready(Ok(done)) => {
                            $this.data = uninit_vec(done);
                            $this.len_fut = Stage::Completed(done);
                        }
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                        Poll::Pending => return Poll::Pending,
                    }
                }
                Stage::Completed(_) => (),
            };
        };
    }

    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct DeserializeBytesStage<R: AsyncRead + Unpin> {
        len_fut: Stage<DeserializeUsizeStage<R>, R, usize, Error>,
        data: Vec<MaybeUninit<u8>>,
        filled: usize
    }

    impl<R: AsyncRead + Unpin> DeserializeBytesStage<R> {
        #[inline(always)]
        pub fn new() -> Self {
            Self {
                data: Vec::new(),
                len_fut: Stage::Pending(DeserializeUsizeStage::new()),
                filled: 0,
            }
        }
    }
    impl<R: AsyncRead + Unpin> Default for DeserializeBytesStage<R> {
        #[inline(always)]
        fn default() -> Self {
            Self::new()
        }
    }
    impl<R: AsyncRead + Unpin> StagePoller for DeserializeBytesStage<R> {
        type Shared = R;
        type Output = io::Result<Vec<u8>>;

        fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = Pin::into_inner(self);

            let shared = Pin::into_inner(shared);

            ready_up_data!(this, Pin::new(&mut *shared), cx);

            while this.filled < this.data.len() {
                let mut buf = ReadBuf::uninit(&mut this.data[this.filled..]);

                match Pin::new(&mut *shared).poll_read(cx, &mut buf) {
                    Poll::Ready(Ok(())) => match buf.filled::<>().len() {
                        0 => return Poll::Ready(Err(UnexpectedEof.into())),
                        filled => this.filled += filled,
                    },

                    Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                    Poll::Pending => return Poll::Pending
                }
            }

            Poll::Ready(Ok(unsafe {
                // the condition above ensures we have initialized all bytes
                assume_vec_innit(std::mem::take(&mut this.data))
            }))
        }
    }

    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct DeserializePrimitiveSliceStage<T: Primitive, R: AsyncRead + Unpin> {
        len_fut: Stage<DeserializeUsizeStage<R>, R, usize, Error>,
        data: Vec<MaybeUninit<T>>,
        filled: usize
    }

    impl<T: Primitive, R: AsyncRead + Unpin> DeserializePrimitiveSliceStage<T, R> {
        #[inline(always)]
        pub fn new() -> Self {
            Self {
                data: Vec::new(),
                len_fut: Stage::Pending(DeserializeUsizeStage::new()),
                filled: 0,
            }
        }
    }
    impl<T: Primitive, R: AsyncRead + Unpin> Default for DeserializePrimitiveSliceStage<T, R> {
        #[inline(always)]
        fn default() -> Self {
            Self::new()
        }
    }
    impl<T: Primitive, R: AsyncRead + Unpin> StagePoller for DeserializePrimitiveSliceStage<T, R> {
        type Shared = R;
        type Output = io::Result<Vec<T>>;

        fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = Pin::into_inner(self);

            let shared = Pin::into_inner(shared);

            ready_up_data!(this, Pin::new(&mut *shared), cx);

            while this.filled < this.data.len() * std::mem::size_of::<T>() {
                let mut buf = ReadBuf::uninit(
                    &mut must_cast_uninit_slice_mut(&mut this.data)[this.filled..]
                );

                match Pin::new(&mut *shared).poll_read(cx, &mut buf) {
                    Poll::Ready(Ok(())) => match buf.filled::<>().len() {
                        0 => return Poll::Ready(Err(UnexpectedEof.into())),
                        filled => this.filled += filled,
                    },

                    Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                    Poll::Pending => return Poll::<_>::Pending
                }
            }

            Poll::Ready(Ok(unsafe {
                // the condition above ensures we have initialized all bytes
                assume_vec_innit(std::mem::take(&mut this.data))
            }))
        }
    }

    macro_rules! slice_type_alias {
        ($($t:tt),* $(,)?) => {$(paste::paste!{
            pub type [<DeserializeI $t SliceStage>]<W> =
                DeserializePrimitiveSliceStage<[<i $t>], W>;

            pub type [<DeserializeU $t SliceStage>]<W> =
                DeserializePrimitiveSliceStage<[<u $t>], W>;
        })*};
    }

    slice_type_alias! {
        128, 64, 32, 16, 8, size
    }

    #[must_use = "futures do nothing unless you `.await` or poll them"]
    #[repr(transparent)]
    pub struct DeserializeStringStage<R: AsyncRead + Unpin> {
        inner: DeserializeBytesStage<R>,
    }

    impl<R: AsyncRead + Unpin> DeserializeStringStage<R> {
        #[inline(always)]
        pub fn new() -> Self {
            Self { inner: DeserializeBytesStage::new() }
        }
    }
    impl<R: AsyncRead + Unpin> Default for DeserializeStringStage<R> {
        #[inline(always)]
        fn default() -> Self {
            Self::new()
        }
    }
    impl<R: AsyncRead + Unpin> StagePoller for DeserializeStringStage<R> {
        type Shared = R;
        type Output = io::Result<String>;

        fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            match Pin::new(&mut Pin::into_inner(self).inner).poll_stage(shared, cx) {
                Poll::Ready(Ok(bytes)) => match simdutf8::basic::string::from_utf8(bytes) {
                    Ok(string) => Poll::Ready(Ok(string)),
                    Err(_) => Poll::Ready(Err(
                        Error::new(InvalidData, "invalid UTF-8")
                    )),
                },
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending
            }
        }
    }


    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub enum DeserializeOption<'a, T: AsyncDe<'a, R> + 'a, R: AsyncRead + Unpin + 'a> {
        DiscriminatorPending(DeserializeU8Stage<R>),
        Pending(T::DeserializeStage),
    }

    impl<'a, T: AsyncDe<'a, R> + 'a, R: AsyncRead + Unpin + 'a> DeserializeOption<'a, T, R> {
        #[inline(always)]
        pub fn new() -> Self {
            Self::DiscriminatorPending(Default::default())
        }
    }
    impl<'a, T: AsyncDe<'a, R> + 'a, R: AsyncRead + Unpin + 'a> Default for DeserializeOption<'a, T, R> {
        #[inline(always)]
        fn default() -> Self {
            Self::new()
        }
    }

    impl<'a, T: AsyncDe<'a, R>  + 'a, R: AsyncRead + Unpin + 'a> StagePoller for DeserializeOption<'a, T, R> {
        type Shared = R;
        type Output = io::Result<Option<T::Owned>>;

        fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = Pin::into_inner(self);
            let shared = Pin::into_inner(shared);


            macro_rules! poll_pending {
                ($pending: ident) => {
                    match Pin::new($pending).poll_stage(Pin::new(shared), cx) {
                        Poll::Ready(Ok(inner)) => Poll::Ready(Ok(Some(inner))),
                        Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                        Poll::Pending => Poll::Pending,
                    }
                };
            }

            match this {
                DeserializeOption::DiscriminatorPending(rd) => {
                    match Pin::new(rd).poll_stage(Pin::new(&mut *shared), cx) {
                        Poll::Ready(Ok(val)) => match val {
                            1 => {
                                *this = DeserializeOption::Pending(T::read_from_stage());

                                let pending = match this {
                                    DeserializeOption::Pending(p) => {
                                        p
                                    },

                                    // We just changed `this`
                                    _ => unsafe {debug_unreachable!()}
                                };

                                poll_pending!(pending)
                            },
                            0 => Poll::Ready(Ok(None)),
                            _ => Poll::Ready(Err(Error::new(InvalidData, "invalid discriminator")))
                        },
                        Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                        Poll::Pending => Poll::Pending,
                    }
                }
                DeserializeOption::Pending(pending) => poll_pending!(pending)
            }
        }
    }

    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub enum DeserializeResult<'a, T: AsyncDe<'a, R> + 'a, E: AsyncDe<'a, R>+ 'a, R: AsyncRead + Unpin + 'a> {
        DiscriminatorPending(DeserializeU8Stage<R>),
        PendingT(T::DeserializeStage),
        PendingE(E::DeserializeStage),
    }

    impl<'a, T: AsyncDe<'a, R> + 'a, E: AsyncDe<'a, R>+ 'a, R: AsyncRead + Unpin + 'a> DeserializeResult<'a, T, E, R> {
        #[inline(always)]
        pub fn new() -> Self {
            Self::DiscriminatorPending(Default::default())
        }
    }
    impl<'a, T: AsyncDe<'a, R> + 'a, E: AsyncDe<'a, R>+ 'a, R: AsyncRead + Unpin + 'a> Default for DeserializeResult<'a, T, E, R> {
        #[inline(always)]
        fn default() -> Self {
            Self::new()
        }
    }

    impl<'a, T: AsyncDe<'a, R> + 'a, E: AsyncDe<'a, R>+ 'a, R: AsyncRead + Unpin + 'a> StagePoller for DeserializeResult<'a, T, E, R> {
        type Shared = R;
        type Output = io::Result<Result<T::Owned, E::Owned>>;

        fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = Pin::into_inner(self);
            let shared = Pin::into_inner(shared);

            macro_rules! poll_t_pending {
                ($pending: ident) => {
                    match Pin::new($pending).poll_stage(Pin::new(shared), cx) {
                        Poll::Ready(Ok(inner)) => Poll::Ready(Ok(Ok(inner))),
                        Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                        Poll::Pending => Poll::Pending,
                    }
                };
            }

            macro_rules! poll_e_pending {
                ($pending: ident) => {
                    match Pin::new($pending).poll_stage(Pin::new(shared), cx) {
                        Poll::Ready(Ok(inner)) => Poll::Ready(Ok(Err(inner))),
                        Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                        Poll::Pending => Poll::Pending,
                    }
                };
            }

            match this {
                DeserializeResult::DiscriminatorPending(rd) => match Pin::new(rd).poll_stage(Pin::new(&mut *shared), cx) {
                    Poll::Ready(Ok(val)) => match val {
                        1 => {
                            *this = DeserializeResult::PendingT(T::read_from_stage());
                            let pending = match this {
                                DeserializeResult::PendingT(p) => {
                                    p
                                },
                                // We just changed `this`
                                _ => unsafe {debug_unreachable!()}
                            };

                            poll_t_pending!(pending)
                        },
                        0 => {
                            *this = DeserializeResult::PendingE(E::read_from_stage());
                            let pending = match this {
                                DeserializeResult::PendingE(p) => {
                                    p
                                },
                                // We just changed `this`
                                _ => unsafe {debug_unreachable!()}
                            };

                            poll_e_pending!(pending)
                        },
                        _ => Poll::Ready(Err(Error::new(InvalidData, "invalid discriminator")))
                    },
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                    Poll::Pending => Poll::Pending,
                },
                DeserializeResult::PendingT(pending) => poll_t_pending!(pending),
                DeserializeResult::PendingE(pending) => poll_e_pending!(pending),
            }
        }
    }
}

pub use primitives::*;