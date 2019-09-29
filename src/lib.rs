pub mod header;

use chrono::{Local, TimeZone};
use num_traits::FromPrimitive;
use std::char;
use std::collections::HashMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek};
use std::path::Path;

use header::{Index, RTag, RType, SigTag, Tag, Type};

const MAGIC: [u8; 4] = [237, 171, 238, 219];
const MAGIC_HEADER: [u8; 4] = [142, 173, 232, 1];

pub struct RawLead {
    pub magic: [u8; 4],
    pub major: u8,
    pub minor: u8,
    pub rpm_type: i16,
    pub archnum: i16,
    pub name: [u8; 66],
    pub osnum: i16,
    pub signature_type: i16,
    pub reserved: [u8; 16],
}

impl RawLead {
    pub fn read<R: Read + Seek>(fh: &mut R) -> Result<Self, io::Error> {
        fh.seek(io::SeekFrom::Start(0))?;
        let mut magic = [0_u8; 4];
        fh.read_exact(&mut magic)?;

        if magic != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Error: File is not rpm",
            ));
        }

        let mut head = [0_u8; 2];
        fh.read_exact(&mut head)?;
        let [major, minor] = head;

        let mut rpm_type_be = [0_u8; 2];
        fh.read_exact(&mut rpm_type_be)?;
        let rpm_type = i16::from_be_bytes(rpm_type_be);
        let mut archnum_be = [0_u8; 2];
        fh.read_exact(&mut archnum_be)?;
        let archnum = i16::from_be_bytes(archnum_be);

        let mut name = [0_u8; 66];
        fh.read_exact(&mut name)?;
        let mut osnum_be = [0_u8; 2];
        fh.read_exact(&mut osnum_be)?;
        let osnum = i16::from_be_bytes(osnum_be);
        let signature_type_be = [0_u8; 2];
        fh.read_exact(&mut osnum_be)?;
        let signature_type = i16::from_be_bytes(signature_type_be);
        let mut reserved = [0_u8; 16];
        fh.read_exact(&mut reserved)?;

        Ok(Self {
            magic,
            major,
            minor,
            rpm_type,
            archnum,
            name,
            osnum,
            signature_type,
            reserved,
        })
    }
}

impl fmt::Display for RawLead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "magic: {:?}", self.magic)?;
        writeln!(f, "major: {}", self.major)?;
        writeln!(f, "minor: {}", self.minor)?;
        writeln!(f, "rpm_type: {}", self.rpm_type)?;
        writeln!(f, "archnum: {}", self.archnum)?;
        writeln!(f, "name: {}", parse_string(&self.name))?;
        writeln!(f, "osnum: {}", self.osnum)?;
        writeln!(f, "signature_type: {}", self.signature_type)?;
        writeln!(f, "reserved: {}", parse_string(&self.reserved))
    }
}

impl fmt::Debug for RawLead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "magic: {:?}", self.magic)?;
        writeln!(f, "major: {}", self.major)?;
        writeln!(f, "minor: {}", self.minor)?;
        writeln!(f, "rpm_type: {}", self.rpm_type)?;
        writeln!(f, "archnum: {}", self.archnum)?;
        writeln!(f, "name: {:?}", &&self.name[..])?;
        writeln!(f, "osnum: {}", self.osnum)?;
        writeln!(f, "signature_type: {}", self.signature_type)?;
        writeln!(f, "reserved: {:?}", self.reserved)
    }
}

#[derive(Debug)]
pub struct RawHeader {
    pub magic: [u8; 4],
    pub reserved: [u8; 4],
    pub nindex: i32,
    pub hsize: i32,
}

impl RawHeader {
    pub fn read<R: Read + Seek>(fh: &mut R) -> Result<Self, io::Error> {
        let mut magic = [0_u8; 4];
        fh.read_exact(&mut magic)?;

        if magic != MAGIC_HEADER {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Error: File is not rpm",
            ));
        }

        let mut reserved = [0_u8; 4];
        fh.read_exact(&mut reserved)?;

        let mut nindex_be = [0_u8; 4];
        fh.read_exact(&mut nindex_be)?;
        let nindex = i32::from_be_bytes(nindex_be);

        let mut hsize_be = [0_u8; 4];
        fh.read_exact(&mut hsize_be)?;
        let hsize = i32::from_be_bytes(hsize_be);

        Ok(RawHeader {
            magic,
            reserved,
            nindex,
            hsize,
        })
    }
}

