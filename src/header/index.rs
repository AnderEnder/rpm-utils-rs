use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};
use std::io;
use std::io::{Read, Seek};
use strum_macros::Display;

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Display)]
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

impl From<RType> for String {
    fn from(t: RType) -> String {
        match t {
            RType::Null | RType::Bin(_) => String::default(),
            RType::Char(v) => v.to_string(),
            RType::String(v) | RType::I18nstring(v) => v.to_string(),
            RType::Int8(v) => v.to_string(),
            RType::Int16(v) => v.to_string(),
            RType::Int32(v) => v.to_string(),
            RType::Int64(v) => v.to_string(),
            RType::StringArray(v) => v.join(""),
            _ => String::default(),
        }
    }
}

impl From<RType> for u64 {
    fn from(t: RType) -> u64 {
        match t {
            RType::Int8(v) => u64::from(v),
            RType::Int16(v) => u64::from(v),
            RType::Int32(v) => u64::from(v),
            RType::Int64(v) => v,
            _ => Default::default(),
        }
    }
}

impl From<RType> for Vec<u64> {
    fn from(t: RType) -> Vec<u64> {
        match t {
            RType::Int8Array(v) => v.into_iter().map(|x| x.into()).collect(),
            RType::Int16Array(v) => v.into_iter().map(|x| x.into()).collect(),
            RType::Int32Array(v) => v.into_iter().map(|x| x.into()).collect(),
            RType::Int64Array(v) => v,
            _ => Default::default(),
        }
    }
}

impl From<RType> for Vec<u8> {
    fn from(t: RType) -> Vec<u8> {
        match t {
            RType::Bin(v) | RType::Int8Array(v) => v,
            _ => Default::default(),
        }
    }
}

macro_rules! from_rtype (
    ($from:path, $to:ty) => (
        impl From<RType> for $to {
            fn from(t: RType) -> $to {
                match t {
                    $from(v) => v,
                    _ => Default::default(),
                }
            }
        }
    );
);

from_rtype!(RType::Char, char);
from_rtype!(RType::Int8, u8);
from_rtype!(RType::Int16, u16);
from_rtype!(RType::Int32, u32);
from_rtype!(RType::Int16Array, Vec<u16>);
from_rtype!(RType::Int32Array, Vec<u32>);
from_rtype!(RType::StringArray, Vec<String>);

#[derive(Debug)]
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
    pub fn read<R: Read + Seek>(fh: &mut R) -> Result<Self, io::Error> {
        let mut tag_be = [0_u8; 4];
        fh.read_exact(&mut tag_be)?;
        let tag_id = u32::from_be_bytes(tag_be);
        let tag = T::from_u32(tag_id).unwrap_or_else(|| {
            println!("Unknown tag {}", tag_id);
            T::default()
        });

        let mut itype_be = [0_u8; 4];
        fh.read_exact(&mut itype_be)?;

        let type_id = u32::from_be_bytes(itype_be);
        let itype = Type::from_u32(type_id).unwrap_or_else(|| {
            println!("Unknown type {}", type_id);
            Type::Null
        });

        let mut offset_be = [0_u8; 4];
        fh.read_exact(&mut offset_be)?;
        let offset = u32::from_be_bytes(offset_be);

        let mut count_be = [0_u8; 4];
        fh.read_exact(&mut count_be)?;
        let count = u32::from_be_bytes(count_be);

        Ok(Index {
            tag,
            itype,
            offset: offset as usize,
            count: count as usize,
        })
    }
}

pub struct IndexArray;

impl IndexArray {
    pub fn read<R, T>(fh: &mut R, nindex: usize) -> Result<Vec<Index<T>>, io::Error>
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
