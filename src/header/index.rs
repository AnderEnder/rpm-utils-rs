use std::io::{Read, Seek};
use std::io;

use crate::header::Tag;

#[derive(Debug, PartialEq)]
pub enum Type {
    Null,
    Char,
    Int8,
    Int16,
    Int32,
    Int64,
    String,
    Bin,
    StringArray,
}

impl From<i32> for Type {
    fn from(tag: i32) -> Self {
        match tag {
            0 => Type::Null,
            1 => Type::Char,
            2 => Type::Int8,
            3 => Type::Int16,
            4 => Type::Int32,
            5 => Type::Int64,
            6 => Type::String,
            7 => Type::Bin,
            8 => Type::StringArray,
            _ => Type::Null,
        }
    }
}

#[derive(Debug)]
pub struct Index {
    pub tag: Tag,
    pub itype: Type,
    pub offset: i32,
    pub count: i32,
}

impl Index {
    pub fn read<R: Read + Seek>(fh: &mut R) -> Result<Self, io::Error> {
        let mut tag_be = [0_u8; 4];
        fh.read_exact(&mut tag_be)?;
        let tag = i32::from_be_bytes(tag_be).into();

        let mut itype_be = [0_u8; 4];
        fh.read_exact(&mut itype_be)?;
        let itype = i32::from_be_bytes(itype_be).into();

        let mut offset_be = [0_u8; 4];
        fh.read_exact(&mut offset_be)?;
        let offset = i32::from_be_bytes(offset_be);

        let mut count_be = [0_u8; 4];
        fh.read_exact(&mut count_be)?;
        let count = i32::from_be_bytes(count_be);

        Ok(Index {
            tag,
            itype,
            offset,
            count,
        })
    }
}
