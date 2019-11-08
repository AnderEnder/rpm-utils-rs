pub mod header;
pub mod payload;
pub mod raw;

use bzip2::read::BzDecoder;
use chrono::{Local, TimeZone};
use flate2::read::GzDecoder;
use itertools::multizip;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use zstd::stream::read::Decoder;

use header::{HeaderLead, IndexArray, SignatureTag, Tag, Tags};
use payload::{FileInfo, RPMPayload};
use raw::Lead;

#[derive(Debug)]
pub struct RPMFile {
    pub signature_tags: Tags<SignatureTag>,
    pub header_tags: Tags<Tag>,
    pub payload_offset: u64,
    pub file: File,
}

impl RPMFile {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<RPMFile, io::Error> {
        let mut file = OpenOptions::new().read(true).open(path)?;

        let _lead = Lead::read(&mut file)?;

        let signature_lead = HeaderLead::read(&mut file)?;
        let signature_indexes = IndexArray::read(&mut file, signature_lead.nindex)?;
        let signature_tags =
            Tags::read(&mut file, &signature_indexes, signature_lead.hsize as usize)?;

        // aligning to 8 bytes
        let pos = signature_lead.hsize - 8 * (signature_lead.hsize / 8);
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

    pub fn copy_payload(mut self, path: &Path) -> Result<u64, io::Error> {
        let compressor: String = self.header_tags.get(Tag::Payloadcompressor);
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
    fn from(rpm: &RPMFile) -> Self {
        let dirs: Vec<String> = rpm.header_tags.get(Tag::DirNames);
        let dir_indexes: Vec<u32> = rpm.header_tags.get(Tag::Dirindexes);
        let basenames: Vec<String> = rpm.header_tags.get(Tag::Basenames);
        let filesizes: Vec<u64> = rpm.header_tags.get(Tag::FileSizes);
        let users: Vec<String> = rpm.header_tags.get(Tag::FileUserName);
        let groups: Vec<String> = rpm.header_tags.get(Tag::FileGroupName);
        let flags: Vec<u32> = rpm.header_tags.get(Tag::FileFlags);
        let mtimes: Vec<u32> = rpm.header_tags.get(Tag::FileMTimes);
        let linknames: Vec<String> = rpm.header_tags.get(Tag::FileGroupName);
        let modes: Vec<u16> = rpm.header_tags.get(Tag::FileModes);
        let devices: Vec<u32> = rpm.header_tags.get(Tag::FileDevices);
        let inodes: Vec<u32> = rpm.header_tags.get(Tag::FileInodes);
        let digests: Vec<String> = rpm.header_tags.get(Tag::FileMD5s);

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
            size: rpm.signature_tags.get(SignatureTag::PayloadSize),
            format: rpm.header_tags.get(Tag::Payloadformat),
            compressor: rpm.header_tags.get(Tag::Payloadcompressor),
            flags: rpm.header_tags.get(Tag::Payloadflags),
            files,
        };

        let build_int: u32 = rpm.header_tags.get(Tag::BuildTime);
        let build_time = Local
            .timestamp(i64::from(build_int), 0)
            .format("%c")
            .to_string();

        RPMInfo {
            name: rpm.header_tags.get(Tag::Name),
            version: rpm.header_tags.get(Tag::Version),
            release: rpm.header_tags.get(Tag::Release),
            arch: rpm.header_tags.get(Tag::Arch),
            group: rpm.header_tags.get(Tag::Group),
            size: rpm.header_tags.get(Tag::Size),
            license: rpm.header_tags.get(Tag::License),
            source_rpm: rpm.header_tags.get(Tag::SourceRpm),
            build_time,
            build_host: rpm.header_tags.get(Tag::BuildHost),
            summary: rpm.header_tags.get(Tag::Summary),
            description: rpm.header_tags.get(Tag::Description),
            payload,
        }
    }
}
