pub mod header;
pub mod raw;
pub mod payload;

use bzip2::read::BzDecoder;
use chrono::{Local, TimeZone};
use flate2::read::GzDecoder;
use itertools::multizip;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::SeekFrom;
use std::io::{self, Read, Seek};
use std::path::Path;
use zstd::stream::read::Decoder;

use header::{IndexArray, SigTag, Tag, Tags};
use raw::*;
use payload::*;

#[derive(Debug)]
pub struct RPMFile {
    pub sigtags: Tags<SigTag>,
    pub tags: Tags<Tag>,
    pub payload_offset: u64,
    pub file: File,
}

impl RPMFile {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<RPMFile, io::Error> {
        let mut file = OpenOptions::new().read(true).open(path)?;

        let _lead = RawLead::read(&mut file)?;

        let signature = RawHeader::read(&mut file)?;
        let indexes = IndexArray::read(&mut file, signature.nindex)?;
        let sigtags = Tags::read(&mut file, &indexes, signature.hsize as usize)?;

        // aligning to 8 bytes
        let pos = signature.hsize - 8 * (signature.hsize / 8);
        file.seek(io::SeekFrom::Current(pos.into()))?;

        let header = RawHeader::read(&mut file)?;
        let h_indexes = IndexArray::read(&mut file, header.nindex)?;
        let tags = Tags::read(&mut file, &h_indexes, header.hsize as usize)?;

        let payload_offset = file.seek(SeekFrom::Current(0))?;

        Ok(RPMFile {
            sigtags,
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

fn debug_some<R: Read + Seek>(file: &mut R) -> Result<(), io::Error> {
    let mut debug = [0_u8; 32];
    file.read_exact(&mut debug)?;
    println!("Bytes: {:?}", debug);
    Ok(())
}
