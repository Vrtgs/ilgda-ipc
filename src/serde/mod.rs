pub mod de;
pub mod ser;

mod sealed {
    use bytemuck::{AnyBitPattern, NoUninit, Pod};

    pub trait Primitive: Pod + NoUninit + AnyBitPattern + Unpin {}
    pub trait Byte: Pod + NoUninit + AnyBitPattern + Unpin {}

    impl Byte for u8 {}
    impl Byte for i8 {}

    impl Primitive for u16 {}
    impl Primitive for i16 {}

    impl Primitive for u32 {}
    impl Primitive for i32 {}

    impl Primitive for u64 {}
    impl Primitive for i64 {}

    impl Primitive for u128 {}
    impl Primitive for i128 {}

    impl Primitive for usize {}
    impl Primitive for isize {}
}

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite};
use crate::serde::sealed::{Primitive};

pub enum Stage<F: StagePoller<Shared=Shared, Output=Result<T, E>>, Shared, T, E> {
    Pending(F),
    Completed(T),
}

pub trait StagePoller {
    type Shared: ?Sized + Unpin;
    type Output;
    fn poll_stage(self: Pin<&mut Self>, shared: Pin<&mut Self::Shared>, cx: &mut Context<'_>)
        -> Poll<Self::Output>;
}

macro_rules! resolve_stage {
    ($stage:expr, $cx: ident, $shared:expr $(,)?) => {
        match &mut $stage {
            Stage::Pending(ft) => match Pin::new(ft).poll_stage($shared, $cx) {
                Poll::Ready(Ok(stage)) => {
                    $stage = Stage::Completed(stage);
                    match &mut $stage {
                        Stage::Completed(stage) => stage,
                        Stage::Pending(_) => unsafe {use $crate::debug_unreachable;$crate::debug_unreachable!()}
                    }
                },
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e.into())),
                Poll::Pending => return Poll::Pending,
            }
            Stage::Completed(done) => done,
        }
    };
}
pub(crate) use resolve_stage;

pub trait AsyncDe<'a, R: AsyncRead + Unpin + 'a>: ToOwned {
    type DeserializeStage: StagePoller<Shared=R, Output=io::Result<Self::Owned>> + Unpin + 'a;
    fn read_from_stage() -> Self::DeserializeStage;

    #[inline(always)]
    fn read_from(stream: &'a mut R) -> StageFuture<'a, Self::DeserializeStage> {
        StageFuture::new(stream, Self::read_from_stage())
    }
}

pub trait AsyncSer<'a, W: AsyncWrite + Unpin + 'a> {
    type SerializeStage: StagePoller<Shared=W, Output=io::Result<()>> + Unpin + 'a
        where Self: 'a;

    fn write_to_stage(&'a self) -> Self::SerializeStage;

    #[inline(always)]
    fn write_to(&'a self, stream: &'a mut W) -> StageFuture<'a, Self::SerializeStage> {
        StageFuture::new(stream, Self::write_to_stage(self))
    }
}

pub struct StageFuture<'a, S: StagePoller> {
    shared: &'a mut S::Shared,
    inner: S
}

impl<'a, S: StagePoller> StageFuture<'a, S> {
    pub(crate) fn new(shared: &'a mut S::Shared, inner: S) -> Self {
        StageFuture{shared, inner}
    }
}

impl<'a, S: StagePoller + Unpin> Future for StageFuture<'a, S> {
    type Output = S::Output;

    #[inline(always)]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = Pin::into_inner(self);
        Pin::new(&mut this.inner).poll_stage(Pin::new(&mut *this.shared), cx)
    }
}

macro_rules! write_to {
    () => {
        #[inline(always)]
        fn write_to_stage(&'a self) -> Self::SerializeStage {
            Self::SerializeStage::new(self)
        }

        #[inline(always)]
        fn write_to(&'a self, stream: &'a mut W) -> StageFuture<'a, Self::SerializeStage> {
            StageFuture::new(stream, Self::SerializeStage::new(self))
        }
    };
}

macro_rules! read_from {
    () => {
        #[inline(always)]
        fn read_from_stage() -> Self::DeserializeStage {
            Self::DeserializeStage::new()
        }

        #[inline(always)]
        fn read_from(stream: &'a mut R) -> StageFuture<'a, Self::DeserializeStage> {
            StageFuture::new(stream, Self::DeserializeStage::new())
        }
    };
}

macro_rules! primitive_serde {
    ($ser: ident $de: ident $ty:ty) => {
        impl<'a, R: AsyncRead + Unpin + 'a> AsyncDe<'a, R> for $ty {
            paste::paste!{type DeserializeStage = de::[<$de Stage>]<R>;}

            read_from! {}
        }

        impl<'a, W: AsyncWrite + Unpin + 'a> AsyncSer<'a, W> for $ty {
            paste::paste!{type SerializeStage = ser::[<$ser Stage>]<W>;}

            #[inline(always)]
            fn write_to_stage(&self) -> Self::SerializeStage {
                Self::SerializeStage::new(*self)
            }

            #[inline(always)]
            fn write_to(&'a self, stream: &'a mut W) -> StageFuture<'a, Self::SerializeStage> {
                StageFuture::new(stream, Self::SerializeStage::new(*self))
            }
        }
    };
}

