use chrono::{Local, TimeZone};
use std::fmt;
use std::io::{self, Read, Write};

use super::file::RPMFile;
use crate::header::{SignatureTag, Tag, Tags};
use crate::lead::Lead;
use crate::payload::{FileInfo, RPMPayload};

#[derive(Debug, Default)]
pub struct RPMInfo {
    pub name: String,
    pub epoch: u32,
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
        let build_time = Local
            .timestamp_opt(self.build_time, 0)
            .single()
            .map(|time| time.format("%c").to_string())
            .unwrap_or_else(|| self.build_time.to_string());

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

        let string = |tag| {
            header_tags
                .get_value(tag)
                .and_then(|v| v.as_string())
                .unwrap_or_default()
        };
        let number = |tag| {
            header_tags
                .get_value(tag)
                .and_then(|v| v.as_u64())
                .unwrap_or_default()
        };
        let strings = |tag| {
            header_tags
                .get_value(tag)
                .and_then(|v| v.as_string_array())
                .unwrap_or_default()
        };
        let numbers = |tag| {
            header_tags
                .get_value(tag)
                .and_then(|v| v.as_u64_array().or_else(|| v.as_u64().map(|v| vec![v])))
                .unwrap_or_default()
        };
        let dirs = strings(Tag::DirNames);
        let dir_indexes = numbers(Tag::DirIndexes);
        let basenames = strings(Tag::BaseNames);
        let filesizes = if header_tags.get_value(Tag::LongFileSizes).is_some() {
            numbers(Tag::LongFileSizes)
        } else {
            numbers(Tag::FileSizes)
        };
        let users = strings(Tag::FileUserName);
        let groups = strings(Tag::FileGroupName);
        let flags = numbers(Tag::FileFlags);
        let mtimes = numbers(Tag::FileMTimes);
        let linknames = strings(Tag::FileLinktos);
        let modes = numbers(Tag::FileModes);
        let devices = numbers(Tag::FileDevices);
        let inodes = numbers(Tag::FileInodes);
        let digests = strings(Tag::FileMD5s);

        let files = basenames
            .into_iter()
            .enumerate()
            .map(|(i, name)| {
                let directory = dir_indexes
                    .get(i)
                    .and_then(|index| usize::try_from(*index).ok())
                    .and_then(|index| dirs.get(index))
                    .cloned()
                    .unwrap_or_default();
                FileInfo {
                    name: directory + &name,
                    size: filesizes.get(i).copied().unwrap_or_default(),
                    user: users.get(i).cloned().unwrap_or_default(),
                    group: groups.get(i).cloned().unwrap_or_default(),
                    flags: flags.get(i).copied().unwrap_or_default() as u32,
                    mtime: mtimes.get(i).copied().unwrap_or_default() as u32,
                    digest: digests.get(i).cloned().unwrap_or_default(),
                    mode: modes.get(i).copied().unwrap_or_default() as u16,
                    linkname: linknames.get(i).cloned().unwrap_or_default(),
                    device: devices.get(i).copied().unwrap_or_default() as u32,
                    inode: inodes.get(i).copied().unwrap_or_default() as u32,
                }
            })
            .collect();
        let payload = RPMPayload {
            file_digest_algorithm: number(Tag::FileDigestAlgo) as u32,
            size: signature_tags
                .get_value(SignatureTag::LongArchiveSize)
                .or_else(|| signature_tags.get_value(SignatureTag::PayloadSize))
                .and_then(|v| v.as_u64())
                .unwrap_or_default(),
            format: string(Tag::PayloadFormat),
            compressor: string(Tag::PayloadCompressor),
            flags: string(Tag::PayloadFlags),
            files,
        };
        RPMInfo {
            name: string(Tag::Name),
            epoch: number(Tag::Epoch) as u32,
            version: string(Tag::Version),
            release: string(Tag::Release),
            arch: string(Tag::Arch),
            group: string(Tag::Group),
            size: if header_tags.get_value(Tag::LongSize).is_some() {
                number(Tag::LongSize)
            } else {
                number(Tag::Size)
            },
            license: string(Tag::License),
            source_rpm: string(Tag::SourceRpm),
            build_time: header_tags
                .get_value(Tag::BuildTime)
                .and_then(|v| v.as_i64())
                .unwrap_or_default(),
            build_host: string(Tag::BuildHost),
            summary: string(Tag::Summary),
            description: string(Tag::Description),
            payload,
        }
    }
}

impl RPMInfo {
    /// Legacy infallible conversion; panics for an invalid build timestamp.
    /// Prefer try_into_rpm() for values originating outside the application.
    pub fn into_rpm<T: Write>(self, writer: T) -> RPMFile<T> {
        self.try_into_rpm(writer).expect("Invalid RPM metadata")
    }

    pub fn try_into_rpm<T: Write>(self, writer: T) -> io::Result<RPMFile<T>> {
        if u32::try_from(self.build_time).is_err() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Build time must fit u32",
            ));
        }
        let lead = Lead::from(&self);
        let mut signature_tags = Tags::<SignatureTag>::new();
        let mut header_tags = Tags::<Tag>::new();

        header_tags
            .insert_name(self.name)
            .insert_epoch(self.epoch)
            .insert_version(self.version)
            .insert(Tag::Release, crate::header::RType::String(self.release))
            .insert_arch(self.arch)
            .insert_group(self.group)
            .insert_size(self.size)
            .insert_license(self.license)
            .insert_source_rpm(self.source_rpm)
            .insert_build_time(self.build_time)
            .insert_build_host(self.build_host)
            .insert_summary(self.summary)
            .insert_description(self.description)
            .insert_payload_format(self.payload.format)
            .insert_payload_compressor(self.payload.compressor)
            .insert_payload_flags(self.payload.flags);

        super::builder::insert_file_tags(&mut header_tags, &self.payload.files);
        header_tags.insert(
            Tag::I18nTable,
            crate::header::RType::StringArray(vec!["C".into()]),
        );
        if self.payload.file_digest_algorithm != 0 {
            header_tags.insert(
                Tag::FileDigestAlgo,
                crate::header::RType::Int32(self.payload.file_digest_algorithm),
            );
        }
        signature_tags.insert_payload_size(self.payload.size);

        Ok(RPMFile {
            lead,
            header_tags,
            signature_tags,
            payload_offset: 0,
            file: writer,
        })
    }
}

impl From<&RPMInfo> for Lead {
    fn from(info: &RPMInfo) -> Self {
        let mut name = [0_u8; 66];

        let size = info.name.len().min(65);
        name[..size].copy_from_slice(&info.name.as_bytes()[..size]);

        Self {
            major: 3,
            minor: 0,
            name,
            ..Default::default()
        }
    }
}
