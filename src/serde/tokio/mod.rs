mod de;
mod ser;

use std::future::Future;
use std::io;
use tokio::io::{AsyncRead, AsyncWrite, BufReader, BufWriter};
use crate::tokio::IpcStream as AsyncStream;


pub trait AsyncSerDe<'a, R: AsyncRead + Unpin, W: AsyncWrite  + Unpin>: ToOwned {
    type SerializeFuture  : Future<Output=io::Result<(         )>> + 'a;
    type DeserializeFuture: Future<Output=io::Result<Self::Owned>> + 'a;

    fn write_to(&'a self, stream: &'a mut AsyncStream<R, W>)
        -> Self::SerializeFuture;

    fn from_reader(stream: &'a mut AsyncStream<R, W>)
        -> Self::DeserializeFuture;
}

macro_rules! primitive_serde {
    ($ser: ident $de: ident $ty:ty) => {
        impl<'a, R: AsyncRead + Unpin +'a, W: AsyncWrite + Unpin + 'a> AsyncSerDe<'a, R, W> for $ty {
            type SerializeFuture = ser::$ser<&'a mut BufWriter<W>>;
            type DeserializeFuture = de::$de<&'a mut BufReader<R>>;

            fn write_to(&'a self, stream: &'a mut AsyncStream<R, W>) -> Self::SerializeFuture {
                ser::$ser::new(&mut stream.write_stream, *self)
            }

            fn from_reader(stream: &'a mut AsyncStream<R, W>) -> Self::DeserializeFuture {
                de::$de::new(&mut stream.read_stream)
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

primitive_serde!(SerializeIsize DeserializeIsize isize);
primitive_serde!(SerializeUsize DeserializeUsize usize);

