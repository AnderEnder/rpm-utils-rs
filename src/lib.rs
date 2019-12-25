pub mod header;
pub mod lead;
pub mod payload;
mod utils;

use bzip2::read::BzDecoder;
use chrono::{Local, TimeZone};
use flate2::read::GzDecoder;
use itertools::multizip;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use xz2::read::XzDecoder;
use zstd::stream::read::Decoder;

use header::{HeaderLead, IndexArray, SignatureTag, Tag, Tags};
use lead::Lead;
use payload::{FileInfo, RPMPayload};
use utils::align_n_bytes;

#[derive(Debug)]
pub struct RPMFile<T> {
    pub signature_tags: Tags<SignatureTag>,
    pub header_tags: Tags<Tag>,
    pub payload_offset: u64,
    pub file: T,
}

impl RPMFile<File> {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, io::Error> {
        let mut file = OpenOptions::new().read(true).open(path)?;

        let _lead = Lead::read(&mut file)?;

        let signature_lead = HeaderLead::read(&mut file)?;
        let signature_indexes = IndexArray::read(&mut file, signature_lead.nindex)?;
        let signature_tags =
            Tags::read(&mut file, &signature_indexes, signature_lead.hsize as usize)?;

        // aligning to 8 bytes
        let pos = align_n_bytes(signature_lead.hsize, 8);

        file.seek(io::SeekFrom::Current(pos.into()))?;

        let header = HeaderLead::read(&mut file)?;
        let header_indexes = IndexArray::read(&mut file, header.nindex)?;
        let header_tags = Tags::read(&mut file, &header_indexes, header.hsize as usize)?;

        let payload_offset = file.seek(SeekFrom::Current(0))?;

        Ok(RPMFile {
            signature_tags,
            header_tags,
            file,
            payload_offset,
        })
    }
}

impl<T: 'static + Read + Seek> RPMFile<T> {
    pub fn copy_payload(self, path: &Path) -> Result<u64, io::Error> {
        let compressor: String = self
            .header_tags
            .get_value(Tag::PayloadCompressor)
            .unwrap()
            .as_string()
            .unwrap();
        let mut writer = OpenOptions::new().create(true).write(true).open(path)?;
        let mut reader = self.into_uncompress_reader(&compressor)?;
        io::copy(&mut reader, &mut writer)
    }

    fn into_uncompress_reader(mut self, compressor: &str) -> Result<Box<dyn Read>, io::Error> {
        self.file.seek(SeekFrom::Start(self.payload_offset))?;
        match compressor {
            "gzip" => Ok(Box::new(GzDecoder::new(self.file))),
            "bzip2" => Ok(Box::new(BzDecoder::new(self.file))),
            "zstd" => Ok(Box::new(Decoder::new(self.file)?)),
            "xz" | "lzma" => Ok(Box::new(XzDecoder::new(self.file))),
            format => Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Decompressor \"{}\" is not implemented", format),
            )),
        }
    }
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

impl<T: Read> From<&RPMFile<T>> for RPMInfo {
    fn from(rpm: &RPMFile<T>) -> Self {
        let dirs = rpm
            .header_tags
            .get_value(Tag::DirNames)
            .unwrap()
            .as_string_array()
            .unwrap();
        let dir_indexes = rpm
            .header_tags
            .get_value(Tag::DirIndexes)
            .unwrap()
            .as_u32_array()
            .unwrap();
        let basenames = rpm
            .header_tags
            .get_value(Tag::BaseNames)
            .unwrap()
            .as_string_array()
            .unwrap();
        let filesizes = rpm
            .header_tags
            .get_value(Tag::FileSizes)
            .unwrap()
            .as_u64_array()
            .unwrap();
        let users: Vec<String> = rpm
            .header_tags
            .get_value(Tag::FileUserName)
            .unwrap()
            .as_string_array()
            .unwrap();
        let groups: Vec<String> = rpm
            .header_tags
            .get_value(Tag::FileGroupName)
            .unwrap()
            .as_string_array()
            .unwrap();
        let flags: Vec<u32> = rpm
            .header_tags
            .get_value(Tag::FileFlags)
            .unwrap()
            .as_u32_array()
            .unwrap();
        let mtimes: Vec<u32> = rpm
            .header_tags
            .get_value(Tag::FileMTimes)
            .unwrap()
            .as_u32_array()
            .unwrap();
        let linknames: Vec<String> = rpm
            .header_tags
            .get_value(Tag::FileGroupName)
            .unwrap()
            .as_string_array()
            .unwrap();
        let modes: Vec<u16> = rpm
            .header_tags
            .get_value(Tag::FileModes)
            .unwrap()
            .as_u16_array()
            .unwrap();
        let devices: Vec<u32> = rpm
            .header_tags
            .get_value(Tag::FileDevices)
            .unwrap()
            .as_u32_array()
            .unwrap();
        let inodes: Vec<u32> = rpm
            .header_tags
            .get_value(Tag::FileInodes)
            .unwrap()
            .as_u32_array()
            .unwrap();
        let digests: Vec<String> = rpm
            .header_tags
            .get_value(Tag::FileMD5s)
            .unwrap()
            .as_string_array()
            .unwrap();

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
            size: rpm
                .signature_tags
                .get_value(SignatureTag::PayloadSize)
                .unwrap()
                .as_u64()
                .unwrap(),
            format: rpm
                .header_tags
                .get_value(Tag::PayloadFormat)
                .unwrap()
                .as_string()
                .unwrap(),
            compressor: rpm
                .header_tags
                .get_value(Tag::PayloadCompressor)
                .unwrap()
                .as_string()
                .unwrap(),
            flags: rpm
                .header_tags
                .get_value(Tag::PayloadFlags)
                .unwrap()
                .as_string()
                .unwrap(),
            files,
        };

        let build_int = rpm
            .header_tags
            .get_value(Tag::BuildTime)
            .unwrap()
            .as_u32()
            .unwrap();
        let build_time = Local
            .timestamp(i64::from(build_int), 0)
            .format("%c")
            .to_string();

        RPMInfo {
            name: rpm
                .header_tags
                .get_value(Tag::Name)
                .unwrap()
                .as_string()
                .unwrap(),
            version: rpm
                .header_tags
                .get_value(Tag::Version)
                .unwrap()
                .as_string()
                .unwrap(),
            release: rpm
                .header_tags
                .get_value(Tag::Release)
                .unwrap()
                .as_string()
                .unwrap(),
            arch: rpm
                .header_tags
                .get_value(Tag::Arch)
                .unwrap()
                .as_string()
                .unwrap(),
            group: rpm
                .header_tags
                .get_value(Tag::Group)
                .unwrap()
                .as_string()
                .unwrap(),
            size: rpm
                .header_tags
                .get_value(Tag::Size)
                .unwrap()
                .as_u64()
                .unwrap(),
            license: rpm
                .header_tags
                .get_value(Tag::License)
                .unwrap()
                .as_string()
                .unwrap(),
            source_rpm: rpm
                .header_tags
                .get_value(Tag::SourceRpm)
                .unwrap()
                .as_string()
                .unwrap(),
            build_time,
            build_host: rpm
                .header_tags
                .get_value(Tag::BuildHost)
                .unwrap()
                .as_string()
                .unwrap(),
            summary: rpm
                .header_tags
                .get_value(Tag::Summary)
                .unwrap()
                .as_string()
                .unwrap(),
            description: rpm
                .header_tags
                .get_value(Tag::Description)
                .unwrap()
                .as_string()
                .unwrap(),
            payload,
        }
    }
}
