mod index;
mod lead;
mod sigtags;
mod tags;

pub use index::*;
pub use lead::*;
pub use sigtags::*;
pub use tags::*;

use num_traits::FromPrimitive;
use std::char;
use std::collections::HashMap;
use std::hash::Hash;
use std::io::{self, Read, Seek};
use std::mem::size_of;

#[derive(Debug, Default)]
pub struct Tags<T>(pub HashMap<T, RType>)
where
    T: Eq + Hash;

impl<T> Tags<T>
where
    T: FromPrimitive + Default + Eq + Hash + Copy,
{
    pub fn get<O>(&self, name: T) -> O
    where
        O: Default + From<RType>,
    {
        match self.0.get(&name) {
            Some(value) => value.clone().into(),
            _ => O::default(),
        }
    }
    pub fn read<R>(fh: &mut R, indexes: &[Index<T>], size: usize) -> Result<Self, io::Error>
    where
        R: Read + Seek,
    {
        let mut s_data = vec![0_u8; size];
        fh.read_exact(&mut s_data)?;

        let tags = Self::tags_from_raw(&indexes, &s_data);
        Ok(tags)
    }

    pub fn tags_from_raw(indexes: &[Index<T>], data: &[u8]) -> Self {
        let tags = (0..indexes.len())
            .map(|i| {
                let item = &indexes[i];
                let ps = item.offset;

                let tag_value = match item.itype {
                    Type::Null => RType::Null,
                    Type::Char => RType::Char(char::from_bytes(&data, ps)),
                    Type::Int8 => extract(data, ps, item.count, RType::Int8, RType::Int8Array),
                    Type::Int16 => extract(data, ps, item.count, RType::Int16, RType::Int16Array),
                    Type::Int32 => extract(data, ps, item.count, RType::Int32, RType::Int32Array),
                    Type::Int64 => extract(data, ps, item.count, RType::Int64, RType::Int64Array),

                    Type::String => {
                        let ps2 = indexes[i + 1].offset;
                        let v = parse_string(&data[ps..ps2]);
                        RType::String(v)
                    }

                    Type::Bin => {
                        let ps2 = ps + item.count;
                        let bytes = &data[ps..ps2];
                        RType::Bin(bytes.to_vec())
                    }

                    Type::StringArray => {
                        let ps2 = indexes[i + 1].offset;
                        let v = parse_strings(&data[ps..ps2], item.count);
                        RType::StringArray(v)
                    }

                    Type::I18nstring => {
                        let ps2 = indexes[i + 1].offset;
                        let v = parse_string(&data[ps..ps2]);
                        RType::I18nstring(v)
                    }
                };

                (item.tag, tag_value)
            })
            .collect();
        Tags(tags)
    }
}

fn parse_string(bytes: &[u8]) -> String {
    let position = bytes.iter().position(|&x| x == 0).unwrap_or(0);
    let bytes2 = &bytes[0..position];
    String::from_utf8_lossy(bytes2).to_string()
}

fn parse_strings(bytes: &[u8], count: usize) -> Vec<String> {
    bytes
        .split(|x| *x == 0)
        .take(count)
        .map(|b| String::from_utf8_lossy(b).to_string())
        .collect()
}

fn extract<T: FromBytes>(
    data: &[u8],
    position: usize,
    count: usize,
    single: fn(T) -> RType,
    multiple: fn(Vec<T>) -> RType,
) -> RType {
    if count > 1 {
        let values: Vec<T> = (0..count)
            .map(|i| T::from_bytes(&data, position + i * size_of::<T>()))
            .collect();
        multiple(values)
    } else {
        single(T::from_bytes(&data, position))
    }
}

trait FromBytes {
    fn from_bytes(data: &[u8], position: usize) -> Self;
}

impl FromBytes for u8 {
    fn from_bytes(data: &[u8], position: usize) -> u8 {
        u8::from_be_bytes([data[position]; 1])
    }
}

impl FromBytes for char {
    fn from_bytes(data: &[u8], position: usize) -> char {
        char::from_u32(u32::from_bytes(&data, position)).unwrap_or_default()
    }
}

macro_rules! from_bytes (
    ($item:ty, $number:expr) => (
        impl FromBytes for $item {
            fn from_bytes(data: &[u8], position: usize) -> $item {
                let mut bytes: [u8; $number] = Default::default();
                bytes.copy_from_slice(&data[position..position + $number]);
                <$item>::from_be_bytes(bytes)
            }
        }
    );
);

from_bytes!(u16, 2);
from_bytes!(u32, 4);
from_bytes!(u64, 8);
