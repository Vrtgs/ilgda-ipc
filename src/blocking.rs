use std::{
    io::{
        Result,
        Error as IoError,
        ErrorKind as IoErrorKind,
        Read, Write,
        StdinLock, StdoutLock,
        BufReader, BufWriter
    },
    process::{Child, ChildStdin, ChildStdout}
};
use std::io::BufRead;

pub struct IpcStream<R: Read, W: Write> {
    input_stream: BufReader<R>,
    output_stream: BufWriter<W>,
}


macro_rules! gen_int_rw {
    ($($r#type: ident)*) => {
        paste::paste! {$(
        #[doc = concat!("writes a ", stringify!($r#type), " in native-endian order to the underlying writer")]
        #[inline(always)]
        pub fn [<write_ $r#type>](&mut self, val: $r#type) -> Result<()> {
            self.write_all(&val.to_ne_bytes())
        }
        #[doc = concat!("writes a ", stringify!($r#type), " in big-endian order to the underlying writer")]
        #[inline(always)]
        pub fn [<write_ $r#type _be>](&mut self, val: $r#type) -> Result<()> {
            self.write_all(&val.to_be_bytes())
        }
        #[doc = concat!("writes a ", stringify!($r#type), " in little-endian order to the underlying writer")]
        #[inline(always)]
        pub fn [<write_ $r#type _le>](&mut self, val: $r#type) -> Result<()> {
            self.write_all(&val.to_le_bytes())
        }
        #[doc = concat!("reads a ", stringify!($r#type), " in native-endian order from the underlying reader")]
        #[inline(always)]
        pub fn [<read_ $r#type>](&mut self) -> Result<$r#type> {
            let mut buff = [0; std::mem::size_of::<$r#type>()];
            self.input_stream.read_exact(&mut buff)?;
            Ok($r#type::from_ne_bytes(buff))
        }
        #[doc = concat!("reads a ", stringify!($r#type), " in big-endian order from the underlying reader")]
        #[inline(always)]
        pub fn [<read_ $r#type _be>](&mut self) -> Result<$r#type> {
            let mut buff = [0; std::mem::size_of::<$r#type>()];
            self.input_stream.read_exact(&mut buff)?;
            Ok($r#type::from_be_bytes(buff))
        }
        #[doc = concat!("reads a ", stringify!($r#type), " in little-endian order from the underlying reader")]
        #[inline(always)]
        pub fn [<read_ $r#type _le>](&mut self) -> Result<$r#type> {
            let mut buff = [0; std::mem::size_of::<$r#type>()];
            self.input_stream.read_exact(&mut buff)?;
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
        pub fn [<write_ $r#type>](&mut self, val: $r#type) -> Result<()> {
            self.[<write_ $bits_type>](val.to_bits())
        }
        #[doc = concat!("writes a ", stringify!($r#type), " in big-endian order to the underlying writer")]
        #[inline(always)]
        pub fn [<write_ $r#type _be>](&mut self, val: $r#type) -> Result<()> {
            self.[<write_ $bits_type _be>](val.to_bits())
        }
        #[doc = concat!("writes a ", stringify!($r#type), " in little-endian order to the underlying writer")]
        #[inline(always)]
        pub fn [<write_ $r#type _le>](&mut self, val: $r#type) -> Result<()> {
            self.[<write_ $bits_type _le>](val.to_bits())
        }

        #[doc = concat!("reads a ", stringify!($r#type), " in native-endian order from the underlying reader")]
        #[inline(always)]
        pub fn [<read_ $r#type>](&mut self) -> Result<$r#type> {
            // inlined for performance reasons
            let mut buff = [0; std::mem::size_of::<$bits_type>()];
            self.input_stream.read_exact(&mut buff)?;
            Ok($r#type::from_bits($bits_type::from_ne_bytes(buff)))
        }

        #[doc = concat!("reads a ", stringify!($r#type), " in big-endian order from the underlying reader")]
        #[inline(always)]
        pub fn [<read_ $r#type _be>](&mut self) -> Result<$r#type> {
            // inlined for performance reasons
            let mut buff = [0; std::mem::size_of::<$bits_type>()];
            self.input_stream.read_exact(&mut buff)?;
            Ok($r#type::from_bits($bits_type::from_be_bytes(buff)))
        }

        #[doc = concat!("reads a ", stringify!($r#type), " in little-endian order from the underlying reader")]
        #[inline(always)]
        pub fn [<read_ $r#type _le>](&mut self) -> Result<$r#type> {
            // inlined for performance reasons
            let mut buff = [0; std::mem::size_of::<$bits_type>()];
            self.input_stream.read_exact(&mut buff)?;
            Ok($r#type::from_bits($bits_type::from_le_bytes(buff)))
        }
        )*}
    };
}
macro_rules! read_stream {
    ($stream: expr, $meth: ident, $r#type: ident) => {{
        let stream = $stream;
        let len = stream.read_usize()?;
        let data = {
            let mut buf = $r#type::with_capacity(len);
            let mut stream = stream.take(len as u64);
            stream.$meth(&mut buf)?;

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

impl<R: Read, W: Write> IpcStream<R, W> {
    #[inline(always)]
    pub fn write_buf(&mut self, buf: &[u8]) -> Result<()> {
        self.output_stream.write_all(&buf.len().to_ne_bytes())?;
        self.output_stream.write_all(buf)
    }

    #[inline(always)]
    pub fn write_str(&mut self, buf: &str) -> Result<()> {
        self.write_buf(buf.as_bytes())
    }

    pub fn read_buff(&mut self) -> Result<Vec<u8>> {
        read_stream!(self, read_to_end, Vec)
    }

    pub fn read_string(&mut self) -> Result<String> {
        read_stream!(self, read_to_string, String)
    }

    gen_int_rw! {u64 u32 u16 u8 usize i64 i32 i16 i8 isize}
    gen_float_rw! {f64 u64 f32 u32}
}

impl<R: Read, W: Write> Read for IpcStream<R, W> {
    #[inline(always)]
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.input_stream.read(buf)
    }
}
impl<R: Read, W: Write> Write for IpcStream<R, W> {
    #[inline(always)]
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.output_stream.write(buf)
    }
    #[inline(always)]
    fn flush(&mut self) -> Result<()> {
        self.output_stream.flush()
    }
}

impl<R: Read, W: Write> BufRead for IpcStream<R, W> {
    fn fill_buf(&mut self) -> Result<&[u8]> {
        self.input_stream.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.input_stream.consume(amt)
    }
}

impl IpcStream<StdinLock<'static>, StdoutLock<'static>> {
    #[inline]
    pub fn parent_stream() -> Self {
        Self {
            input_stream:  BufReader::new(std::io::stdin().lock() ),
            output_stream: BufWriter::new(std::io::stdout().lock())
        }
    }
}

impl IpcStream<ChildStdout, ChildStdin> {
    #[inline]
    pub fn connect_to_child(child: &mut Child<>) -> Option<Self> {
        Some(Self {
            input_stream : BufReader::new(child.stdout.take()?),
            output_stream: BufWriter::new(child.stdin.take()? )
        })
    }
}