mod index;
mod lead;
mod tags;

pub use index::*;
pub use lead::*;
pub use tags::*;

use num_traits::{FromPrimitive, ToPrimitive};
use omnom::ReadBytes;
use omnom::prelude::*;
use std::char;
use std::collections::HashMap;
use std::convert::TryInto;
use std::hash::Hash;
use std::io::{self, Read, Seek, Write};
use std::mem::size_of;

use crate::utils::{align_n_bytes, parse_string, parse_strings};

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

    pub fn insert(&mut self, key: T, value: RType) -> &mut Self {
        self.0.insert(key, value);
        self
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

        Self::tags_from_raw(indexes, &s_data)
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
}

impl Tags<Tag> {
    pub fn insert_name(&mut self, name: String) -> &mut Self {
        self.insert(Tag::Name, RType::String(name))
    }

    pub fn insert_epoch(&mut self, epoch: u8) -> &mut Self {
        self.insert(Tag::Epoch, RType::Int8(epoch))
    }

    pub fn insert_version(&mut self, version: String) -> &mut Self {
        self.insert(Tag::Version, RType::String(version))
    }

    pub fn insert_group(&mut self, group: String) -> &mut Self {
        self.insert(Tag::Group, RType::String(group))
    }

    pub fn insert_arch(&mut self, arch: String) -> &mut Self {
        self.insert(Tag::Arch, RType::String(arch))
    }

    pub fn insert_size(&mut self, size: u64) -> &mut Self {
        self.insert(Tag::Size, RType::Int64(size))
    }

    pub fn insert_license(&mut self, license: String) -> &mut Self {
        self.insert(Tag::License, RType::String(license))
    }

    pub fn insert_summary(&mut self, summary: String) -> &mut Self {
        self.insert(Tag::Summary, RType::String(summary))
    }

    pub fn insert_description(&mut self, description: String) -> &mut Self {
        self.insert(Tag::Description, RType::String(description))
    }

    pub fn insert_build_host(&mut self, host: String) -> &mut Self {
        self.insert(Tag::BuildHost, RType::String(host))
    }

    pub fn insert_payload_format(&mut self, compression: String) -> &mut Self {
        self.insert(Tag::PayloadFormat, RType::String(compression))
    }

    pub fn insert_payload_compressor(&mut self, compressor: String) -> &mut Self {
        self.insert(Tag::PayloadCompressor, RType::String(compressor))
    }

    pub fn insert_payload_flags(&mut self, flags: String) -> &mut Self {
        self.insert(Tag::PayloadFlags, RType::String(flags))
    }

    pub fn insert_source_rpm(&mut self, source: String) -> &mut Self {
        self.insert(Tag::SourceRpm, RType::String(source))
    }

    pub fn insert_build_time(&mut self, time: i64) -> &mut Self {
        let time_u64 = time.try_into().expect("Timestamp is out of u64");
        self.insert(Tag::BuildTime, RType::Int64(time_u64))
    }

    pub fn insert_pre_install(&mut self, script: String) -> &mut Self {
        self.insert(Tag::PreIn, RType::String(script))
    }

    pub fn insert_post_install(&mut self, script: String) -> &mut Self {
        self.insert(Tag::PostIn, RType::String(script))
    }

    pub fn insert_pre_uninstall(&mut self, script: String) -> &mut Self {
        self.insert(Tag::PreUn, RType::String(script))
    }

    pub fn insert_post_uninstall(&mut self, script: String) -> &mut Self {
        self.insert(Tag::PostUn, RType::String(script))
    }
}

impl Tags<SignatureTag> {
    pub fn insert_payload_size(&mut self, size: u64) -> &mut Self {
        self.insert(SignatureTag::PayloadSize, RType::Int64(size))
    }
}

pub trait TagsWrite {
    fn write_header<T: ToPrimitive + Eq + Hash + Copy>(&mut self, tags: &Tags<T>)
    -> io::Result<()>;
}

impl<W> TagsWrite for W
where
    W: Write,
{
    fn write_header<T: ToPrimitive + Eq + Hash + Copy>(
        &mut self,
        tags: &Tags<T>,
    ) -> io::Result<()> {
        let mut address: Vec<u8> = Vec::new();
        let mut data: Vec<u8> = Vec::new();
        let index = tags.0.len();

        for (tag, value) in &tags.0 {
            let current = data.len();
            match value {
                RType::Null => {
                    let index = Index::from(tag, value, 0, 1);
                    address.write_index(index)?;
                }

                RType::Char(c) => {
                    data.write_be(*c as u32)?;
                    let index = Index::from(tag, value, current, 1);
                    address.write_index(index)?;
                }

                RType::Int8(i) => {
                    data.write_be(*i)?;
                    let index = Index::from(tag, value, current, 1);
                    address.write_index(index)?;
                }

                RType::Int16(i) => {
                    data.write_be(*i)?;
                    let index = Index::from(tag, value, current, 1);
                    address.write_index(index)?;
                }

                RType::Int32(i) => {
                    data.write_be(*i)?;
                    let index = Index::from(tag, value, current, 1);
                    address.write_index(index)?;
                }

                RType::Int64(i) => {
                    data.write_be(*i)?;
                    let index = Index::from(tag, value, current, 1);
                    address.write_index(index)?;
                }

                RType::String(s) => {
                    data.write_all(s.as_bytes())?;
                    data.write_be(0_u8)?;
                    let index = Index::from(tag, value, current, 1);
                    address.write_index(index)?;
                }

                RType::Bin(b) => {
                    data.write_all(b)?;
                    let index = Index::from(tag, value, current, 1);
                    address.write_index(index)?;
                }

                RType::StringArray(vector) => {
                    let index = Index::from(tag, value, current, vector.len());
                    address.write_index(index)?;
                    for s in vector {
                        data.write_all(s.as_bytes())?;
                        data.write_be(0_u8)?;
                    }
                }

                RType::I18nstring(s) => {
                    data.write_all(s.as_bytes())?;
                    data.write_be(0_u8)?;
                    let index = Index::from(tag, value, current, 1);
                    address.write_index(index)?;
                }

                RType::Int8Array(vector) => {
                    let index = Index::from(tag, value, current, vector.len());
                    address.write_index(index)?;
                    for value in vector {
                        data.write_be(*value)?;
                    }
                }

                RType::Int16Array(vector) => {
                    let index = Index::from(tag, value, current, vector.len());
                    address.write_index(index)?;
                    for value in vector {
                        data.write_be(*value)?;
                    }
                }

                RType::Int32Array(vector) => {
                    let index = Index::from(tag, value, current, vector.len());
                    address.write_index(index)?;
                    for value in vector {
                        data.write_be(*value)?;
                    }
                }

                RType::Int64Array(vector) => {
                    let index = Index::from(tag, value, current, vector.len());
                    address.write_index(index)?;
                    for value in vector {
                        data.write_be(*value)?;
                    }
                }
            }
        }

        let size = data.len() as u32;
        let lead = HeaderLead::from(index, size);

        lead.write(self)?;
        self.write_all(&address)?;
        self.write_all(&data)?;

        // aligning to 8 bytes
        let number = align_n_bytes(size, 8) as usize;
        let pad = vec![0_u8; number];
        self.write_all(&pad)?;

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
