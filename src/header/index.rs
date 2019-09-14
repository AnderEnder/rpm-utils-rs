use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};
use std::io;
use std::io::{Read, Seek};

use crate::header::Tag;

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive)]
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
        let tag_id = i32::from_be_bytes(tag_be);
        let tag = Tag::from_i32(tag_id).unwrap_or_else(|| {
            println!("Unknown tag {}", tag_id);
            Tag::Other
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
            offset,
            count,
        })
    }
}
