pub mod header;
pub mod raw;

use bzip2::read::BzDecoder;
use chrono::{Local, TimeZone};
use flate2::read::GzDecoder;
use itertools::multizip;
use num_traits::FromPrimitive;
use std::char;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::hash::Hash;
use std::io::SeekFrom;
use std::io::{self, Read, Seek};
use std::mem::size_of;
use std::path::Path;
use zstd::stream::read::Decoder;

use header::{Index, RType, SigTag, Tag, Tags, Type};
use raw::*;

#[derive(Debug)]
pub struct RPMFile {
// pub struct RPMFile<T: Read + Seek> {
    pub lead: RawLead,
    pub signature: RawHeader,
    pub indexes: Vec<Index<SigTag>>,
    pub sigtags: Tags<SigTag>,
    pub header: RawHeader,
    pub h_indexes: Vec<Index<Tag>>,
    pub tags: Tags<Tag>,
    pub payload_offset: u64,
//    pub file: T,
    pub file: File,

}

impl RPMFile {
// impl<T: Read + Seek> RPMFile<T> {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<RPMFile, io::Error> {
        let mut file = OpenOptions::new().read(true).open(path)?;
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

        let payload_offset = file.seek(SeekFrom::Current(0))?;

        Ok(RPMFile {
            lead,
            signature,
            indexes,
            sigtags,
            header,
            h_indexes,
            tags,
            file,
            payload_offset,
        })
    }

    pub fn copy_payload(mut self, path: &Path) -> Result<u64, io::Error> {
        let compressor: String = self.tags.get(Tag::Payloadcompressor);
        let mut writer = OpenOptions::new().create(true).write(true).open(path)?;
        self.file.seek(SeekFrom::Start(self.payload_offset))?;

        let mut reader: Box<dyn Read> = match compressor.as_str() {
            "gzip" => Box::new(GzDecoder::new(&mut self.file)),
            "bzip2" => Box::new(BzDecoder::new(&mut self.file)),
            "zstd" => Box::new(Decoder::new(&mut self.file)?),
            format => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Decompressor \"{}\" is not implemented", format),
                ))
            }
        };
        io::copy(&mut reader, &mut writer)
    }
}

/*
impl Default for RPMFile<Empty> {
    fn default() -> RPMFile<Empty> {
        RPMFile {
            lead: Default::default(),
            signature: Default::default(),
            indexes: Default::default(),
            sigtags: Default::default(),
            header: Default::default(),
            h_indexes: Default::default(),
            tags: Default::default(),
            file: Empty,
            payload_offset: Default::default(),
        }
    }
}
*/

#[derive(Debug)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    pub user: String,
    pub group: String,
    pub flags: u32,
    pub mtime: u32,
    pub digest: String,
    pub mode: u16,
    pub linkname: String,
    pub device: u32,
    pub inode: u32,
}

#[derive(Debug)]
pub struct RPMPayload {
    pub size: u64,
    pub format: String,
    pub compressor: String,
    pub flags: String,
    pub files: Vec<FileInfo>,
}

#[derive(Debug)]
pub struct RPMInfo {
    pub name: String,
    pub version: String,
    pub release: String,
    pub arch: String,
    pub group: String,
    pub size: u64,
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

impl From<&RPMFile> for RPMInfo {
// impl<T: Read + Seek> From<&RPMFile<T>> for RPMInfo {
//    fn from(rpm: &RPMFile<T>) -> Self {
    fn from(rpm: &RPMFile) -> Self {
        let dirs: Vec<String> = rpm.tags.get(Tag::DirNames);
        let dir_indexes: Vec<u32> = rpm.tags.get(Tag::Dirindexes);
        let basenames: Vec<String> = rpm.tags.get(Tag::Basenames);
        let filesizes: Vec<u64> = rpm.tags.get(Tag::FileSizes);
        let users: Vec<String> = rpm.tags.get(Tag::FileUserName);
        let groups: Vec<String> = rpm.tags.get(Tag::FileGroupName);
        let flags: Vec<u32> = rpm.tags.get(Tag::FileFlags);
        let mtimes: Vec<u32> = rpm.tags.get(Tag::FileMTimes);
        let linknames: Vec<String> = rpm.tags.get(Tag::FileGroupName);
        let modes: Vec<u16> = rpm.tags.get(Tag::FileModes);
        let devices: Vec<u32> = rpm.tags.get(Tag::FileDevices);
        let inodes: Vec<u32> = rpm.tags.get(Tag::FileInodes);
        let digests: Vec<String> = rpm.tags.get(Tag::FileMD5s);

        let files: Vec<FileInfo> = multizip((
            basenames,
            dir_indexes,
            filesizes,
            users,
            groups,
            linknames,
            digests,
        ))
        .enumerate()
        .map(
            |(i, (name, index, size, user, group, linkname, digest))| FileInfo {
                name: dirs[index as usize].clone() + &name,
                size,
                user,
                group,
                flags: flags[i],
                mtime: mtimes[i],
                digest,
                mode: modes[i],
                linkname,
                device: devices[i],
                inode: inodes[i],
            },
        )
        .collect();

        let payload = RPMPayload {
            size: rpm.sigtags.get(SigTag::PayloadSize),
            format: rpm.tags.get(Tag::Payloadformat),
            compressor: rpm.tags.get(Tag::Payloadcompressor),
            flags: rpm.tags.get(Tag::Payloadflags),
            files,
        };

        let build_int: u32 = rpm.tags.get(Tag::BuildTime);
        let build_time = Local
            .timestamp(i64::from(build_int), 0)
            .format("%c")
            .to_string();

        RPMInfo {
            name: rpm.tags.get(Tag::Name),
            version: rpm.tags.get(Tag::Version),
            release: rpm.tags.get(Tag::Release),
            arch: rpm.tags.get(Tag::Arch),
            group: rpm.tags.get(Tag::Group),
            size: rpm.tags.get(Tag::Size),
            license: rpm.tags.get(Tag::License),
            source_rpm: rpm.tags.get(Tag::SourceRpm),
            build_time,
            build_host: rpm.tags.get(Tag::BuildHost),
            summary: rpm.tags.get(Tag::Summary),
            description: rpm.tags.get(Tag::Description),
            payload,
        }
    }
}
/*
impl From<RPMInfo> for RPMFile<Empty> {
    fn from(info: RPMInfo) -> Self {
        RPMFile::default()
    }
}
*/

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

fn parse_strings(bytes: &[u8], count: usize) -> Vec<String> {
    bytes
        .split(|x| *x == 0)
        .take(count)
        .map(|b| String::from_utf8_lossy(b).to_string())
        .collect()
}

fn tags_from_raw<T>(indexes: &[Index<T>], data: &[u8]) -> Tags<T>
where
    T: FromPrimitive + Copy + Eq + Hash + Default,
{
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
