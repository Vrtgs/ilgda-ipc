use std::{
    pin::Pin,
    task::{Context, Poll},
    io::{
        IoSlice,
        Result,
        Error as IoError,
        ErrorKind as IoErrorKind,
    }
};
use tokio::{
    io::{
        AsyncRead, AsyncReadExt, ReadBuf,
        AsyncWrite, AsyncWriteExt,
        Stdout,
        Stdin,
    },
    process::{
        Child,
        ChildStdout,
        ChildStdin,
    }
};
use tokio::io::{AsyncBufRead, BufReader, BufWriter};

pub struct IpcStream<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> {
    pub(crate) read_stream: BufReader<R>,
    pub(crate) write_stream: BufWriter<W>,
}

macro_rules! gen_int_rw {
    ($($r#type: ident)*) => {
        paste::paste! {$(

        #[doc = concat!("writes a ", stringify!($r#type), " in native-endian order to the underlying writer")]
        #[inline(always)]
        pub async fn [<write_ $r#type>](&mut self, val: $r#type) -> Result<()> {
            self.write_all(&val.to_ne_bytes()).await
        }
        #[doc = concat!("writes a ", stringify!($r#type), " in big-endian order to the underlying writer")]
        #[inline(always)]
        pub async fn [<write_ $r#type _be>](&mut self, val: $r#type) -> Result<()> {
            self.write_all(&val.to_be_bytes()).await
        }
        #[doc = concat!("writes a ", stringify!($r#type), " in little-endian order to the underlying writer")]
        #[inline(always)]
        pub async fn [<write_ $r#type _le>](&mut self, val: $r#type) -> Result<()> {
            self.write_all(&val.to_le_bytes()).await
        }
        #[doc = concat!("reads a ", stringify!($r#type), " in native-endian order from the underlying reader")]
        #[inline(always)]
        pub async fn [<read_ $r#type>](&mut self) -> Result<$r#type> {
            let mut buff = [0; std::mem::size_of::<$r#type>()];
            self.read_exact(&mut buff).await?;
            Ok($r#type::from_ne_bytes(buff))
        }
        #[doc = concat!("reads a ", stringify!($r#type), " in big-endian order from the underlying reader")]
        #[inline(always)]
        pub async fn [<read_ $r#type _be>](&mut self) -> Result<$r#type> {
            let mut buff = [0; std::mem::size_of::<$r#type>()];
            self.read_exact(&mut buff).await?;
            Ok($r#type::from_be_bytes(buff))
        }
        #[doc = concat!("reads a ", stringify!($r#type), " in little-endian order from the underlying reader")]
        #[inline(always)]
        pub async fn [<read_ $r#type _le>](&mut self) -> Result<$r#type> {
            let mut buff = [0; std::mem::size_of::<$r#type>()];
            self.read_exact(&mut buff).await?;
            Ok($r#type::from_le_bytes(buff))
        }
        )*}
    };
}
macro_rules! gen_float_rw {
    ($($r#type: ident $bits_type: ident)*) => {
        paste::paste! {$(
        #[doc = concat!("writes a ", stringify!($r#type), " in native-endian order to the underlying writer")]
        #[inline(always)]
        pub async fn [<write_ $r#type>](&mut self, val: $r#type) -> Result<()> {
            self.[<write_ $bits_type>](val.to_bits()).await
        }
        #[doc = concat!("writes a ", stringify!($r#type), " in big-endian order to the underlying writer")]
        #[inline(always)]
        pub async fn [<write_ $r#type _be>](&mut self, val: $r#type) -> Result<()> {
            self.[<write_ $bits_type _be>](val.to_bits()).await
        }
        #[doc = concat!("writes a ", stringify!($r#type), " in little-endian order to the underlying writer")]
        #[inline(always)]
        pub async fn [<write_ $r#type _le>](&mut self, val: $r#type) -> Result<()> {
            self.[<write_ $bits_type _le>](val.to_bits()).await
        }

        #[doc = concat!("reads a ", stringify!($r#type), " in native-endian order from the underlying reader")]
        #[inline(always)]
        pub async fn [<read_ $r#type>](&mut self) -> Result<$r#type> {
            // inlined for performance reasons
            let mut buff = [0; std::mem::size_of::<$bits_type>()];
            self.read_exact(&mut buff).await?;
            Ok($r#type::from_bits($bits_type::from_ne_bytes(buff)))
        }

        #[doc = concat!("reads a ", stringify!($r#type), " in big-endian order from the underlying reader")]
        #[inline(always)]
        pub async fn [<read_ $r#type _be>](&mut self) -> Result<$r#type> {
            // inlined for performance reasons
            let mut buff = [0; std::mem::size_of::<$bits_type>()];
            self.read_exact(&mut buff).await?;
            Ok($r#type::from_bits($bits_type::from_be_bytes(buff)))
        }

        #[doc = concat!("reads a ", stringify!($r#type), " in little-endian order from the underlying reader")]
        #[inline(always)]
        pub async fn [<read_ $r#type _le>](&mut self) -> Result<$r#type> {
            // inlined for performance reasons
            let mut buff = [0; std::mem::size_of::<$bits_type>()];
            self.read_exact(&mut buff).await?;
            Ok($r#type::from_bits($bits_type::from_le_bytes(buff)))
        }
        )*}
    };
}
macro_rules! read_stream {
    ($stream: expr, $meth: ident, $r#type: ident) => {{
        let stream = $stream;
        let len = stream.read_usize().await?;
        let data = {
            let mut buf = $r#type::with_capacity(len);
            let mut stream = stream.take(len as u64);
            stream.$meth(&mut buf).await?;

            if buf.len() != len {
                return Err(IoError::new(
                    IoErrorKind::UnexpectedEof,
                    format!("expected {len} extra bytes found {} in stream", buf.len())
                ))
            }
            buf
        };

        Ok(data)
    }};
}

impl<R: AsyncReadExt + Unpin, W: AsyncWriteExt + Unpin> IpcStream<R, W> {
    #[inline(always)]
    pub async fn write_buf(&mut self, buf: &[u8]) -> Result<()> {
        self.write_usize(buf.len()).await?;
        self.write_all(buf).await
    }

    #[inline(always)]
    pub async fn write_str(&mut self, buf: &str) -> Result<()> {
        self.write_buf(buf.as_bytes()).await
    }

    pub async fn read_buff(&mut self) -> Result<Vec<u8>> {
        read_stream!(self, read_to_end, Vec)
    }

    pub async fn read_string(&mut self) -> Result<String> {
        read_stream!(self, read_to_string, String)
    }

    gen_int_rw! {u64 u32 u16 u8 usize i64 i32 i16 i8 isize  u128 i128}
    gen_float_rw! {f64 u64 f32 u32}
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> AsyncRead for IpcStream<R, W> {
    #[inline(always)]
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<()>> {
        Pin::new(&mut self.read_stream).poll_read(cx, buf)
    }
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> AsyncWrite for IpcStream<R, W> {
    #[inline(always)]
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize>> {
        Pin::new(&mut self.write_stream).poll_write(cx, buf)
    }

    #[inline(always)]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.write_stream).poll_flush(cx)
    }

    #[inline(always)]
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.write_stream).poll_shutdown(cx)
    }

    #[inline(always)]
    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<Result<usize>> {
        Pin::new(&mut self.write_stream).poll_write_vectored(cx, bufs)
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
            read_stream:  BufReader::new(tokio::io::stdin() ),
            write_stream: BufWriter::new(tokio::io::stdout())
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