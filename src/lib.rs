pub mod header;

use chrono::{Local, TimeZone};
use num_traits::FromPrimitive;
use std::char;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::hash::Hash;
use std::io::{self, Read, Seek};
use std::path::Path;

use header::{get_tag, Index, RType, SigTag, Tag, Tags, Type};

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
    pub nindex: usize,
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
            nindex: nindex as usize,
            hsize,
        })
    }
}

#[derive(Debug)]
pub struct RPMFile {
    pub lead: RawLead,
    pub signature: RawHeader,
    pub indexes: Vec<Index<SigTag>>,
    pub sigtags: Tags<SigTag>,
    pub header: RawHeader,
    pub h_indexes: Vec<Index<Tag>>,
    pub tags: Tags<Tag>,
    pub file: File,
}

impl RPMFile {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, io::Error> {
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        let lead = RawLead::read(&mut file)?;

        let signature = RawHeader::read(&mut file)?;

        let mut indexes = Vec::with_capacity(signature.nindex);
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

        let mut h_indexes = Vec::with_capacity(signature.nindex);
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
pub struct RPMPayload {
    pub size: i32,
    pub format: String,
    pub compressor: String,
    pub flags: String,
    pub files: Vec<String>,
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
    pub payload: RPMPayload,
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
        writeln!(f, "Signature   : (unimplemented)")?;
        writeln!(f, "Source RPM  : {}", self.source_rpm)?;
        writeln!(f, "Build Date  : {}", self.build_time)?;
        writeln!(f, "Build Host  : {}", self.build_host)?;
        writeln!(f, "Relocations : (unimplemented)")?;
        writeln!(f, "Summary     : {}", self.summary)?;
        writeln!(f, "Description : \n{}", self.description)
    }
}

impl From<RPMFile> for RPMInfo {
    fn from(rpm: RPMFile) -> Self {
        let dirs: Vec<String> = get_tag(&rpm.tags, Tag::DirNames);
        let dir_indexes: Vec<i32> = get_tag(&rpm.tags, Tag::Dirindexes);
        let basenames: Vec<String> = get_tag(&rpm.tags, Tag::Basenames);
        let files: Vec<String> = basenames
            .iter()
            .zip(dir_indexes.iter())
            .map(|(x, y)| dirs[*y as usize].clone() + x)
            .collect();

        let payload = RPMPayload {
            size: get_tag(&rpm.sigtags, SigTag::PayloadSize),
            format: get_tag(&rpm.tags, Tag::Payloadformat),
            compressor: get_tag(&rpm.tags, Tag::Payloadcompressor),
            flags: get_tag(&rpm.tags, Tag::Payloadflags),
            files,
        };

        let build_int: i32 = get_tag(&rpm.tags, Tag::BuildTime);
        let build_time = Local
            .timestamp(i64::from(build_int), 0)
            .format("%c")
            .to_string();

        RPMInfo {
            name: get_tag(&rpm.tags, Tag::Name),
            version: get_tag(&rpm.tags, Tag::Version),
            release: get_tag(&rpm.tags, Tag::Release),
            arch: get_tag(&rpm.tags, Tag::Arch),
            group: get_tag(&rpm.tags, Tag::Group),
            size: get_tag(&rpm.tags, Tag::Size),
            license: get_tag(&rpm.tags, Tag::License),
            source_rpm: get_tag(&rpm.tags, Tag::SourceRpm),
            build_time,
            build_host: get_tag(&rpm.tags, Tag::BuildHost),
            summary: get_tag(&rpm.tags, Tag::Summary),
            description: get_tag(&rpm.tags, Tag::Description),
            payload,
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

fn tags_from_raw<T>(indexes: &[Index<T>], data: &[u8]) -> Tags<T>
where
    T: FromPrimitive + Copy + Eq + Hash,
{
    (0..indexes.len())
        .map(|i| {
            let item = &indexes[i];
            let ps = item.offset;

            let tag_value = match item.itype {
                Type::Null => RType::Null,

                Type::Char => RType::Char(char::from_bytes(&data, ps)),

                Type::Int8 => {
                    if item.count > 1 {
                        let values: Vec<i8> = (0..item.count)
                            .map(|i| -> i8 { i8::from_be_bytes([data[ps + i]; 1]) })
                            .collect();
                        RType::Int8Array(values)
                    } else {
                        RType::Int8(i8::from_be_bytes([data[ps]; 1]))
                    }
                }

                Type::Int16 => {
                    if item.count > 1 {
                        let values: Vec<i16> = (0..item.count)
                            .map(|i| -> i16 { i16::from_bytes(&data, ps + i * 2) })
                            .collect();
                        RType::Int16Array(values)
                    } else {
                        RType::Int16(i16::from_bytes(&data, ps))
                    }
                }

                Type::Int32 => {
                    if item.count > 1 {
                        let values: Vec<i32> = (0..item.count)
                            .map(|i| -> i32 { i32::from_bytes(&data, ps + i * 4) })
                            .collect();
                        RType::Int32Array(values)
                    } else {
                        RType::Int32(i32::from_bytes(&data, ps))
                    }
                }

                Type::Int64 => {
                    if item.count > 1 {
                        let values: Vec<i64> = (0..item.count)
                            .map(|i| -> i64 { i64::from_bytes(&data, ps + i * 8) })
                            .collect();
                        RType::Int64Array(values)
                    } else {
                        RType::Int64(i64::from_bytes(&data, ps))
                    }
                }

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
                    let v = parse_strings(&data[ps..ps2]);
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
        .collect()
}

trait FromBytes {
    fn from_bytes(data: &[u8], position: usize) -> Self;
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

from_bytes!(i16, 2);
from_bytes!(i32, 4);
from_bytes!(i64, 8);
from_bytes!(u32, 4);
