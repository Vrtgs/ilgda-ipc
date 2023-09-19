pub mod serde;
mod entity;

use std::{
    pin::{Pin},
    task::{Context, Poll},
    io::{
        IoSlice,
        Result,
    }
};
use tokio::{
    io::{
        AsyncRead, AsyncReadExt, AsyncBufRead, AsyncWrite, AsyncWriteExt,
        BufReader, BufWriter, ReadBuf,
        Stdout,
        Stdin,
    },
    process::{
        Child,
        ChildStdout,
        ChildStdin,
    }
};
use crate::serde::{AsyncDe, AsyncSer, ser::*, de::*, SerializeFuture, DeserializeFuture};

pub struct IpcStream<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> {
    pub(crate) read_stream: BufReader<R>,
    pub(crate) write_stream: BufWriter<W>,
}

macro_rules! gen_rw {
    ($($r#type: ident $camel_type: ident)*) => {
        paste::paste! {$(
        #[doc = concat!("writes an i", stringify!($r#type), " in native-endian order to the underlying writer")]
        #[inline(always)]
        pub fn [<write_ $r#type>](&mut self, val: $r#type) -> [<Serialize $camel_type>]<BufWriter<W>> {
            crate::serde::StageFuture::new(
                &mut self.write_stream,
                <$r#type as AsyncSer<BufWriter<W>>>::SerializeStage::new(val)
            )
        }
        #[doc = concat!("reads a ", stringify!($r#type), " in native-endian order from the underlying reader")]
        #[inline(always)]
        pub fn [<read_ $r#type>](&mut self) -> [<Deserialize $camel_type>]<BufReader<R>> {
            <$r#type as AsyncDe<BufReader<R>>>::read_from(&mut self.read_stream)
        }
        )*}
    };
}

impl<R: AsyncReadExt + Unpin, W: AsyncWriteExt + Unpin> IpcStream<R, W> {
    #[inline(always)]
    pub fn write_buf<'a>(&'a mut self, buf: &'a [u8]) -> SerializeFuture<'a, [u8], BufWriter<W>> {
        <[u8] as AsyncSer<'a, BufWriter<W>>>::write_to(buf, &mut self.write_stream)
    }

    #[inline(always)]
    pub fn write_str<'a>(&'a mut self, str: &'a str) -> SerializeFuture<'a, str, BufWriter<W>> {
        str.write_to(&mut self.write_stream)
    }

    #[inline(always)]
    pub fn read_buf(&mut self) -> DeserializeFuture<[u8], BufReader<R>> {
        <[u8]>::read_from(&mut self.read_stream)
    }

    #[inline(always)]
    pub fn read_str(&mut self) -> DeserializeFuture<str, BufReader<R>> {
        str::read_from(&mut self.read_stream)
    }

    gen_rw! {
        usize Usize
        isize Isize
        u128 U128
        i128 I128
        u64 U64
        u32 U32
        u16 U16

        i64 I64
        i32 I32
        i16 I16

        f64 F64
        f32 F32

        u8 U8
        i8 I8
    }
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> AsyncRead for IpcStream<R, W> {
    #[inline(always)]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<()>> {
        let mut_ref = &mut Pin::into_inner(self).read_stream;
        Pin::new(mut_ref).poll_read(cx, buf)
    }
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> AsyncWrite for IpcStream<R, W> {
    #[inline(always)]
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize>> {
        let mut_ref = &mut Pin::into_inner(self).write_stream;
        Pin::new(mut_ref).poll_write(cx, buf)
    }

    #[inline(always)]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        let mut_ref = &mut Pin::into_inner(self).write_stream;
        Pin::new(mut_ref).poll_flush(cx)
    }

    #[inline(always)]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        let mut_ref = &mut Pin::into_inner(self).write_stream;
        Pin::new(mut_ref).poll_shutdown(cx)
    }

    #[inline(always)]
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<Result<usize>> {
        let mut_ref = &mut Pin::into_inner(self).write_stream;
        Pin::new(mut_ref).poll_write_vectored(cx, bufs)
    }

    #[inline(always)]
    fn is_write_vectored(&self) -> bool {
        self.write_stream.is_write_vectored()
    }
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> AsyncBufRead for IpcStream<R, W> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<&[u8]>> {
        let this = Pin::into_inner(self);
        Pin::new(&mut this.read_stream).poll_fill_buf(cx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let this = Pin::into_inner(self);
        Pin::new(&mut this.read_stream).consume(amt)
    }
}

impl IpcStream<Stdin, Stdout> {
    #[inline]
    pub fn parent_stream() -> Self {
        Self {
            read_stream:  BufReader::new( tokio::io::stdin() ),
            write_stream: BufWriter::new( tokio::io::stdout())
        }
    }
}
impl IpcStream<ChildStdout, ChildStdin> {
    #[inline]
    pub fn connect_to_child(child: &mut Child) -> Option<Self> {
        Some(Self {
            read_stream:  BufReader::new(child.stdout.take()?),
            write_stream: BufWriter::new(child.stdin .take()?)
        })
    }
}

macro_rules! debug_unreachable {
    () => {
        debug_unreachable!("entered unreachable code")
    };
    ($e:expr) => {
        if cfg!(debug_assertions) {
            panic!($e);
        } else {
            std::hint::unreachable_unchecked();
        }
    };
}
pub(crate) use debug_unreachable;