#[derive(Debug)]
pub struct RPMFile {
    pub lead: RawLead,
    pub signature: RawHeader,
    pub indexes: Vec<Index<SigTag>>,
    pub sigtags: Vec<RTag<SigTag>>,
    pub header: RawHeader,
    pub h_indexes: Vec<Index<Tag>>,
    pub tags: Vec<RTag<Tag>>,
    pub file: File,
}

impl RPMFile {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, io::Error> {
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        let lead = RawLead::read(&mut file)?;

        let signature = RawHeader::read(&mut file)?;

        let mut indexes = Vec::with_capacity(signature.nindex as usize);
        for _ in 0..signature.nindex {
            let index = Index::read(&mut file)?;
            indexes.push(index);
        }

        indexes.sort_by_key(|k| k.offset);

        let mut s_data = vec![0_u8; signature.hsize as usize];
        file.read_exact(&mut s_data)?;

        let sigtags = tags_from_raw(&indexes, &s_data);

        // aligning to 8 bytes
        let pos = signature.hsize - 8 * (signature.hsize / 8);
        file.seek(io::SeekFrom::Current(pos.into()))?;

        let header = RawHeader::read(&mut file)?;

        let mut h_indexes = Vec::with_capacity(signature.nindex as usize);
        for _ in 0..header.nindex {
            let index = Index::read(&mut file)?;
            h_indexes.push(index);
        }

        h_indexes.sort_by_key(|k| k.offset);

        let mut data = vec![0_u8; header.hsize as usize];
        file.read_exact(&mut data)?;

        let tags = tags_from_raw(&h_indexes, &data);

        Ok(Self {
            lead,
            signature,
            indexes,
            sigtags,
            header,
            h_indexes,
            tags,
            file,
        })
    }
}

#[derive(Debug)]
pub struct RPMInfo {
    pub name: String,
    pub version: String,
    pub release: String,
    pub arch: String,
    pub group: String,
    pub size: i32,
    pub license: String,
    pub source_rpm: String,
    pub build_time: String,
    pub build_host: String,
    pub summary: String,
    pub description: String,
}

impl fmt::Display for RPMInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Name        : {}", self.name)?;
        writeln!(f, "Version     : {}", self.version)?;
        writeln!(f, "Release     : {}", self.release)?;
        writeln!(f, "Architecture: {}", self.arch)?;
        writeln!(f, "Group       : {}", self.group)?;
        writeln!(f, "Size        : {}", self.size)?;
        writeln!(f, "License     : {}", self.license)?;
        writeln!(f, "Signature   : (none)")?;
        writeln!(f, "Source RPM  : {}", self.source_rpm)?;
        writeln!(f, "Build Date  : {}", self.build_time)?;
        writeln!(f, "Build Host  : {}", self.build_host)?;
        writeln!(f, "Relocations : /usr")?;
        writeln!(f, "Summary     : {}", self.summary)?;
        writeln!(f, "Description : \n{}", self.description)
    }
}

impl From<RPMFile> for RPMInfo {
    fn from(item: RPMFile) -> Self {
        let mut tags = HashMap::new();

        for tag in &item.tags {
            tags.insert(tag.name, tag.value.clone());
        }

        let name = match tags.get(&Tag::Name) {
            Some(RType::String(v)) => v.to_string(),
            _ => "".to_owned(),
        };

        let version = match tags.get(&Tag::Version) {
            Some(RType::String(v)) => v.to_string(),
            _ => "".to_owned(),
        };

        let release = match tags.get(&Tag::Release) {
            Some(RType::String(v)) => v.to_string(),
            _ => "".to_owned(),
        };

        let arch = match tags.get(&Tag::Arch) {
            Some(RType::String(v)) => v.to_string(),
            _ => "".to_owned(),
        };

        let group = match tags.get(&Tag::Group) {
            Some(RType::I18nstring(v)) => v.to_string(),
            _ => "".to_owned(),
        };

        let size = match tags.get(&Tag::Size) {
            Some(RType::Int32(v)) => *v,
            _ => 0,
        };

        let license = match tags.get(&Tag::License) {
            Some(RType::String(v)) => v.to_string(),
            _ => "".to_owned(),
        };

        let source_rpm = match tags.get(&Tag::SourceRpm) {
            Some(RType::String(v)) => v.to_string(),
            _ => "".to_owned(),
        };

        let build_time = match tags.get(&Tag::BuildTime) {
            Some(RType::Int32(v)) => Local.timestamp(i64::from(*v), 0).format("%c").to_string(),
            _ => "".to_owned(),
        };

        let build_host = match tags.get(&Tag::BuildHost) {
            Some(RType::String(v)) => v.to_string(),
            _ => "".to_owned(),
        };

        let summary = match tags.get(&Tag::Summary) {
            Some(RType::I18nstring(v)) => v.to_string(),
            _ => "".to_owned(),
        };

        let description = match tags.get(&Tag::Description) {
            Some(RType::I18nstring(v)) => v.to_string(),
            _ => "".to_owned(),
        };

        RPMInfo {
            name,
            version,
            release,
            arch,
            group,
            size,
            license,
            source_rpm,
            build_time,
            build_host,
            summary,
            description,
        }
    }
}

