mod index;
mod lead;
mod raw;
mod tags;

pub use index::*;
pub use lead::*;
pub use raw::*;
pub use tags::*;

use num_traits::{FromPrimitive, ToPrimitive};
use omnom::ReadBytes;
use omnom::prelude::*;
use std::collections::HashMap;
use std::hash::Hash;
use std::io::{self, Read, Seek, Write};
use std::mem::size_of;

use crate::utils::align_n_bytes;

/// Maximum allowed header size (32 MB) - prevents OOM attacks from malicious RPM files
const MAX_HEADER_SIZE: usize = 32 * 1024 * 1024;

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
        if size > MAX_HEADER_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Header size {} exceeds maximum allowed size {}",
                    size, MAX_HEADER_SIZE
                ),
            ));
        }

        let mut s_data = vec![0_u8; size];
        fh.read_exact(&mut s_data)?;

        Self::tags_from_raw(indexes, &s_data)
    }

    fn tags_from_raw(indexes: &[Index<T>], data: &[u8]) -> io::Result<Self> {
        let tags = indexes
            .iter()
            .map(|item| {
                let tail = data.get(item.offset..).ok_or_else(invalid_tag_data)?;
                let tag_value = match item.itype {
                    Type::Null => RType::Null,
                    Type::Char => {
                        if item.count == 1 {
                            RType::Char(char::from(*tail.first().ok_or_else(invalid_tag_data)?))
                        } else {
                            extract(data, item.offset, item.count, RType::Int8, RType::Int8Array)?
                        }
                    }
                    Type::Int8 => {
                        extract(data, item.offset, item.count, RType::Int8, RType::Int8Array)?
                    }
                    Type::Int16 => extract(
                        data,
                        item.offset,
                        item.count,
                        RType::Int16,
                        RType::Int16Array,
                    )?,
                    Type::Int32 => extract(
                        data,
                        item.offset,
                        item.count,
                        RType::Int32,
                        RType::Int32Array,
                    )?,
                    Type::Int64 => extract(
                        data,
                        item.offset,
                        item.count,
                        RType::Int64,
                        RType::Int64Array,
                    )?,
                    Type::String => {
                        if item.count != 1 {
                            return Err(invalid_tag_data());
                        }
                        RType::String(read_strings(tail, 1)?.remove(0))
                    }
                    Type::Bin => RType::Bin(
                        tail.get(..item.count)
                            .ok_or_else(invalid_tag_data)?
                            .to_vec(),
                    ),
                    Type::StringArray => RType::StringArray(read_strings(tail, item.count)?),
                    Type::I18nstring => {
                        // The public model exposes the first translation only.
                        RType::I18nstring(
                            read_strings(tail, item.count)?
                                .into_iter()
                                .next()
                                .unwrap_or_default(),
                        )
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

    pub fn insert_epoch(&mut self, epoch: u32) -> &mut Self {
        self.insert(Tag::Epoch, RType::Int32(epoch))
    }

    pub fn insert_version(&mut self, version: String) -> &mut Self {
        self.insert(Tag::Version, RType::String(version))
    }

    pub fn insert_group(&mut self, group: String) -> &mut Self {
        self.insert(Tag::Group, RType::I18nstring(group))
    }

    pub fn insert_arch(&mut self, arch: String) -> &mut Self {
        self.insert(Tag::Arch, RType::String(arch))
    }

    pub fn insert_size(&mut self, size: u64) -> &mut Self {
        self.0.remove(&Tag::Size);
        self.0.remove(&Tag::LongSize);
        match u32::try_from(size) {
            Ok(size) => self.insert(Tag::Size, RType::Int32(size)),
            Err(_) => self.insert(Tag::LongSize, RType::Int64(size)),
        }
    }

    pub fn insert_license(&mut self, license: String) -> &mut Self {
        self.insert(Tag::License, RType::String(license))
    }

    pub fn insert_summary(&mut self, summary: String) -> &mut Self {
        self.insert(Tag::Summary, RType::I18nstring(summary))
    }

    pub fn insert_description(&mut self, description: String) -> &mut Self {
        self.insert(Tag::Description, RType::I18nstring(description))
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
        let time = u32::try_from(time).expect("RPM build time must fit u32");
        self.insert(Tag::BuildTime, RType::Int32(time))
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
        self.0.remove(&SignatureTag::PayloadSize);
        self.0.remove(&SignatureTag::LongArchiveSize);
        match u32::try_from(size) {
            Ok(size) => self.insert(SignatureTag::PayloadSize, RType::Int32(size)),
            Err(_) => self.insert(SignatureTag::LongArchiveSize, RType::Int64(size)),
        }
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

        let mut sorted_tags: Vec<_> = tags.0.iter().collect();
        sorted_tags.sort_by_key(|(tag, _)| tag.to_u32());
        for (tag, value) in sorted_tags {
            let alignment = match value {
                RType::Int16(_) | RType::Int16Array(_) => 2,
                RType::Int32(_) | RType::Int32Array(_) => 4,
                RType::Int64(_) | RType::Int64Array(_) => 8,
                _ => 1,
            };
            data.resize(
                data.len() + align_n_bytes(data.len() as u32, alignment) as usize,
                0,
            );
            let current = data.len();
            match value {
                RType::Null => {
                    let index = Index::from(tag, value, 0, 1);
                    address.write_index(index)?;
                }

                RType::Char(c) => {
                    let byte = u8::try_from(*c as u32).map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidInput, "RPM char must fit in one byte")
                    })?;
                    data.write_be(byte)?;
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
                    if s.contains('\0') {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "NUL in RPM string",
                        ));
                    }
                    data.write_all(s.as_bytes())?;
                    data.write_be(0_u8)?;
                    let index = Index::from(tag, value, current, 1);
                    address.write_index(index)?;
                }

                RType::Bin(b) => {
                    data.write_all(b)?;
                    let index = Index::from(tag, value, current, b.len());
                    address.write_index(index)?;
                }

                RType::StringArray(vector) => {
                    let index = Index::from(tag, value, current, vector.len());
                    address.write_index(index)?;
                    for s in vector {
                        if s.contains('\0') {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "NUL in RPM string",
                            ));
                        }
                        data.write_all(s.as_bytes())?;
                        data.write_be(0_u8)?;
                    }
                }

                RType::I18nstring(s) => {
                    if s.contains('\0') {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "NUL in RPM string",
                        ));
                    }
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

        Ok(())
    }
}

