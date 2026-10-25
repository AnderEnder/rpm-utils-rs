//! Lossless tag values for editing headers, including unknown IDs and locales.
use super::{HeaderLead, MAX_HEADER_SIZE, RType, Tags, TagsWrite, Type};
use num_traits::{FromPrimitive, ToPrimitive};
use std::collections::HashSet;
use std::hash::Hash;
use std::io::{self, Read, Write};

#[derive(Debug, Clone, PartialEq)]
pub struct RawTag {
    pub id: u32,
    pub kind: Type,
    pub count: u32,
    /// Original network-order bytes, including string terminators.
    pub data: Vec<u8>,
}

impl RawTag {
    pub fn new(id: u32, kind: Type, count: u32, data: Vec<u8>) -> io::Result<Self> {
        if value_len(&kind, count, &data)? != data.len() {
            return Err(invalid("Unexpected tag data length"));
        }
        Ok(Self {
            id,
            kind,
            count,
            data,
        })
    }
    pub fn from_value(id: u32, value: RType) -> io::Result<Self> {
        let tags = Tags(std::collections::HashMap::from([(id, value)]));
        let mut bytes = Vec::new();
        bytes.write_header(&tags)?;
        Ok(RawHeader::read(&mut bytes.as_slice())?.entries.remove(0))
    }
    pub fn number(&self) -> Option<u64> {
        if self.count != 1 {
            return None;
        }
        self.numbers().ok()?.first().copied()
    }
    pub fn numbers(&self) -> io::Result<Vec<u64>> {
        let width = match self.kind {
            Type::Char | Type::Int8 => 1,
            Type::Int16 => 2,
            Type::Int32 => 4,
            Type::Int64 => 8,
            _ => return Err(invalid("Not a numeric tag")),
        };
        Self::new(self.id, self.kind.clone(), self.count, self.data.clone())?;
        Ok(self
            .data
            .chunks_exact(width)
            .map(|bytes| {
                bytes
                    .iter()
                    .fold(0, |value, byte| (value << 8) | u64::from(*byte))
            })
            .collect())
    }
    pub fn strings(&self) -> io::Result<Vec<String>> {
        if !matches!(
            self.kind,
            Type::String | Type::StringArray | Type::I18nstring
        ) {
            return Err(invalid("Not a string tag"));
        }
        Self::new(self.id, self.kind.clone(), self.count, self.data.clone())?;
        self.data
            .split(|byte| *byte == 0)
            .take(self.count as usize)
            .map(|bytes| String::from_utf8(bytes.to_vec()).map_err(|_| invalid("Tag is not UTF-8")))
            .collect()
    }
    pub fn strings_value(id: u32, kind: Type, strings: &[String]) -> io::Result<Self> {
        let mut data = Vec::new();
        for value in strings {
            if value.contains('\0') {
                return Err(invalid("NUL in string value"));
            }
            data.extend_from_slice(value.as_bytes());
            data.push(0);
        }
        Self::new(
            id,
            kind,
            u32::try_from(strings.len()).map_err(|_| invalid("Too many strings"))?,
            data,
        )
    }
    pub(crate) fn alignment(&self) -> usize {
        match self.kind {
            Type::Int16 => 2,
            Type::Int32 => 4,
            Type::Int64 => 8,
            _ => 1,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RawHeader {
    pub entries: Vec<RawTag>,
    pub reserved: [u8; 4],
    /// Original offsets in index order, for layout diagnostics.
    pub source_offsets: Vec<u32>,
}

impl RawHeader {
    pub fn read(reader: &mut impl Read) -> io::Result<Self> {
        let lead = HeaderLead::read(reader)?;
        if lead.hsize as usize > MAX_HEADER_SIZE || lead.nindex > MAX_HEADER_SIZE / 16 {
            return Err(invalid("RPM header exceeds limits"));
        }
        let mut indexes = vec![0; lead.nindex * 16];
        reader.read_exact(&mut indexes)?;
        let mut data = vec![0; lead.hsize as usize];
        reader.read_exact(&mut data)?;
        let mut entries = Vec::new();
        let mut offsets = Vec::new();
        for index in indexes.as_chunks::<16>().0 {
            let number = |offset| {
                u32::from_be_bytes(
                    index[offset..offset + 4]
                        .try_into()
                        .expect("four index bytes"),
                )
            };
            let id = number(0);
            let kind =
                Type::from_u32(number(4)).ok_or_else(|| invalid("Unsupported RPM tag type"))?;
            let offset = number(8);
            let count = number(12);
            let tail = data
                .get(offset as usize..)
                .ok_or_else(|| invalid("RPM tag offset exceeds header"))?;
            let length = value_len(&kind, count, tail)?;
            entries.push(RawTag {
                id,
                kind,
                count,
                data: tail[..length].to_vec(),
            });
            offsets.push(offset);
        }
        Ok(Self {
            entries,
            reserved: lead.reserved,
            source_offsets: offsets,
        })
    }
    pub fn from_tags<T: ToPrimitive + Copy + Eq + Hash>(tags: &Tags<T>) -> io::Result<Self> {
        let mut bytes = Vec::new();
        bytes.write_header(tags)?;
        Self::read(&mut bytes.as_slice())
    }
    pub fn get(&self, id: u32) -> Option<&RawTag> {
        self.entries.iter().rev().find(|entry| entry.id == id)
    }
    pub fn set(&mut self, tag: RawTag) -> io::Result<()> {
        RawTag::new(tag.id, tag.kind.clone(), tag.count, tag.data.clone())?;
        self.remove(tag.id);
        self.entries.push(tag);
        Ok(())
    }
    pub fn set_value(&mut self, id: u32, value: RType) -> io::Result<()> {
        self.set(RawTag::from_value(id, value)?)
    }
    pub fn remove(&mut self, id: u32) {
        self.entries.retain(|entry| entry.id != id);
    }
    pub fn strings(&self, id: u32) -> io::Result<Vec<String>> {
        self.get(id)
            .ok_or_else(|| invalid("Missing string tag"))?
            .strings()
    }
    pub fn number(&self, id: u32) -> Option<u64> {
        self.get(id)?.number()
    }
    pub fn encode_region(&self, region: u32) -> io::Result<Vec<u8>> {
        if !matches!(region, 62 | 63) {
            return Err(invalid("Invalid immutable region ID"));
        }
        let mut tags: Vec<_> = self
            .entries
            .iter()
            .filter(|entry| !matches!(entry.id, 61..=64))
            .collect();
        if tags.iter().any(|tag| tag.id < region) {
            return Err(invalid("Reserved tag cannot precede immutable region"));
        }
        tags.sort_by_key(|entry| entry.id);
        let mut seen = HashSet::new();
        let mut data = Vec::new();
        let mut indexes = Vec::new();
        for tag in &tags {
            if !seen.insert(tag.id) {
                return Err(invalid("Duplicate RPM tag ID"));
            }
            RawTag::new(tag.id, tag.kind.clone(), tag.count, tag.data.clone())?;
            let padding = (tag.alignment() - data.len() % tag.alignment()) % tag.alignment();
            data.resize(data.len() + padding, 0);
            let offset = u32::try_from(data.len()).map_err(|_| invalid("Header data too large"))?;
            for number in [
                tag.id,
                tag.kind.to_u32().expect("known type"),
                offset,
                tag.count,
            ] {
                indexes.extend_from_slice(&number.to_be_bytes());
            }
            data.extend_from_slice(&tag.data);
            if data.len() > MAX_HEADER_SIZE - 16 {
                return Err(invalid("Header data too large"));
            }
        }
        let count = tags.len() + 1;
        if count > MAX_HEADER_SIZE / 16 {
            return Err(invalid("Too many header indexes"));
        }
        let mut result = Vec::new();
        HeaderLead::from(count, (data.len() + 16) as u32).write(&mut result)?;
        for number in [region, 7, data.len() as u32, 16] {
            result.extend_from_slice(&number.to_be_bytes());
        }
        result.write_all(&indexes)?;
        result.write_all(&data)?;
        for number in [region, 7, (-(count as i32 * 16)) as u32, 16] {
            result.extend_from_slice(&number.to_be_bytes());
        }
        Ok(result)
    }
}

fn value_len(kind: &Type, count: u32, bytes: &[u8]) -> io::Result<usize> {
    let width = match kind {
        Type::Null => 0,
        Type::Char | Type::Int8 | Type::Bin => 1,
        Type::Int16 => 2,
        Type::Int32 => 4,
        Type::Int64 => 8,
        _ => {
            if *kind == Type::String && count != 1 {
                return Err(invalid("STRING count must be one"));
            }
            if count as usize > bytes.len() {
                return Err(invalid("Invalid string count"));
            }
            let mut end = 0;
            for _ in 0..count {
                end += bytes[end..]
                    .iter()
                    .position(|byte| *byte == 0)
                    .ok_or_else(|| invalid("Unterminated RPM string"))?
                    + 1;
            }
            return Ok(end);
        }
    };
    let size = (count as usize)
        .checked_mul(width)
        .ok_or_else(|| invalid("RPM tag size overflow"))?;
    if size > bytes.len() {
        return Err(invalid("RPM tag extends past header data"));
    }
    Ok(size)
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
