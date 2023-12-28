use std::{
    fmt::{self, Debug, Formatter},
    num::{FpCategory, NonZeroU64}
};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{Error as DeserializeError, Expected, SeqAccess, Unexpected, Visitor}
};

use heap_array::HeapArray;

pub enum IlgdaId {
    Numeric(u64),
    String(Box<str>),
    Bytes(HeapArray<u8>)
}

macro_rules! impl_from {
    ($id_ty:ident |> $ty:ty $(|from> $({for $($constraints:tt)*})? $other_ty:ty)*) => {
        impl From<$ty> for IlgdaId {
            #[inline(always)]
            fn from(value: $ty) -> Self {
                IlgdaId::$id_ty(value)
            }
        }
        $(
        impl$(<$($constraints)*>)? From<$other_ty> for IlgdaId {
            #[inline]
            fn from(value: $other_ty) -> Self {
                IlgdaId::$id_ty(<$ty>::from(value))
            }
        }
        )*
    };
}

impl_from! { Numeric |> u64 |from> u32 |from> u16 |from> u8 |from> NonZeroU64 }
impl_from! { String  |> Box<str> |from> String |from> &str }
impl_from! {
    Bytes|> HeapArray<u8>
    |from>  Vec<u8>
    |from>  Box<[u8]>
    |from>  {for const N: usize} [u8; N]
    |from>  &[u8]
}

impl Debug for IlgdaId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let field: &dyn Debug = match self {
            IlgdaId::Numeric(num) => num,
            IlgdaId::String(str) => str,
            IlgdaId::Bytes(bytes) => bytes
        };

        f.debug_tuple("Id").field(field).finish()
    }
}

impl Serialize for IlgdaId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
        match self {
            IlgdaId::Numeric(num) => serializer.serialize_u64(*num),
            IlgdaId::String(str) => serializer.serialize_str(str),
            IlgdaId::Bytes(bytes) => serializer.serialize_bytes(bytes)
        }
    }
}

struct ExpectedUnsigned;
struct ExpectedSafeUnsignedInteger;

impl<'de> Deserialize<'de> for IlgdaId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IdVisitor;

        impl Expected for ExpectedUnsigned {
            #[inline]
            fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
                formatter.write_str("expected an unsigned integer")
            }
        }

        impl Expected for ExpectedSafeUnsignedInteger {
            #[inline]
            fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
                formatter.write_str("a float that can safely be represented as an unsigned integer")
            }
        }

        impl<'a> Visitor<'a> for IdVisitor {
            type Value = IlgdaId;

            fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
                formatter.write_str("expecting either an unsigned integer, string, or an array of bytes")
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> where E: DeserializeError {
                match u64::try_from(v) {
                    Ok(v) => Ok(IlgdaId::Numeric(v)),
                    Err(_) => Err(E::invalid_value(Unexpected::Signed(v), &ExpectedUnsigned))
                }
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> where E: DeserializeError {
                Ok(IlgdaId::Numeric(v))
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E> where E: DeserializeError {
                const MAX_SAFE_INTEGER: f64 = ((1_u64 << f64::MANTISSA_DIGITS) - 1) as f64;

                match v.classify() {
                    FpCategory::Zero => Ok(IlgdaId::Numeric(0)),
                    FpCategory::Normal if v.trunc() == v && (0.0..=MAX_SAFE_INTEGER).contains(&v) => Ok(IlgdaId::Numeric(v as u64)),
                    _ => Err(E::invalid_value(Unexpected::Float(v), &ExpectedSafeUnsignedInteger))
                }
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> where E: DeserializeError {
                Ok(IlgdaId::from(v))
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E> where E: DeserializeError {
                Ok(IlgdaId::from(v))
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E> where E: DeserializeError {
                Ok(IlgdaId::from(v))
            }

            fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E> where E: DeserializeError {
                Ok(IlgdaId::from(v))
            }

            fn visit_seq<A: SeqAccess<'a>>(self, seq: A) -> Result<Self::Value, A::Error> {
                Ok(IlgdaId::Bytes(HeapArray::from_sequence(seq)?))
            }
        }

        deserializer.deserialize_any(IdVisitor)
    }
}