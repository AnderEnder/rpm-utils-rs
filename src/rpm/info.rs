use chrono::{Local, TimeZone};
use itertools::multizip;
use std::fmt;
use std::io::Read;

use crate::header::RType;
use crate::header::{SignatureTag, Tag};
use crate::payload::{FileInfo, RPMPayload};

use super::file::RPMFile;

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
            .unwrap_or(RType::StringArray(Vec::new()))
            .as_string_array()
            .unwrap();
        let dir_indexes = rpm
            .header_tags
            .get_value(Tag::DirIndexes)
            .unwrap_or(RType::Int32Array(Vec::new()))
            .as_u32_array()
            .unwrap();
        let basenames = rpm
            .header_tags
            .get_value(Tag::BaseNames)
            .unwrap_or(RType::StringArray(Vec::new()))
            .as_string_array()
            .unwrap();
        let filesizes = rpm
            .header_tags
            .get_value(Tag::FileSizes)
            .unwrap_or(RType::Int64Array(Vec::new()))
            .as_u64_array()
            .unwrap();
        let users: Vec<String> = rpm
            .header_tags
            .get_value(Tag::FileUserName)
            .unwrap_or(RType::StringArray(Vec::new()))
            .as_string_array()
            .unwrap();
        let groups: Vec<String> = rpm
            .header_tags
            .get_value(Tag::FileGroupName)
            .unwrap_or(RType::StringArray(Vec::new()))
            .as_string_array()
            .unwrap();
        let flags: Vec<u32> = rpm
            .header_tags
            .get_value(Tag::FileFlags)
            .unwrap_or(RType::Int32Array(Vec::new()))
            .as_u32_array()
            .unwrap();
        let mtimes: Vec<u32> = rpm
            .header_tags
            .get_value(Tag::FileMTimes)
            .unwrap_or(RType::Int32Array(Vec::new()))
            .as_u32_array()
            .unwrap();
        let linknames: Vec<String> = rpm
            .header_tags
            .get_value(Tag::FileGroupName)
            .unwrap_or(RType::StringArray(Vec::new()))
            .as_string_array()
            .unwrap();
        let modes: Vec<u16> = rpm
            .header_tags
            .get_value(Tag::FileModes)
            .unwrap_or(RType::Int16Array(Vec::new()))
            .as_u16_array()
            .unwrap();
        let devices: Vec<u32> = rpm
            .header_tags
            .get_value(Tag::FileDevices)
            .unwrap_or(RType::Int32Array(Vec::new()))
            .as_u32_array()
            .unwrap();
        let inodes: Vec<u32> = rpm
            .header_tags
            .get_value(Tag::FileInodes)
            .unwrap_or(RType::Int32Array(Vec::new()))
            .as_u32_array()
            .unwrap();
        let digests: Vec<String> = rpm
            .header_tags
            .get_value(Tag::FileMD5s)
            .unwrap_or(RType::StringArray(Vec::new()))
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
                .unwrap_or(RType::String("".to_owned()))
                .as_string()
                .unwrap(),
            source_rpm: rpm
                .header_tags
                .get_value(Tag::SourceRpm)
                .unwrap_or(RType::String("".to_owned()))
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
