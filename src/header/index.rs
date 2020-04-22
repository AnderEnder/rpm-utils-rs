use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};
use omnom::prelude::*;
use std::convert::TryFrom;
use std::io;
use std::io::{Read, Seek, Write};
use strum_macros::Display;

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Display, Clone)]
pub enum Type {
    Null = 0,
    Char = 1,
    Int8 = 2,
    Int16 = 3,
    Int32 = 4,
    Int64 = 5,
    String = 6,
    Bin = 7,
    StringArray = 8,
    I18nstring = 9,
}

impl Default for Type {
    fn default() -> Self {
        Type::Null
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum RType {
    Null,
    Char(char),
    Int8(u8),
    Int8Array(Vec<u8>),
    Int16(u16),
    Int16Array(Vec<u16>),
    Int32(u32),
    Int32Array(Vec<u32>),
    Int64(u64),
    Int64Array(Vec<u64>),
    String(String),
    Bin(Vec<u8>),
    StringArray(Vec<String>),
    I18nstring(String),
}

impl RType {
    pub fn as_string(&self) -> Option<String> {
        match self {
            RType::Null => Some(Default::default()),
            RType::Bin(b) => Some(format!("{:x?}", b)),
            RType::Char(s) => Some(s.to_string()),
            RType::String(s) | RType::I18nstring(s) => Some(s.to_owned()),
            RType::Int8(n) => Some(n.to_string()),
            RType::Int16(n) => Some(n.to_string()),
            RType::Int32(n) => Some(n.to_string()),
            RType::Int64(n) => Some(n.to_string()),
            RType::StringArray(a) => Some(a.join(",")),
            _ => None,
        }
    }

    pub fn as_string_array(&self) -> Option<Vec<String>> {
        match self {
            RType::StringArray(a) => Some(a.clone()),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            RType::Int8(n) => Some(u64::from(*n)),
            RType::Int16(n) => Some(u64::from(*n)),
            RType::Int32(n) => Some(u64::from(*n)),
            RType::Int64(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_u64_array(&self) -> Option<Vec<u64>> {
        match self {
            RType::Int8Array(a) => Some(a.iter().map(|x| u64::from(*x)).collect()),
            RType::Int16Array(a) => Some(a.iter().map(|x| u64::from(*x)).collect()),
            RType::Int32Array(a) => Some(a.iter().map(|x| u64::from(*x)).collect()),
            RType::Int64Array(a) => Some(a.clone()),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self {
            RType::Int8(n) => Some(u32::from(*n)),
            RType::Int16(n) => Some(u32::from(*n)),
            RType::Int32(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            RType::Int8(n) => Some(i64::from(*n)),
            RType::Int16(n) => Some(i64::from(*n)),
            RType::Int32(n) => Some(i64::from(*n)),
            _ => None,
        }
    }

    pub fn as_u32_array(&self) -> Option<Vec<u32>> {
        match self {
            RType::Int8Array(a) => Some(a.iter().map(|x| u32::from(*x)).collect()),
            RType::Int16Array(a) => Some(a.iter().map(|x| u32::from(*x)).collect()),
            RType::Int32Array(a) => Some(a.clone()),
            _ => None,
        }
    }

    pub fn as_u16(&self) -> Option<u16> {
        match self {
            RType::Int8(n) => Some(u16::from(*n)),
            RType::Int16(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_u16_array(&self) -> Option<Vec<u16>> {
        match self {
            RType::Int8Array(a) => Some(a.iter().map(|x| u16::from(*x)).collect()),
            RType::Int16Array(a) => Some(a.clone()),
            _ => None,
        }
    }

    pub fn as_u8(&self) -> Option<u8> {
        match self {
            RType::Int8(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_u8_array(&self) -> Option<Vec<u8>> {
        match self {
            RType::Int8Array(a) => Some(a.clone()),
            _ => None,
        }
    }

    pub fn as_char(&self) -> Option<char> {
        match self {
            RType::Char(n) => Some(*n),
            _ => None,
        }
    }
}

impl TryFrom<RType> for String {
    type Error = &'static str;

    fn try_from(value: RType) -> Result<Self, Self::Error> {
        value.as_string().ok_or("can not convert to string")
    }
}

impl TryFrom<RType> for Vec<String> {
    type Error = &'static str;

    fn try_from(value: RType) -> Result<Self, Self::Error> {
        value
            .as_string_array()
            .ok_or("can not convert to string array")
    }
}

impl TryFrom<RType> for u8 {
    type Error = &'static str;

    fn try_from(value: RType) -> Result<Self, Self::Error> {
        value.as_u8().ok_or("can not convert to u8")
    }
}

impl TryFrom<RType> for Vec<u8> {
    type Error = &'static str;

    fn try_from(value: RType) -> Result<Self, Self::Error> {
        value.as_u8_array().ok_or("can not convert to u8 array")
    }
}

impl TryFrom<RType> for u16 {
    type Error = &'static str;

    fn try_from(value: RType) -> Result<Self, Self::Error> {
        value.as_u16().ok_or("can not convert to u16")
    }
}

impl TryFrom<RType> for Vec<u16> {
    type Error = &'static str;

    fn try_from(value: RType) -> Result<Self, Self::Error> {
        value.as_u16_array().ok_or("can not convert to u16 array")
    }
}

impl TryFrom<RType> for u32 {
    type Error = &'static str;

    fn try_from(value: RType) -> Result<Self, Self::Error> {
        value.as_u32().ok_or("can not convert to u32")
    }
}

impl TryFrom<RType> for Vec<u32> {
    type Error = &'static str;

    fn try_from(value: RType) -> Result<Self, Self::Error> {
        value.as_u32_array().ok_or("can not convert to u32 array")
    }
}

impl TryFrom<RType> for u64 {
    type Error = &'static str;

    fn try_from(value: RType) -> Result<Self, Self::Error> {
        value.as_u64().ok_or("can not convert to u64")
    }
}

impl TryFrom<RType> for Vec<u64> {
    type Error = &'static str;

    fn try_from(value: RType) -> Result<Self, Self::Error> {
        value.as_u64_array().ok_or("can not convert to u64 array")
    }
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Index<T> {
    pub tag: T,
    pub itype: Type,
    pub offset: usize,
    pub count: usize,
}

impl<T> Index<T>
where
    T: FromPrimitive + Default,
{
    pub fn read<R: Read>(fh: &mut R) -> io::Result<Self> {
        let tag_id: u32 = fh.read_be()?;
        let tag = T::from_u32(tag_id).unwrap_or_else(|| {
            println!("Unknown tag {}", tag_id);
            T::default()
        });

        let type_id: u32 = fh.read_be()?;
        let itype = Type::from_u32(type_id).unwrap_or_else(|| {
            println!("Unknown type {}", type_id);
            Type::Null
        });

        let offset: u32 = fh.read_be()?;
        let count: u32 = fh.read_be()?;

        Ok(Index {
            tag,
            itype,
            offset: offset as usize,
            count: count as usize,
        })
    }
}

pub trait IndexWriter {
    fn write_index<T: ToPrimitive>(&mut self, index: Index<T>) -> io::Result<()>;
}

impl<W> IndexWriter for W
where
    W: Write,
{
    fn write_index<T: ToPrimitive>(&mut self, index: Index<T>) -> io::Result<()> {
        let tag_id = index
            .tag
            .to_u32()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Error: tag id is not correct"))?;
        self.write_be(tag_id)?;

        let itype = index.itype.to_u32().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Error: tag type is not defined")
        })?;
        self.write_be(itype)?;

        self.write_be(index.offset as u32)?;
        self.write_be(index.count as u32)?;
        Ok(())
    }
}

impl<T: Copy> Index<T> {
    pub fn from(tag: &T, rtype: &RType, offset: usize, count: usize) -> Self {
        let itype = match rtype {
            RType::Null => Type::Null,
            RType::Char(_) => Type::Char,
            RType::Int8(_) | RType::Int8Array(_) => Type::Int8,
            RType::Int16(_) | RType::Int16Array(_) => Type::Int16,
            RType::Int32(_) | RType::Int32Array(_) => Type::Int32,
            RType::Int64(_) | RType::Int64Array(_) => Type::Int64,
            RType::String(_) => Type::String,
            RType::Bin(_) => Type::String,
            RType::StringArray(_) => Type::StringArray,
            RType::I18nstring(_) => Type::I18nstring,
        };

        Index {
            itype,
            tag: *tag,
            offset,
            count,
        }
    }
}
pub struct IndexArray;

impl IndexArray {
    pub fn read<R, T>(fh: &mut R, nindex: usize) -> io::Result<Vec<Index<T>>>
    where
        R: Read + Seek,
        T: FromPrimitive + Default,
    {
        let mut indexes = Vec::with_capacity(nindex);
        for _ in 0..nindex {
            let index = Index::read(fh)?;
            indexes.push(index);
        }

        indexes.sort_by_key(|k| k.offset);
        Ok(indexes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::tags::*;
    use std::io::Cursor;

    #[test]
    fn test_index_read_write_smoke() {
        let index = Index {
            itype: Type::Int32,
            tag: Tag::BuildTime,
            offset: 10,
            count: 11,
        };

        let mut data: Vec<u8> = Vec::new();
        data.write_index(index.clone()).unwrap();

        let mut cursor = Cursor::new(data);
        let index2 = Index::read(&mut cursor).unwrap();

        assert_eq!(index, index2);
    }
}
