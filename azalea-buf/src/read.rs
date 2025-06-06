use std::{
    backtrace::Backtrace,
    collections::HashMap,
    hash::Hash,
    io::{self, Cursor, Read},
    sync::Arc,
};

use byteorder::{BE, ReadBytesExt};
use thiserror::Error;
use tracing::warn;

use super::{MAX_STRING_LENGTH, UnsizedByteArray};

#[derive(Error, Debug)]
pub enum BufReadError {
    #[error("Invalid VarInt")]
    InvalidVarInt,
    #[error("Invalid VarLong")]
    InvalidVarLong,
    #[error("Error reading bytes")]
    CouldNotReadBytes,
    #[error(
        "The received encoded string buffer length is longer than maximum allowed ({length} > {max_length})"
    )]
    StringLengthTooLong { length: u32, max_length: u32 },
    #[error("The received Vec length is longer than maximum allowed ({length} > {max_length})")]
    VecLengthTooLong { length: u32, max_length: u32 },
    #[error("{source}")]
    Io {
        #[from]
        #[backtrace]
        source: io::Error,
    },
    #[error("Invalid UTF-8: {bytes:?} (lossy: {lossy:?})")]
    InvalidUtf8 {
        bytes: Vec<u8>,
        lossy: String,
        // backtrace: Backtrace,
    },
    #[error("Unexpected enum variant {id}")]
    UnexpectedEnumVariant { id: i32 },
    #[error("Unexpected enum variant {id}")]
    UnexpectedStringEnumVariant { id: String },
    #[error("Tried to read {attempted_read} bytes but there were only {actual_read}")]
    UnexpectedEof {
        attempted_read: usize,
        actual_read: usize,
        backtrace: Backtrace,
    },
    #[error("{0}")]
    Custom(String),
    #[cfg(feature = "serde_json")]
    #[error("{source}")]
    Deserialization {
        #[from]
        #[backtrace]
        source: serde_json::Error,
    },
    #[error("{source}")]
    Nbt {
        #[from]
        #[backtrace]
        source: simdnbt::Error,
    },
    #[error("{source}")]
    DeserializeNbt {
        #[from]
        #[backtrace]
        source: simdnbt::DeserializeError,
    },
}

fn read_bytes<'a>(buf: &'a mut Cursor<&[u8]>, length: usize) -> Result<&'a [u8], BufReadError> {
    if length > (buf.get_ref().len() - buf.position() as usize) {
        return Err(BufReadError::UnexpectedEof {
            attempted_read: length,
            actual_read: buf.get_ref().len() - buf.position() as usize,
            backtrace: Backtrace::capture(),
        });
    }
    let initial_position = buf.position() as usize;
    buf.set_position(buf.position() + length as u64);
    let data = &buf.get_ref()[initial_position..initial_position + length];
    Ok(data)
}

fn read_utf_with_len(buf: &mut Cursor<&[u8]>, max_length: u32) -> Result<String, BufReadError> {
    let length = u32::azalea_read_var(buf)?;
    // i don't know why it's multiplied by 4 but it's like that in mojang's code so
    if length > max_length * 4 {
        return Err(BufReadError::StringLengthTooLong {
            length,
            max_length: max_length * 4,
        });
    }

    let buffer = read_bytes(buf, length as usize)?;
    let string = std::str::from_utf8(buffer)
        .map_err(|_| BufReadError::InvalidUtf8 {
            bytes: buffer.to_vec(),
            lossy: String::from_utf8_lossy(buffer).to_string(),
            // backtrace: Backtrace::capture(),
        })?
        .to_string();
    if string.len() > length as usize {
        return Err(BufReadError::StringLengthTooLong { length, max_length });
    }

    Ok(string)
}

pub trait AzaleaRead
where
    Self: Sized,
{
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError>;
}

pub trait AzaleaReadVar
where
    Self: Sized,
{
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError>;
}

// note that there's no Write equivalent for this trait since we don't really
// care if we're writing over the limit (and maybe we already know that the
// server implementation accepts it)
pub trait AzaleaReadLimited
where
    Self: Sized,
{
    fn azalea_read_limited(buf: &mut Cursor<&[u8]>, limit: usize) -> Result<Self, BufReadError>;
}

impl AzaleaRead for i32 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(buf.read_i32::<BE>()?)
    }
}

