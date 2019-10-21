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
    Int8(i8),
    Int8Array(Vec<i8>),
    Int16(i16),
    Int16Array(Vec<i16>),
    Int32(i32),
    Int32Array(Vec<i32>),
    Int64(i64),
    Int64Array(Vec<i64>),
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

impl From<RType> for Vec<String> {
    fn from(t: RType) -> Vec<String> {
        match t {
            RType::StringArray(v) => v,
            _ => Vec::new(),
        }
    }
}

impl From<RType> for i32 {
    fn from(t: RType) -> i32 {
        match t {
            RType::Int32(v) => v,
            _ => i32::default(),
        }
    }
}

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
        let tag_id = i32::from_be_bytes(tag_be);
        let tag = T::from_i32(tag_id).unwrap_or_else(|| {
            println!("Unknown tag {}", tag_id);
            T::default()
        });

        let mut itype_be = [0_u8; 4];
        fh.read_exact(&mut itype_be)?;

        let type_id = i32::from_be_bytes(itype_be);
        let itype = Type::from_i32(type_id).unwrap_or_else(|| {
            println!("Unknown type {}", type_id);
            Type::Null
        });

        let mut offset_be = [0_u8; 4];
        fh.read_exact(&mut offset_be)?;
        let offset = i32::from_be_bytes(offset_be);

        let mut count_be = [0_u8; 4];
        fh.read_exact(&mut count_be)?;
        let count = i32::from_be_bytes(count_be);

        Ok(Index {
            tag,
            itype,
            offset: offset as usize,
            count: count as usize,
        })
    }
}
