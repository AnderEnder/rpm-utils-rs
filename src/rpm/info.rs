use chrono::{Local, TimeZone};
use itertools::multizip;
use std::convert::TryInto;
use std::fmt;
use std::io::{self, Read, Write};

use super::file::RPMFile;
use crate::header::{RType, SignatureTag, Tag, Tags};
use crate::lead::Lead;
use crate::payload::{FileInfo, RPMPayload};

#[derive(Debug, Default)]
pub struct RPMInfo {
    pub name: String,
    pub epoch: u8,
    pub version: String,
    pub release: String,
    pub arch: String,
    pub group: String,
    pub size: u64,
    pub license: String,
    pub source_rpm: String,
    pub build_time: i64,
    pub build_host: String,
    pub summary: String,
    pub description: String,
    pub payload: RPMPayload,
}

impl fmt::Display for RPMInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let build_time = Local.timestamp(self.build_time, 0).format("%c").to_string();

        writeln!(f, "Name        : {}", self.name)?;
        writeln!(f, "Version     : {}", self.version)?;
        writeln!(f, "Release     : {}", self.release)?;
        writeln!(f, "Architecture: {}", self.arch)?;
        writeln!(f, "Group       : {}", self.group)?;
        writeln!(f, "Size        : {}", self.size)?;
        writeln!(f, "License     : {}", self.license)?;
        writeln!(f, "Signature   : (unimplemented)")?;
        writeln!(f, "Source RPM  : {}", self.source_rpm)?;
        writeln!(f, "Build Date  : {}", build_time)?;
        writeln!(f, "Build Host  : {}", self.build_host)?;
        writeln!(f, "Relocations : (unimplemented)")?;
        writeln!(f, "Summary     : {}", self.summary)?;
        writeln!(f, "Description : \n{}", self.description)
    }
}

impl<T: Read> From<&RPMFile<T>> for RPMInfo {
    fn from(rpm: &RPMFile<T>) -> Self {
        let RPMFile {
            signature_tags,
            header_tags,
            ..
        } = rpm;

        let dirs = header_tags.get_as_string_array_or(Tag::DirNames);
        let dir_indexes = header_tags.get_as_u32_array_or(Tag::DirIndexes);
        let basenames = header_tags.get_as_string_array_or(Tag::BaseNames);
        let filesizes = header_tags.get_as_u64_array_or(Tag::FileSizes);
        let users = header_tags.get_as_string_array_or(Tag::FileUserName);
        let groups = header_tags.get_as_string_array_or(Tag::FileGroupName);
        let flags = header_tags.get_as_u32_array_or(Tag::FileFlags);
        let mtimes = header_tags.get_as_u32_array_or(Tag::FileMTimes);
        let linknames = header_tags.get_as_string_array_or(Tag::FileGroupName);
        let modes = header_tags.get_as_u16_array_or(Tag::FileModes);
        let devices = header_tags.get_as_u32_array_or(Tag::FileDevices);
        let inodes = header_tags.get_as_u32_array_or(Tag::FileInodes);
        let digests = header_tags.get_as_string_array_or(Tag::FileMD5s);

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
            size: signature_tags.get_as_u64(SignatureTag::PayloadSize),
            format: header_tags.get_as_string(Tag::PayloadFormat),
            compressor: header_tags.get_as_string(Tag::PayloadCompressor),
            flags: header_tags.get_as_string(Tag::PayloadFlags),
            files,
        };

        RPMInfo {
            name: header_tags.get_as_string(Tag::Name),
            epoch: header_tags.get_as_u8_default(Tag::Epoch),
            version: header_tags.get_as_string(Tag::Version),
            release: header_tags.get_as_string(Tag::Release),
            arch: header_tags.get_as_string(Tag::Arch),
            group: header_tags.get_as_string(Tag::Group),
            size: header_tags.get_as_u64(Tag::Size),
            license: header_tags.get_as_string_or(Tag::License),
            source_rpm: header_tags.get_as_string_or(Tag::SourceRpm),
            build_time: header_tags.get_as_i64(Tag::BuildTime),
            build_host: header_tags.get_as_string(Tag::BuildHost),
            summary: header_tags.get_as_string(Tag::Summary),
            description: header_tags.get_as_string(Tag::Description),
            payload,
        }
    }
}

impl RPMInfo {
    pub fn into_rpm<T: Write>(self, writer: T) -> RPMFile<T> {
        let lead = Lead::from(&self);
        let mut signature_tags = Tags::<SignatureTag>::new();
        let mut header_tags = Tags::<Tag>::new();

        header_tags
            .insert(Tag::Name, RType::String(self.name))
            .insert(Tag::Epoch, RType::Int8(self.epoch))
            .insert(Tag::Version, RType::String(self.version))
            .insert(Tag::Arch, RType::String(self.arch))
            .insert(Tag::Group, RType::String(self.group))
            .insert(Tag::Size, RType::Int64(self.size))
            .insert(Tag::License, RType::String(self.license))
            .insert(Tag::SourceRpm, RType::String(self.source_rpm))
            .insert(
                Tag::BuildTime,
                RType::Int64(self.build_time.try_into().unwrap()),
            )
            .insert(Tag::BuildHost, RType::String(self.build_host))
            .insert(Tag::Summary, RType::String(self.summary))
            .insert(Tag::Description, RType::String(self.description))
            .insert(Tag::PayloadFormat, RType::String(self.payload.format))
            .insert(
                Tag::PayloadCompressor,
                RType::String(self.payload.compressor),
            )
            .insert(Tag::PayloadFlags, RType::String(self.payload.flags));

        signature_tags.insert(SignatureTag::PayloadSize, RType::Int64(self.payload.size));

        RPMFile {
            lead,
            header_tags,
            signature_tags,
            payload_offset: 0,
            file: writer,
        }
    }
}

impl From<&RPMInfo> for Lead {
    fn from(info: &RPMInfo) -> Self {
        let mut name = [0_u8; 66];
        info.name.as_bytes().read(&mut name).unwrap();

        Self {
            major: 4,
            minor: 3,
            name,
            ..Default::default()
        }
    }
}