fn debug_some<R: Read + Seek>(file: &mut R) -> Result<(), io::Error> {
    let mut debug = [0_u8; 32];
    file.read_exact(&mut debug)?;
    println!("Bytes: {:?}", debug);
    Ok(())
}

fn parse_string(bytes: &[u8]) -> String {
    let position = bytes.iter().position(|&x| x == 0).unwrap_or(0);
    let bytes2 = &bytes[0..position];
    String::from_utf8_lossy(bytes2).to_string()
}

fn parse_strings(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|x| *x == 0)
        .filter(|x| !x.is_empty())
        .map(|b| String::from_utf8_lossy(b).to_string())
        .collect()
}

fn tags_from_raw<T>(indexes: &[Index<T>], data: &[u8]) -> Vec<RTag<T>>
where
    T: FromPrimitive + Default + Copy,
{
    (0..indexes.len())
        .map(|i| {
            let item = &indexes[i];
            let tag_value = match item.itype {
                Type::Null => RType::Null,

                Type::Char => {
                    let ps = item.offset as usize;
                    let mut bytes: [u8; 4] = Default::default();
                    bytes.copy_from_slice(&data[ps..ps + 4]);
                    let d = u32::from_be_bytes(bytes);
                    let v = char::from_u32(d).unwrap_or_default();
                    RType::Char(v)
                }

                Type::Int8 => {
                    let v = i8::from_be_bytes([data[item.offset as usize]; 1]);
                    RType::Int8(v)
                }

                Type::Int16 => {
                    let ps = item.offset as usize;
                    let s: [u8; 2] = [data[ps], data[ps + 1]];
                    let v = i16::from_be_bytes(s);
                    RType::Int16(v)
                }

                Type::Int32 => {
                    let ps = item.offset as usize;
                    let mut bytes: [u8; 4] = Default::default();
                    bytes.copy_from_slice(&data[ps..ps + 4]);
                    let v = i32::from_be_bytes(bytes);
                    RType::Int32(v)
                }

                Type::Int64 => {
                    let ps = item.offset as usize;
                    let mut bytes: [u8; 8] = Default::default();
                    bytes.copy_from_slice(&data[ps..ps + 8]);
                    let v = i64::from_be_bytes(bytes);
                    RType::Int64(v)
                }

                Type::String => {
                    let ps = item.offset as usize;
                    let ps2 = indexes[i + 1].offset as usize;
                    let bytes = &data[ps..ps2];
                    let v = parse_string(bytes);
                    RType::String(v)
                }

                Type::Bin => {
                    let ps = item.offset as usize;
                    let ps2 = ps + item.count as usize;
                    let bytes = &data[ps..ps2];
                    RType::Bin(bytes.to_vec())
                }

                Type::StringArray => {
                    let ps = item.offset as usize;
                    let ps2 = indexes[i + 1].offset as usize;
                    let bytes = &data[ps..ps2];
                    let v = parse_strings(bytes);
                    RType::StringArray(v)
                }

                Type::I18nstring => {
                    let ps = item.offset as usize;
                    let ps2 = indexes[i + 1].offset as usize;
                    let bytes = &data[ps..ps2];
                    let v = parse_string(bytes);
                    RType::I18nstring(v)
                }
            };

            RTag {
                name: item.tag,
                value: tag_value,
            }
        })
        .collect()
}