impl AzaleaReadVar for i32 {
    // fast varints modified from https://github.com/luojia65/mc-varint/blob/master/src/lib.rs#L67
    /// Read a single varint from the reader and return the value
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        let mut buffer = [0];
        let mut ans = 0;
        for i in 0..5 {
            buf.read_exact(&mut buffer)?;
            ans |= ((buffer[0] & 0b0111_1111) as i32) << (7 * i);
            if buffer[0] & 0b1000_0000 == 0 {
                break;
            }
        }
        Ok(ans)
    }
}

impl AzaleaReadVar for i64 {
    // fast varints modified from https://github.com/luojia65/mc-varint/blob/master/src/lib.rs#L54
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        let mut buffer = [0];
        let mut ans = 0;
        for i in 0..10 {
            buf.read_exact(&mut buffer)
                .map_err(|_| BufReadError::InvalidVarLong)?;
            ans |= ((buffer[0] & 0b0111_1111) as i64) << (7 * i);
            if buffer[0] & 0b1000_0000 == 0 {
                break;
            }
        }
        Ok(ans)
    }
}
impl AzaleaReadVar for u64 {
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        i64::azalea_read_var(buf).map(|i| i as u64)
    }
}

impl AzaleaRead for UnsizedByteArray {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        // read to end of the buffer
        let data = buf.get_ref()[buf.position() as usize..].to_vec();
        buf.set_position((buf.position()) + data.len() as u64);
        Ok(UnsizedByteArray(data))
    }
}

impl<T: AzaleaRead> AzaleaRead for Vec<T> {
    default fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        let length = u32::azalea_read_var(buf)? as usize;
        // we limit the capacity to not get exploited into allocating a bunch
        let mut contents = Vec::with_capacity(usize::min(length, 65536));
        for _ in 0..length {
            contents.push(T::azalea_read(buf)?);
        }
        Ok(contents)
    }
}
impl<T: AzaleaRead> AzaleaRead for Box<[T]> {
    default fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Vec::<T>::azalea_read(buf).map(Vec::into_boxed_slice)
    }
}
impl<T: AzaleaRead> AzaleaReadLimited for Vec<T> {
    fn azalea_read_limited(buf: &mut Cursor<&[u8]>, limit: usize) -> Result<Self, BufReadError> {
        let length = u32::azalea_read_var(buf)? as usize;
        if length > limit {
            return Err(BufReadError::VecLengthTooLong {
                length: length as u32,
                max_length: limit as u32,
            });
        }

        let mut contents = Vec::with_capacity(usize::min(length, 65536));
        for _ in 0..length {
            contents.push(T::azalea_read(buf)?);
        }
        Ok(contents)
    }
}
impl<T: AzaleaRead> AzaleaReadLimited for Box<[T]> {
    fn azalea_read_limited(buf: &mut Cursor<&[u8]>, limit: usize) -> Result<Self, BufReadError> {
        Vec::<T>::azalea_read_limited(buf, limit).map(Vec::into_boxed_slice)
    }
}

impl<K: AzaleaRead + Send + Eq + Hash, V: AzaleaRead + Send> AzaleaRead for HashMap<K, V> {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        let length = i32::azalea_read_var(buf)? as usize;
        let mut contents = HashMap::with_capacity(usize::min(length, 65536));
        for _ in 0..length {
            contents.insert(K::azalea_read(buf)?, V::azalea_read(buf)?);
        }
        Ok(contents)
    }
}

impl<K: AzaleaRead + Send + Eq + Hash, V: AzaleaReadVar + Send> AzaleaReadVar for HashMap<K, V> {
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        let length = i32::azalea_read_var(buf)? as usize;
        let mut contents = HashMap::with_capacity(usize::min(length, 65536));
        for _ in 0..length {
            contents.insert(K::azalea_read(buf)?, V::azalea_read_var(buf)?);
        }
        Ok(contents)
    }
}

impl AzaleaRead for Vec<u8> {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        let length = i32::azalea_read_var(buf)? as usize;
        read_bytes(buf, length).map(|b| b.to_vec())
    }
}

impl AzaleaRead for String {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        read_utf_with_len(buf, MAX_STRING_LENGTH.into())
    }
}
impl AzaleaReadLimited for String {
    fn azalea_read_limited(buf: &mut Cursor<&[u8]>, limit: usize) -> Result<Self, BufReadError> {
        read_utf_with_len(buf, limit as u32)
    }
}