/// Encode a v4 immutable header region (62 for signatures, 63 for metadata).
/// Region trailers contain a negative index byte size in their offset field.
pub fn encode_region<T: ToPrimitive + Eq + Hash + Copy>(
    tags: &Tags<T>,
    region: T,
) -> io::Result<Vec<u8>> {
    let region_id = region.to_u32().ok_or_else(invalid_tag_data)?;
    if !matches!(region_id, 62 | 63)
        || tags
            .0
            .keys()
            .any(|tag| tag.to_u32().is_none_or(|id| id <= region_id))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid or duplicate RPM region tag",
        ));
    }
    let mut raw = Vec::new();
    raw.write_header(tags)?;
    let count = tags.0.len() + 1;
    let data_offset = 16 + tags.0.len() * 16;
    let size = raw.len() - data_offset;
    let size_u32 = u32::try_from(size + 16).map_err(|_| invalid_tag_data())?;
    if size + 16 > MAX_HEADER_SIZE || count > MAX_HEADER_SIZE / 16 {
        return Err(invalid_tag_data());
    }
    let mut result = Vec::new();
    HeaderLead::from(count, size_u32).write(&mut result)?;
    for value in [region_id, 7, size as u32, 16] {
        result.write_be(value)?;
    }
    result.extend_from_slice(&raw[16..]);
    for value in [region_id, 7, (-(count as i32 * 16)) as u32, 16] {
        result.write_be(value)?;
    }
    Ok(result)
}

fn invalid_tag_data() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Invalid RPM tag offset, count or data",
    )
}

fn read_strings(mut bytes: &[u8], count: usize) -> io::Result<Vec<String>> {
    if count > bytes.len() {
        return Err(invalid_tag_data());
    }
    let mut strings = Vec::new();
    for _ in 0..count {
        let end = bytes
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(invalid_tag_data)?;
        strings.push(String::from_utf8_lossy(&bytes[..end]).into_owned());
        bytes = &bytes[end + 1..];
    }
    Ok(strings)
}

fn extract<T: ReadBytes>(
    data: &[u8],
    position: usize,
    count: usize,
    single: fn(T) -> RType,
    multiple: fn(Vec<T>) -> RType,
) -> io::Result<RType> {
    let length = count
        .checked_mul(size_of::<T>())
        .ok_or_else(invalid_tag_data)?;
    let mut bytes = data
        .get(position..)
        .and_then(|tail| tail.get(..length))
        .ok_or_else(invalid_tag_data)?;
    if count == 1 {
        Ok(single(omnom::ReadExt::read_be(&mut bytes)?))
    } else {
        let values = (0..count)
            .map(|_| omnom::ReadExt::read_be(&mut bytes))
            .collect::<io::Result<Vec<T>>>()?;
        Ok(multiple(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::tags::Tag;

    #[test]
    fn test_header_size_limit_enforced() {
        // Create a mock reader (doesn't matter what's in it for this test)
        let mut data = vec![0u8; 100];
        let mut reader = std::io::Cursor::new(&mut data);

        // Test that size exceeding MAX_HEADER_SIZE is rejected
        let oversized = MAX_HEADER_SIZE + 1;
        let indexes: Vec<Index<Tag>> = Vec::new();

        let result = Tags::read(&mut reader, &indexes, oversized);

        assert!(result.is_err(), "Should reject oversized header");
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("exceeds maximum allowed size"));
    }

    #[test]
    fn test_header_size_at_limit_accepted() {
        // Test that size exactly at MAX_HEADER_SIZE is accepted
        let mut data = vec![0u8; MAX_HEADER_SIZE];
        let mut reader = std::io::Cursor::new(&mut data);
        let indexes: Vec<Index<Tag>> = Vec::new();

        let result = Tags::read(&mut reader, &indexes, MAX_HEADER_SIZE);

        // Should not fail with size limit error
        // (it may fail for other reasons like invalid data, but not size limit)
        if let Err(e) = result {
            assert_ne!(
                e.kind(),
                io::ErrorKind::InvalidData,
                "Should not reject size at limit: {}",
                e
            );
        }
    }

    #[test]
    fn test_header_size_below_limit_accepted() {
        // Test that normal-sized header is accepted
        let size = 1024; // 1 KB - well below limit
        let mut data = vec![0u8; size];
        let mut reader = std::io::Cursor::new(&mut data);
        let indexes: Vec<Index<Tag>> = Vec::new();

        let result = Tags::read(&mut reader, &indexes, size);

        // Should not fail with size limit error
        if let Err(e) = result {
            assert_ne!(
                e.kind(),
                io::ErrorKind::InvalidData,
                "Should not reject normal size: {}",
                e
            );
        }
    }
}
