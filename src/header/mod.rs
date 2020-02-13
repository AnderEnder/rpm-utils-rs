mod index;
mod lead;
mod tags;

pub use index::*;
pub use lead::*;
pub use tags::*;

use num_traits::{FromPrimitive, ToPrimitive};
use omnom::prelude::*;
use omnom::ReadBytes;
use std::char;
use std::collections::HashMap;
use std::hash::Hash;
use std::io::{self, Read, Seek, Write};
use std::mem::size_of;

use crate::utils::{parse_string, parse_strings};

#[derive(Debug, Default)]
pub struct Tags<T>(pub HashMap<T, RType>)
where
    T: Eq + Hash;

impl<T> Tags<T>
where
    T: FromPrimitive + Default + Eq + Hash + Copy,
{
    pub fn new() -> Self {
        Tags(HashMap::new())
    }

    pub fn get_value(&self, name: T) -> Option<RType> {
        self.0.get(&name).cloned()
    }

    pub fn get<O>(&self, name: T) -> O
    where
        O: Default + From<RType>,
    {
        match self.0.get(&name) {
            Some(value) => value.clone().into(),
            _ => O::default(),
        }
    }

    pub fn insert(&mut self, key: T, value: RType) {
        self.0.insert(key, value);
    }

    pub fn get_as_string(&self, name: T) -> String {
        self.get_value(name)
            .expect("Tag: not found")
            .as_string()
            .expect("Tag: is not a string")
    }

    pub fn get_as_string_or(&self, name: T) -> String {
        if let Some(s) = self.get_value(name) {
            s.as_string().expect("Tag: is not a string")
        } else {
            Default::default()
        }
    }

    pub fn get_as_string_array_or(&self, name: T) -> Vec<String> {
        if let Some(s) = self.get_value(name) {
            s.as_string_array().expect("Tag: is not a string array")
        } else {
            Default::default()
        }
    }

    pub fn get_as_u8(&self, name: T) -> u8 {
        self.get_value(name)
            .expect("Tag: not found")
            .as_u8()
            .expect("Tag: is not a u8")
    }
    pub fn get_as_u8_default(&self, name: T) -> u8 {
        if let Some(s) = self.get_value(name) {
            s.as_u8().expect("Tag: is not a u8")
        } else {
            Default::default()
        }
    }

    pub fn get_as_u16(&self, name: T) -> u16 {
        self.get_value(name)
            .expect("Tag: not found")
            .as_u16()
            .expect("Tag: is not a u16")
    }

    pub fn get_as_u32(&self, name: T) -> u32 {
        self.get_value(name)
            .expect("Tag: not found")
            .as_u32()
            .expect("Tag: is not a integer")
    }

    pub fn get_as_u64(&self, name: T) -> u64 {
        self.get_value(name)
            .expect("Tag: not found")
            .as_u64()
            .expect("Tag: is not a integer")
    }

    pub fn get_as_i64(&self, name: T) -> i64 {
        self.get_value(name)
            .expect("Tag: not found")
            .as_i64()
            .expect("Tag: is not a integer")
    }

    pub fn get_as_u64_array_or(&self, name: T) -> Vec<u64> {
        if let Some(s) = self.get_value(name) {
            s.as_u64_array().expect("Tag: is not a u64 array")
        } else {
            Default::default()
        }
    }

    pub fn get_as_u32_array_or(&self, name: T) -> Vec<u32> {
        if let Some(s) = self.get_value(name) {
            s.as_u32_array().expect("Tag: is not a u32 array")
        } else {
            Default::default()
        }
    }

    pub fn get_as_u16_array_or(&self, name: T) -> Vec<u16> {
        if let Some(s) = self.get_value(name) {
            s.as_u16_array().expect("Tag: is not a u16 array")
        } else {
            Default::default()
        }
    }

    pub fn read<R>(fh: &mut R, indexes: &[Index<T>], size: usize) -> io::Result<Self>
    where
        R: Read + Seek,
    {
        let mut s_data = vec![0_u8; size];
        fh.read_exact(&mut s_data)?;

        Self::tags_from_raw(&indexes, &s_data)
    }

    fn tags_from_raw(indexes: &[Index<T>], data: &[u8]) -> io::Result<Self> {
        let tags = (0..indexes.len())
            .map(|i| {
                let item = &indexes[i];
                let ps = item.offset;

                let tag_value = match item.itype {
                    Type::Null => RType::Null,
                    Type::Char => {
                        let c_byte = (&data[ps..]).read_be()?;
                        let c = char::from_u32(c_byte).unwrap_or_default();
                        RType::Char(c)
                    }
                    Type::Int8 => extract(data, ps, item.count, RType::Int8, RType::Int8Array)?,
                    Type::Int16 => extract(data, ps, item.count, RType::Int16, RType::Int16Array)?,
                    Type::Int32 => extract(data, ps, item.count, RType::Int32, RType::Int32Array)?,
                    Type::Int64 => extract(data, ps, item.count, RType::Int64, RType::Int64Array)?,

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

                Ok((item.tag, tag_value))
            })
            .collect::<io::Result<HashMap<_, _>>>()?;
        Ok(Tags(tags))
    }

    pub fn write<W: Write>(&self, fh: &mut W) -> io::Result<()> {
        for (_, value) in &self.0 {
            match value {
                RType::Null => {}
                RType::Char(c) => {
                    fh.write_be(*c as u32)?;
                }
                RType::Int8(i) => {
                    fh.write_be(*i)?;
                }
                RType::Int16(i) => {
                    fh.write_be(*i)?;
                }
                RType::Int32(i) => {
                    fh.write_be(*i)?;
                }
                RType::Int64(i) => {
                    fh.write_be(*i)?;
                }
                RType::String(s) => {
                    fh.write_all(s.as_bytes())?;
                }
                RType::Bin(b) => {
                    fh.write_all(b)?;
                }
                RType::StringArray(vector) => {
                    for s in vector {
                        fh.write_all(s.as_bytes())?;
                        fh.write_be(0_u8)?;
                    }
                }
                RType::I18nstring(s) => {
                    fh.write_all(s.as_bytes())?;
                    fh.write_be(0_u8)?;
                }
                RType::Int8Array(vector) => {
                    for value in vector {
                        fh.write_be(*value)?;
                    }
                }
                RType::Int16Array(vector) => {
                    for value in vector {
                        fh.write_be(*value)?;
                    }
                }
                RType::Int32Array(vector) => {
                    for value in vector {
                        fh.write_be(*value)?;
                    }
                }
                RType::Int64Array(vector) => {
                    for value in vector {
                        fh.write_be(*value)?;
                    }
                }
            }
        }

        Ok(())
    }
}

fn extract<T: ReadBytes>(
    data: &[u8],
    position: usize,
    count: usize,
    single: fn(T) -> RType,
    multiple: fn(Vec<T>) -> RType,
) -> io::Result<RType> {
    if count > 1 {
        let values = (0..count)
            .map(|i| {
                let pos = position + i * size_of::<T>();
                (&data[pos..]).read_be()
            })
            .collect::<io::Result<Vec<T>>>()?;
        Ok(multiple(values))
    } else {
        Ok(single((&data[position..]).read_be()?))
    }
}