impl AzaleaRead for u32 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(i32::azalea_read(buf)? as u32)
    }
}

impl AzaleaReadVar for u32 {
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(i32::azalea_read_var(buf)? as u32)
    }
}

impl AzaleaRead for u16 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        i16::azalea_read(buf).map(|i| i as u16)
    }
}

impl AzaleaRead for i16 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(buf.read_i16::<BE>()?)
    }
}

impl AzaleaReadVar for u16 {
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(i32::azalea_read_var(buf)? as u16)
    }
}

impl<T: AzaleaReadVar> AzaleaReadVar for Vec<T> {
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        let length = i32::azalea_read_var(buf)? as usize;
        let mut contents = Vec::with_capacity(usize::min(length, 65536));
        for _ in 0..length {
            contents.push(T::azalea_read_var(buf)?);
        }
        Ok(contents)
    }
}
impl<T: AzaleaReadVar> AzaleaReadVar for Box<[T]> {
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Vec::<T>::azalea_read_var(buf).map(Vec::into_boxed_slice)
    }
}

impl AzaleaRead for i64 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(buf.read_i64::<BE>()?)
    }
}

impl AzaleaRead for u64 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        i64::azalea_read(buf).map(|i| i as u64)
    }
}

impl AzaleaRead for bool {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        let byte = u8::azalea_read(buf)?;
        if byte > 1 {
            warn!("Boolean value was not 0 or 1, but {}", byte);
        }
        Ok(byte != 0)
    }
}

impl AzaleaRead for u8 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(buf.read_u8()?)
    }
}

impl AzaleaRead for i8 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        u8::azalea_read(buf).map(|i| i as i8)
    }
}

impl AzaleaRead for f32 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(buf.read_f32::<BE>()?)
    }
}

impl AzaleaRead for f64 {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(buf.read_f64::<BE>()?)
    }
}

impl<T: AzaleaRead> AzaleaRead for Option<T> {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        let present = bool::azalea_read(buf)?;
        Ok(if present {
            Some(T::azalea_read(buf)?)
        } else {
            None
        })
    }
}

impl<T: AzaleaReadVar> AzaleaReadVar for Option<T> {
    fn azalea_read_var(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        let present = bool::azalea_read(buf)?;
        Ok(if present {
            Some(T::azalea_read_var(buf)?)
        } else {
            None
        })
    }
}
impl<T: AzaleaReadLimited> AzaleaReadLimited for Option<T> {
    fn azalea_read_limited(buf: &mut Cursor<&[u8]>, limit: usize) -> Result<Self, BufReadError> {
        let present = bool::azalea_read(buf)?;
        Ok(if present {
            Some(T::azalea_read_limited(buf, limit)?)
        } else {
            None
        })
    }
}

// [String; 4]
impl<T: AzaleaRead, const N: usize> AzaleaRead for [T; N] {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        let mut contents = Vec::with_capacity(N);
        for _ in 0..N {
            contents.push(T::azalea_read(buf)?);
        }
        contents.try_into().map_err(|_| {
            unreachable!("Panic is not possible since the Vec is the same size as the array")
        })
    }
}

impl AzaleaRead for simdnbt::owned::NbtTag {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(simdnbt::owned::read_tag(buf).map_err(simdnbt::Error::from)?)
    }
}

impl AzaleaRead for simdnbt::owned::NbtCompound {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        match simdnbt::owned::read_tag(buf).map_err(simdnbt::Error::from)? {
            simdnbt::owned::NbtTag::Compound(compound) => Ok(compound),
            _ => Err(BufReadError::Custom("Expected compound tag".to_string())),
        }
    }
}

impl AzaleaRead for simdnbt::owned::Nbt {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(simdnbt::owned::read_unnamed(buf)?)
    }
}

impl<T> AzaleaRead for Box<T>
where
    T: AzaleaRead,
{
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(Box::new(T::azalea_read(buf)?))
    }
}

impl<A: AzaleaRead, B: AzaleaRead> AzaleaRead for (A, B) {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok((A::azalea_read(buf)?, B::azalea_read(buf)?))
    }
}

impl<T: AzaleaRead> AzaleaRead for Arc<T> {
    fn azalea_read(buf: &mut Cursor<&[u8]>) -> Result<Self, BufReadError> {
        Ok(Arc::new(T::azalea_read(buf)?))
    }
}