primitive_serde!(SerializeI8 DeserializeI8 i8);
primitive_serde!(SerializeU8 DeserializeU8 u8);

primitive_serde!(SerializeI16 DeserializeI16 i16);
primitive_serde!(SerializeU16 DeserializeU16 u16);

primitive_serde!(SerializeI32 DeserializeI32 i32);
primitive_serde!(SerializeU32 DeserializeU32 u32);

primitive_serde!(SerializeI64 DeserializeI64 i64);
primitive_serde!(SerializeU64 DeserializeU64 u64);

primitive_serde!(SerializeI128 DeserializeI128 i128);
primitive_serde!(SerializeU128 DeserializeU128 u128);

primitive_serde!(SerializeIsize DeserializeIsize isize);
primitive_serde!(SerializeUsize DeserializeUsize usize);

primitive_serde!(SerializeF64 DeserializeF64 f64);
primitive_serde!(SerializeF32 DeserializeF32 f32);

impl<'a, W: AsyncWrite + Unpin + 'a> AsyncSer<'a, W> for [u8] {
    type SerializeStage = ser::SerializeBytes<'a, W>;

    write_to! {}
}
impl<'a, R: AsyncRead + Unpin + 'a> AsyncDe<'a, R> for [u8] {
    type DeserializeStage = de::DeserializeBytesStage<R>
        where Self: 'a;

    read_from! {}
}

impl<'a, W: AsyncWrite + Unpin + 'a> AsyncSer<'a, W> for Vec<u8> {
    type SerializeStage = ser::SerializeBytes<'a, W>;

    write_to! {}
}
impl<'a, R: AsyncRead + Unpin + 'a> AsyncDe<'a, R> for Vec<u8> {
    type DeserializeStage = de::DeserializeBytesStage<R>
        where Self: 'a;

    read_from! {}
}

impl<'a, W: AsyncWrite + Unpin + 'a> AsyncSer<'a, W> for str {
    type SerializeStage  = <[u8] as AsyncSer<'a, W>>::SerializeStage
        where Self: 'a;

    #[inline(always)]
    fn write_to_stage(&'a self) -> Self::SerializeStage {
        Self::SerializeStage::new(self.as_bytes())
    }
}
impl<'a, R: AsyncRead + Unpin + 'a> AsyncDe<'a, R> for str {
    type DeserializeStage = de::DeserializeStringStage<R>
        where Self: 'a;

    read_from! {}
}

impl<'a, W: AsyncWrite + Unpin + 'a> AsyncSer<'a, W> for String {
    type SerializeStage  = <[u8] as AsyncSer<'a, W>>::SerializeStage
        where Self: 'a;

    #[inline(always)]
    fn write_to_stage(&'a self) -> Self::SerializeStage {
        Self::SerializeStage::new(self.as_bytes())
    }
}
impl<'a, R: AsyncRead + Unpin + 'a> AsyncDe<'a, R> for String {
    type DeserializeStage = de::DeserializeStringStage<R>
        where Self: 'a;

    read_from! {}
}

impl<'a, T: Primitive, W: AsyncWrite + Unpin + 'a> AsyncSer<'a, W> for [T] {
    type SerializeStage = ser::SerializePrimitiveSlice<'a, T,  W>;

    write_to! {}
}
impl<'a, T: Primitive, R: AsyncRead + Unpin + 'a> AsyncDe<'a, R> for [T] {
    type DeserializeStage = de::DeserializePrimitiveSliceStage<T, R>;

    read_from! {}
}

impl<'a, T: Primitive, W: AsyncWrite + Unpin + 'a> AsyncSer<'a, W> for Vec<T> {
    type SerializeStage = ser::SerializePrimitiveSlice<'a, T,  W>;

    write_to! {}
}
impl<'a, T: Primitive, R: AsyncRead + Unpin + 'a> AsyncDe<'a, R> for Vec<T> {
    type DeserializeStage = de::DeserializePrimitiveSliceStage<T, R>;

    read_from! {}
}

impl<'a, T: AsyncSer<'a, W> + 'a, W: AsyncWrite + Unpin + 'a> AsyncSer<'a, W> for Option<T> {
    type SerializeStage = ser::SerializeOption<'a, T, W>
        where Self: 'a;

    fn write_to_stage(&'a self) -> Self::SerializeStage {
        Self::SerializeStage::new(self.as_ref())
    }
}

impl<'a, T: AsyncSer<'a, W> + 'a, E: AsyncSer<'a, W>+ 'a, W: AsyncWrite + Unpin + 'a> AsyncSer<'a, W> for Result<T, E> {
    type SerializeStage = ser::SerializeResult<'a, T, E, W>
        where Self: 'a;

    fn write_to_stage(&'a self) -> Self::SerializeStage {
        Self::SerializeStage::new(self.as_ref())
    }
}

pub type SerializeFuture<'a, T, W> = StageFuture<'a, <T as AsyncSer<'a, W>>::SerializeStage>;
pub type DeserializeFuture<'a, T, R> = StageFuture<'a, <T as AsyncDe<'a, R>>::DeserializeStage>;