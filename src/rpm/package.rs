use super::file::decompress;
use crate::header::{RawHeader, RawTag, Type};
use crate::lead::Lead;
use crate::payload::{CpioRead, FileEntry, FileInfo};
use crate::utils::align_n_bytes;
use sha2::Digest;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Component, Path};

const MAX_FILES: usize = 262_144;
const MAX_PATH_BYTES: usize = 32 * 1024 * 1024;

pub(super) struct Package {
    pub file: File,
    pub lead: Lead,
    pub signature: RawHeader,
    pub header: RawHeader,
    pub header_start: u64,
    pub payload_offset: u64,
}
impl Package {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let lead = Lead::read(&mut file)?;
        let signature_start = file.stream_position()?;
        let signature = RawHeader::read(&mut file)?;
        let signature_size = file.stream_position()? - signature_start;
        file.seek(SeekFrom::Current(
            align_n_bytes(signature_size as u32, 8).into(),
        ))?;
        let header_start = file.stream_position()?;
        let header = RawHeader::read(&mut file)?;
        let payload_offset = file.stream_position()?;
        if header.get(100).is_some_and(|tag| tag.count > 1024) {
            return Err(unsupported(
                "Locale count exceeds editor/linter resource limit",
            ));
        }
        if let Some(tag) = header.get(5114) {
            let format = tag
                .number()
                .ok_or_else(|| invalid("Invalid package format tag"))?;
            if !matches!(format, 3 | 4) {
                return Err(unsupported(
                    "Editing and lint currently support RPM v3/v4 only",
                ));
            }
        }
        Ok(Self {
            file,
            lead,
            signature,
            header,
            header_start,
            payload_offset,
        })
    }
    pub fn compressor(&self) -> io::Result<String> {
        if self.header.get(1125).is_none_or(|tag| tag.count != 1) {
            return Err(invalid("Invalid compressor count"));
        }
        let values = self.header.strings(1125)?;
        if values.len() != 1 {
            return Err(invalid("Invalid compressor count"));
        }
        Ok(values[0].clone())
    }
    pub fn original_header(&mut self) -> io::Result<Vec<u8>> {
        self.file.seek(SeekFrom::Start(self.header_start))?;
        let size = self.payload_offset - self.header_start;
        if size > 64 * 1024 * 1024 {
            return Err(invalid("Header too large"));
        }
        let mut bytes = vec![0; size as usize];
        self.file.read_exact(&mut bytes)?;
        Ok(bytes)
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Data {
    pub offset: u64,
    pub size: u32,
}
pub(super) struct Record {
    pub entry: FileEntry,
    pub path: String,
    pub data: Data,
}
pub(super) struct Payload {
    pub file: tempfile::NamedTempFile,
    pub records: Vec<Record>,
}
impl Payload {
    pub fn read(package: &mut Package) -> io::Result<Self> {
        if package.header.get(1124).is_none_or(|tag| tag.count != 1) {
            return Err(invalid("Invalid payload format count"));
        }
        if package.header.strings(1124)?.as_slice() != ["cpio"] {
            return Err(unsupported("Unsupported payload format"));
        }
        let compressor = package.compressor()?;
        package.file.seek(SeekFrom::Start(package.payload_offset))?;
        let mut file = tempfile::NamedTempFile::new()?;
        io::copy(&mut decompress(&mut package.file, &compressor)?, &mut file)?;
        file.rewind()?;
        let mut records = Vec::new();
        let mut path_bytes = 0_usize;
        let mut names = HashSet::new();
        loop {
            let entry = file.read_cpio_entry()?;
            if entry.name == "TRAILER!!!" {
                break;
            }
            if Path::new(&entry.name).is_absolute() {
                return Err(invalid("Absolute CPIO path"));
            }
            let path = package_path(&entry.name)?;
            let file_type = entry.mode & 0o170000;
            if !matches!(
                file_type,
                0o100000 | 0o040000 | 0o120000 | 0o010000 | 0o020000 | 0o060000 | 0o140000
            ) {
                return Err(invalid("Invalid CPIO file type"));
            }
            if !matches!(file_type, 0o100000 | 0o120000) && entry.file_size != 0 {
                return Err(invalid("Non-data CPIO entry contains a payload"));
            }
            if file_type == 0o120000 && (entry.file_size == 0 || entry.file_size > 4096) {
                return Err(invalid("Invalid symlink target size"));
            }
            path_bytes = path_bytes
                .checked_add(path.len())
                .ok_or_else(|| invalid("Path size overflow"))?;
            if records.len() >= MAX_FILES || path_bytes > MAX_PATH_BYTES {
                return Err(unsupported(
                    "Payload file/path count exceeds editor/linter resource limits",
                ));
            }
            if !names.insert(path.clone()) {
                return Err(invalid("Duplicate CPIO path"));
            }
            let data = Data {
                offset: file.stream_position()?,
                size: entry.file_size,
            };
            file.read_cpio_entry_payload(&entry, &mut io::sink())?;
            records.push(Record { entry, path, data });
        }
        let mut bytes = [0; 8192];
        loop {
            let count = file.read(&mut bytes)?;
            if count == 0 {
                break;
            }
            if bytes[..count].iter().any(|byte| *byte != 0) {
                return Err(invalid("Nonzero data after CPIO trailer"));
            }
        }
        let mut groups = HashMap::new();
        for record in &records {
            if linkable(&record.entry) && record.entry.nlink > 1 {
                let key = (
                    record.entry.dev_major,
                    record.entry.dev_minor,
                    record.entry.ino,
                );
                if record.data.size > 0 || !groups.contains_key(&key) {
                    groups.insert(key, record.data);
                }
            }
        }
        for record in &mut records {
            if linkable(&record.entry) && record.entry.nlink > 1 {
                record.data = groups[&(
                    record.entry.dev_major,
                    record.entry.dev_minor,
                    record.entry.ino,
                )];
            }
        }
        Ok(Self { file, records })
    }
    pub fn digest(&mut self, data: Data, algorithm: u32) -> io::Result<String> {
        self.file.seek(SeekFrom::Start(data.offset))?;
        let mut reader = self.file.as_file_mut().take(data.size.into());
        match algorithm {
            1 => Ok(hex::encode(hash_reader::<md5::Md5>(&mut reader)?)),
            2 => Ok(hex::encode(hash_reader::<sha1::Sha1>(&mut reader)?)),
            8 => Ok(hex::encode(hash_reader::<sha2::Sha256>(&mut reader)?)),
            10 => Ok(hex::encode(hash_reader::<sha2::Sha512>(&mut reader)?)),
            _ => Err(unsupported("Unsupported file digest algorithm")),
        }
    }
    pub fn bytes(&mut self, data: Data) -> io::Result<Vec<u8>> {
        if data.size > 4096 {
            return Err(invalid("Symlink target too large"));
        }
        self.file.seek(SeekFrom::Start(data.offset))?;
        let mut bytes = vec![0; data.size as usize];
        self.file.read_exact(&mut bytes)?;
        Ok(bytes)
    }
    pub fn append_source(&mut self, path: &Path) -> io::Result<(FileEntry, Data)> {
        if !path.symlink_metadata()?.is_file() {
            return Err(invalid("File edits require a regular source file"));
        }
        let entry = FileEntry::try_from(&path.to_path_buf())?;
        let offset = self.file.seek(SeekFrom::End(0))?;
        let mut source = File::open(path)?.take(entry.file_size.into());
        if io::copy(&mut source, &mut self.file)? != u64::from(entry.file_size) {
            return Err(invalid("Source file changed or was truncated"));
        }
        let data = Data {
            offset,
            size: entry.file_size,
        };
        Ok((entry, data))
    }
}

pub(super) fn package_path(value: &str) -> io::Result<String> {
    let mut names = Vec::new();
    for component in Path::new(value).components() {
        match component {
            Component::Normal(value) => names.push(
                value
                    .to_str()
                    .ok_or_else(|| invalid("Non-UTF8 package path"))?,
            ),
            Component::RootDir | Component::CurDir => {}
            _ => return Err(invalid("Unsafe package path")),
        }
    }
    if names.is_empty() {
        return if value.is_empty() {
            Err(invalid("Empty package path"))
        } else {
            Ok(".".into())
        };
    }
    Ok(names.join("/"))
}
pub(super) fn installed_path(path: &str) -> String {
    if path == "." {
        "/".into()
    } else {
        format!("/{path}")
    }
}
pub(super) fn linkable(entry: &FileEntry) -> bool {
    matches!(entry.mode & 0o170000, 0o100000 | 0o120000)
}
pub(super) fn regular(entry: &FileEntry) -> bool {
    entry.mode & 0o170000 == 0o100000
}
pub(super) fn file_infos(header: &RawHeader) -> io::Result<Vec<FileInfo>> {
    for id in [1027, 1117, 1118] {
        if header
            .get(id)
            .is_some_and(|tag| tag.count as usize > MAX_FILES)
        {
            return Err(unsupported(
                "File-table count exceeds editor/linter resource limit",
            ));
        }
    }
    let mut path_bytes = 0_usize;
    let names = if let Some(tag) = header.get(1117) {
        let names = tag.strings()?;
        let directories = header.strings(1118)?;
        let indexes = numbers(header, 1116, names.len(), 0)?;
        names
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let directory = usize::try_from(indexes[index])
                    .ok()
                    .and_then(|index| directories.get(index))
                    .ok_or_else(|| invalid("File directory index out of range"))?;
                path_bytes = path_bytes
                    .checked_add(directory.len() + name.len())
                    .ok_or_else(|| invalid("Path size overflow"))?;
                if path_bytes > MAX_PATH_BYTES {
                    return Err(unsupported(
                        "Expanded file paths exceed editor/linter resource limit",
                    ));
                }
                Ok(format!("{directory}{name}"))
            })
            .collect::<io::Result<Vec<_>>>()?
    } else if let Some(tag) = header.get(1027) {
        tag.strings()?
    } else {
        return Ok(Vec::new());
    };
    let count = names.len();
    if count > 0 && (header.get(1039).is_none() || header.get(1040).is_none()) {
        return Err(invalid(
            "Missing per-file owner/group names; these cannot be inferred safely",
        ));
    }
    let sizes = numbers(
        header,
        if header.get(5008).is_some() {
            5008
        } else {
            1028
        },
        count,
        0,
    )?;
    let modes = numbers(header, 1030, count, 0)?;
    let times = numbers(header, 1034, count, 0)?;
    let flags = numbers(header, 1037, count, 0)?;
    let devices = numbers(header, 1095, count, 0)?;
    let inodes = numbers(header, 1096, count, 0)?;
    let users = strings(header, 1039, count, "root")?;
    let groups = strings(header, 1040, count, "root")?;
    if users.iter().chain(&groups).any(|name| name.is_empty()) {
        return Err(invalid(
            "Empty file owner/group names cannot be inferred safely",
        ));
    }
    let links = strings(header, 1036, count, "")?;
    let digests = strings(header, 1035, count, "")?;
    let mut seen = HashSet::new();
    names
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            let path = package_path(&name)?;
            if !seen.insert(path.clone()) {
                return Err(invalid("Duplicate metadata file path"));
            }
            Ok(FileInfo {
                name: installed_path(&path),
                size: sizes[index],
                mode: modes[index] as u16,
                mtime: times[index] as u32,
                flags: flags[index] as u32,
                device: devices[index] as u32,
                inode: inodes[index] as u32,
                user: users[index].clone(),
                group: groups[index].clone(),
                linkname: links[index].clone(),
                digest: digests[index].clone(),
            })
        })
        .collect()
}
fn numbers(header: &RawHeader, id: u32, count: usize, default: u64) -> io::Result<Vec<u64>> {
    if let Some(tag) = header.get(id) {
        if tag.count as usize != count {
            return Err(invalid(&format!("File array {id} has wrong length")));
        }
        let values = tag.numbers()?;
        let limit = match id {
            1030 => u64::from(u16::MAX),
            5008 => u64::MAX,
            _ => u64::from(u32::MAX),
        };
        if values.iter().any(|value| *value > limit) {
            return Err(invalid("File numeric value exceeds its format range"));
        }
        if values.len() != count {
            return Err(invalid(&format!("File array {id} has wrong length")));
        }
        Ok(values)
    } else {
        Ok(vec![default; count])
    }
}
fn strings(header: &RawHeader, id: u32, count: usize, default: &str) -> io::Result<Vec<String>> {
    if let Some(tag) = header.get(id) {
        if tag.count as usize != count {
            return Err(invalid(&format!("File array {id} has wrong length")));
        }
        let values = tag.strings()?;
        if values.len() != count {
            return Err(invalid(&format!("File array {id} has wrong length")));
        }
        Ok(values)
    } else {
        Ok(vec![default.into(); count])
    }
}
pub(super) fn hash_reader<D: Digest + Default>(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut digest = D::default();
    let mut bytes = [0; 8192];
    loop {
        let count = reader.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        digest.update(&bytes[..count]);
    }
    Ok(digest.finalize().to_vec())
}
pub(super) fn set_number(header: &mut RawHeader, id: u32, value: u64, long: u32) -> io::Result<()> {
    header.remove(id);
    header.remove(long);
    if let Ok(value) = u32::try_from(value) {
        header.set(RawTag::new(
            id,
            Type::Int32,
            1,
            value.to_be_bytes().to_vec(),
        )?)
    } else {
        header.set(RawTag::new(
            long,
            Type::Int64,
            1,
            value.to_be_bytes().to_vec(),
        )?)
    }
}
pub(super) fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
pub(super) fn unsupported(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message)
}
