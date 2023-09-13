use std::io;
use std::io::{Read, Write};
use crate::blocking::IpcStream as BlockingStream;

pub trait BlockingSerde: ToOwned {
    fn write_to(&self, stream: &mut BlockingStream<impl Read, impl Write>) -> io::Result<()>;
    fn from_reader(stream: &mut BlockingStream<impl Read, impl Write>) -> io::Result<Self::Owned>;
}
impl BlockingSerde for str {
    #[inline(always)]
    fn write_to(&self, stream: &mut BlockingStream<impl Read, impl Write>) -> io::Result<()> {
        stream.write_str(self)
    }
    #[inline(always)]
    fn from_reader(stream: &mut BlockingStream<impl Read, impl Write>) -> io::Result<Self::Owned> {
        stream.read_string()
    }
}
impl BlockingSerde for [u8] {
    #[inline(always)]
    fn write_to(&self, stream: &mut BlockingStream<impl Read, impl Write>) -> io::Result<()> {
        stream.write_buf(self)
    }
    #[inline(always)]
    fn from_reader(stream: &mut BlockingStream<impl Read, impl Write>) -> io::Result<Self::Owned> {
        stream.read_buff()
    }
}

macro_rules! serde_primitive {
    ($($primitive: ty)*) => {
        paste::paste! {$(
        impl BlockingSerde for $primitive {
            #[inline(always)]
            fn write_to(&self, stream: &mut BlockingStream<impl Read, impl Write>) -> io::Result<()> {
                stream.[<write_ $primitive>](*self)
            }
            #[inline(always)]
            fn from_reader(stream: &mut BlockingStream<impl Read, impl Write>) -> io::Result<Self::Owned> {
                stream.[<read_ $primitive>]()
            }
        })*}
    };
}

serde_primitive! {u64 u32 u16 u8 usize i64 i32 i16 i8 isize f64 f32}