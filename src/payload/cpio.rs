use filetime::{FileTime, set_file_mtime};
use std::convert::{TryFrom, TryInto};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};

use crate::utils::{HexReader, HexWriter, align_n_bytes};

const MAGIC: &[u8] = b"070701";
const TRAILER: &str = "TRAILER!!!";

/// Maximum allowed CPIO entry name size (4 KB) - prevents OOM attacks
const MAX_NAME_SIZE: u32 = 4096;
/// Maximum allowed CPIO entry file size (1 GB) - prevents OOM attacks
const MAX_CPIO_ENTRY_SIZE: u32 = 1024 * 1024 * 1024;

/// Check if a path is safe for extraction (no path traversal attempts)
///
/// Returns false if the path:
/// - Contains ".." components (path traversal)
/// - Is an absolute path (including Unix-style paths like "/etc" on Windows)
/// - Starts with a path separator (cross-platform absolute path detection)
fn is_safe_path(path: &Path) -> bool {
    let has_traversal = path.components().any(|c| matches!(c, Component::ParentDir));
    let is_absolute = path.is_absolute();

    // On Windows, is_absolute() returns false for Unix-style paths like "/etc/passwd"
    // So we also check if the path starts with a separator
    let path_str = path.to_string_lossy();
    let starts_with_separator = path_str.starts_with('/') || path_str.starts_with('\\');

    !has_traversal && !is_absolute && !starts_with_separator
}

#[derive(Debug, PartialEq)]
pub struct FileEntry {
    pub name: String,
    pub ino: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u32,
    pub mtime: u32,
    pub file_size: u32,
    pub dev_major: u32,
    pub dev_minor: u32,
    pub rdev_major: u32,
    pub rdev_minor: u32,
}

impl FileEntry {
    pub fn read<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut magic = [0_u8; 6];
        reader.read_exact(&mut magic)?;

        if magic != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Error: incorrect magic of cpio entry {:x?}", magic),
            ));
        }

        let ino = reader.read_hex_as_u32()?;
        let mode = reader.read_hex_as_u32()?;
        let uid = reader.read_hex_as_u32()?;
        let gid = reader.read_hex_as_u32()?;
        let nlink = reader.read_hex_as_u32()?;
        let mtime = reader.read_hex_as_u32()?;
        let file_size = reader.read_hex_as_u32()?;

        // Validate file size to prevent OOM attacks
        if file_size > MAX_CPIO_ENTRY_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("CPIO entry file size {} exceeds maximum allowed size {}", file_size, MAX_CPIO_ENTRY_SIZE),
            ));
        }

        let dev_major = reader.read_hex_as_u32()?;
        let dev_minor = reader.read_hex_as_u32()?;
        let rdev_major = reader.read_hex_as_u32()?;
        let rdev_minor = reader.read_hex_as_u32()?;
        let name_size = reader.read_hex_as_u32()?;

        // Validate name size to prevent OOM attacks
        if name_size > MAX_NAME_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("CPIO entry name size {} exceeds maximum allowed size {}", name_size, MAX_NAME_SIZE),
            ));
        }

        let mut checksum = [0_u8; 8];
        reader.read_exact(&mut checksum)?;

        // optimise later
        let mut name_bytes = vec![0_u8; name_size as usize];
        reader.read_exact(&mut name_bytes)?;
        let name = if name_size > 0 {
            let size = (name_size - 1) as usize;
            String::from_utf8(name_bytes[0..size].to_vec()).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Error: incorrect utf8 symbol: {}", e),
                )
            })?
        } else {
            return Err(io::Error::new(io::ErrorKind::Other, "incorrect cpio name"));
        };

        // aligning to 4 bytes: name +
        let position = align_n_bytes(name_size + 6, 4);
        let mut tmp_bytes = vec![0_u8; position as usize];
        reader.read_exact(&mut tmp_bytes)?;

        Ok(FileEntry {
            name,
            ino,
            mode,
            uid,
            gid,
            nlink,
            mtime,
            file_size,
            dev_major,
            dev_minor,
            rdev_major,
            rdev_minor,
        })
    }

    fn write<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(MAGIC)?;
        writer.write_u32_as_hex(self.ino)?;
        writer.write_u32_as_hex(self.mode)?;
        writer.write_u32_as_hex(self.uid)?;
        writer.write_u32_as_hex(self.gid)?;
        writer.write_u32_as_hex(self.nlink)?;
        writer.write_u32_as_hex(self.mtime)?;
        writer.write_u32_as_hex(self.file_size)?;
        writer.write_u32_as_hex(self.dev_major)?;
        writer.write_u32_as_hex(self.dev_minor)?;
        writer.write_u32_as_hex(self.rdev_major)?;
        writer.write_u32_as_hex(self.rdev_minor)?;
        let name_size = (self.name.len() + 1) as u32;
        writer.write_u32_as_hex(name_size)?;
        let checksum = [0_u8; 8];
        writer.write_all(&checksum)?;

        let mut name = self.name.as_bytes().to_vec();
        name.push(0_u8);
        writer.write_all(&name)?;

        // aligning to 4 bytes
        let number = align_n_bytes(name_size + 6, 4) as usize;
        let pad = vec![0_u8; number];
        writer.write_all(&pad)
    }
}

impl Default for FileEntry {
    fn default() -> Self {
        FileEntry {
            name: TRAILER.to_owned(),
            ino: 0,
            mode: 0,
            uid: 0,
            gid: 0,
            nlink: 1,
            mtime: 0,
            file_size: 0,
            dev_major: 0,
            dev_minor: 0,
            rdev_major: 0,
            rdev_minor: 0,
        }
    }
}

impl TryFrom<&PathBuf> for FileEntry {
    type Error = io::Error;

    fn try_from(f: &PathBuf) -> Result<Self, Self::Error> {
        let meta = f.metadata()?;
        let name = f
            .file_name()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("cannot find filename from path {:?}", f),
                )
            })?
            .to_str()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("cannot parse path {:?} to string", f),
                )
            })?
            .to_owned();

        #[cfg(all(unix))]
        {
            use std::os::unix::fs::MetadataExt;
            Ok(FileEntry {
                name,
                ino: meta.ino() as u32,
                mode: meta.mode(),
                uid: meta.uid(),
                gid: meta.gid(),
                nlink: meta.nlink() as u32,
                mtime: meta.mtime() as u32,
                file_size: meta.size() as u32,
                dev_major: major(meta.dev() as u32),
                dev_minor: minor(meta.dev() as u32),
                rdev_major: major(meta.rdev() as u32),
                rdev_minor: minor(meta.rdev() as u32),
            })
        }
        #[cfg(all(windows))]
        {
            // TODO: reimplement properly for Windows
            use std::os::windows::fs::MetadataExt;
            Ok(FileEntry {
                name,
                ino: 1,
                mode: meta.file_attributes() as u32,
                uid: 0,
                gid: 0,
                nlink: 0,
                mtime: meta.last_write_time() as u32,
                file_size: meta.file_size() as u32,
                dev_major: 0,
                dev_minor: 0,
                rdev_major: 0,
                rdev_minor: 0,
            })
        }
    }
}

fn major(x: u32) -> u32 {
    (x >> 8) & 0x7F
}

fn minor(x: u32) -> u32 {
    x & 0xFF
}

pub fn read_entries<R: Read + Seek>(reader: &mut R) -> io::Result<Vec<FileEntry>> {
    let mut entries = Vec::new();

    loop {
        let entry = FileEntry::read(reader)?;
        let position = align_n_bytes(entry.file_size, 4) + entry.file_size;
        reader.seek(io::SeekFrom::Current(position.into()))?;
        if entry.name == TRAILER {
            break;
        }
        entries.push(entry);
    }
    Ok(entries)
}

pub fn read_entry<R: Read + Seek, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> io::Result<(FileEntry, u64)> {
    let entry = FileEntry::read(reader)?;
    let number = io_copy_exact(reader, writer, entry.file_size)?;
    let position = align_n_bytes(entry.file_size, 4);
    reader.seek(io::SeekFrom::Current(position.into()))?;
    Ok((entry, number.into()))
}

pub fn extract_entry<R: Read + Seek>(
    reader: &mut R,
    dir: &Path,
    creates_dir: bool,
    change_owner: bool,
) -> io::Result<(FileEntry, u64)> {
    let entry = FileEntry::read(reader)?;

    // write content to file only if it is not a last pseudo
    if entry.name != TRAILER {
        let path = dir.join(&entry.name);

        // Validate path safety - prevent path traversal attacks
        // First check: reject paths with ".." components or absolute paths
        if !is_safe_path(&Path::new(&entry.name)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Unsafe path in archive (potential path traversal): {}", entry.name),
            ));
        }

        // Second check: ensure the resolved path stays within the extraction directory
        // This protects against complex traversals that might bypass component checks
        let canonical_dir = dir.canonicalize().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Cannot canonicalize extraction directory: {}", e),
            )
        })?;

        // Create parent directories if needed before validation
        // This ensures we can canonicalize paths for validation
        if entry.nlink == 2 {
            // Entry is a directory
            if !path.exists() {
                std::fs::create_dir_all(&path)?;
            }
        } else {
            // Entry is a file - ensure parent directory exists
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    if creates_dir {
                        std::fs::create_dir_all(parent)?;
                    } else {
                        // Parent doesn't exist and we're not allowed to create it
                        return Err(io::Error::new(
                            io::ErrorKind::NotFound,
                            format!("Parent directory does not exist: {:?}", parent),
                        ));
                    }
                }
            }
        }

        // Now validate that the path (or its parent for new files) is within the extraction directory
        let canonical_path = if path.exists() {
            path.canonicalize()?
        } else {
            // Path doesn't exist yet - validate using parent directory
            let parent = path.parent().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Invalid path: no parent directory")
            })?;

            // Parent should exist now (we created it above if needed)
            let canonical_parent = parent.canonicalize().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Cannot canonicalize parent directory: {}", e),
                )
            })?;

            // Construct expected canonical path
            let filename = path.file_name().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Invalid path: no filename")
            })?;
            canonical_parent.join(filename)
        };

        // Verify the canonical path is within the canonical extraction directory
        if !canonical_path.starts_with(&canonical_dir) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Path escapes extraction directory: {}", entry.name),
            ));
        }

        let mut number = 0;

        if entry.nlink == 2 {
            // Directory already created above for validation
        } else {
            // Parent directory already created above for validation
            let mut writer = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&path)?;
            number = io_copy_exact(reader, &mut writer, entry.file_size)?;

            let position = align_n_bytes(entry.file_size, 4);
            reader.seek(io::SeekFrom::Current(position.into()))?;
        }

        #[cfg(all(unix))]
        {
            if change_owner {
                use nix::unistd::{Gid, Uid, chown};
                use std::os::unix::fs::PermissionsExt;

                let metadata = path.metadata()?;
                let mut permissions = metadata.permissions();
                permissions.set_mode(entry.mode);
                std::fs::set_permissions(&path, permissions)?;
                chown(
                    &path,
                    Some(Uid::from_raw(entry.uid)),
                    Some(Gid::from_raw(entry.gid)),
                )
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Error: can not change owner {}", e),
                    )
                })?
            }
        }

        let mtime = FileTime::from_unix_time(entry.mtime.into(), 0);
        set_file_mtime(&path, mtime)?;
        Ok((entry, number.into()))
    } else {
        Ok((entry, 0))
    }
}

pub fn extract_entries<R: Read + Seek>(
    reader: &mut R,
    dir: &Path,
    creates_dir: bool,
    change_owner: bool,
) -> io::Result<Vec<FileEntry>> {
    let mut entries = Vec::new();
    loop {
        let (entry, _) = extract_entry(reader, dir, creates_dir, change_owner)?;
        if entry.name == TRAILER {
            break;
        }
        entries.push(entry);
    }
    Ok(entries)
}

const BUFSIZE: usize = 8 * 1024;

fn io_copy_exact<R: Read, W: Write>(reader: &mut R, writer: &mut W, count: u32) -> io::Result<u32> {
    let mut buf = [0_u8; BUFSIZE];
    let buf_count = count as usize / BUFSIZE;
    let buf_left = count as usize % BUFSIZE;

    for _ in 0..buf_count {
        reader.read_exact(&mut buf)?;
        writer.write_all(&buf)?;
    }

    if buf_left > 0 {
        let mut buf2 = vec![0_u8; buf_left];
        reader.read_exact(&mut buf2)?;
        writer.write_all(&buf2)?;
    }

    Ok(count)
}

struct CpioFiles<T> {
    reader: T,
}

impl<T: Read + Seek> CpioFiles<T> {
    pub fn new(reader: T) -> Self {
        CpioFiles { reader }
    }
}

impl<T: Read + Seek> Iterator for CpioFiles<T> {
    type Item = (FileEntry, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        let mut bytes = Vec::new();
        let (entry, _) = read_entry(&mut self.reader, &mut bytes).unwrap();
        if entry.name != TRAILER {
            Some((entry, bytes))
        } else {
            None
        }
    }
}

struct CpioEntries<T> {
    reader: T,
}

impl<T: Read + Seek> CpioEntries<T> {
    pub fn new(reader: T) -> Self {
        CpioEntries { reader }
    }
}

impl<T: Read + Seek> Iterator for CpioEntries<T> {
    type Item = FileEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = FileEntry::read(&mut self.reader).unwrap();
        let position = align_n_bytes(entry.file_size, 4) + entry.file_size;
        self.reader
            .seek(io::SeekFrom::Current(position.into()))
            .unwrap();

        if entry.name != TRAILER {
            Some(entry)
        } else {
            None
        }
    }
}

pub trait CpioRead {
    fn read_cpio_entry(&mut self) -> io::Result<FileEntry>;
    fn read_cpio_entry_payload<W: Write>(
        &mut self,
        entry: &FileEntry,
        write: &mut W,
    ) -> io::Result<()>;
}

impl<R> CpioRead for R
where
    R: Read + Seek,
{
    fn read_cpio_entry(&mut self) -> io::Result<FileEntry> {
        FileEntry::read(self)
    }

    fn read_cpio_entry_payload<W: Write>(
        &mut self,
        entry: &FileEntry,
        writer: &mut W,
    ) -> io::Result<()> {
        let file_size = entry.file_size;
        io_copy_exact(self, writer, file_size)?;
        let position = align_n_bytes(entry.file_size, 4) + entry.file_size;
        self.seek(io::SeekFrom::Current(position.into()))?;
        Ok(())
    }
}

pub trait CpioWriter {
    fn write_cpio_entry(&mut self, entry: FileEntry) -> io::Result<()>;

    fn write_cpio_entry_payload<R: Read>(&mut self, reader: &mut R) -> io::Result<()>;

    fn write_cpio_file(&mut self, path: &PathBuf) -> io::Result<()> {
        let entry: FileEntry = path.try_into()?;
        self.write_cpio_entry(entry)?;
        let mut file = File::open(path)?;
        self.write_cpio_entry_payload(&mut file)
    }

    fn write_cpio_files(&mut self, paths: Vec<PathBuf>) -> io::Result<()> {
        for path in &paths {
            self.write_cpio_file(path)?
        }
        self.cpio_close()
    }

    fn write_cpio_record<R: Read>(&mut self, record: FileEntry, data: &mut R) -> io::Result<()> {
        self.write_cpio_entry(record)?;
        self.write_cpio_entry_payload(data)
    }

    fn write_cpio_records<R: Read>(&mut self, records: Vec<(FileEntry, &mut R)>) -> io::Result<()> {
        for (record, data) in records.into_iter() {
            self.write_cpio_record(record, data)?;
        }
        self.cpio_close()
    }

    fn cpio_close(&mut self) -> io::Result<()> {
        self.write_cpio_entry(FileEntry::default())
    }
}

impl<W> CpioWriter for W
where
    W: Write,
{
    fn write_cpio_entry(&mut self, entry: FileEntry) -> io::Result<()> {
        entry.write(self)
    }

    fn write_cpio_entry_payload<R: Read>(&mut self, reader: &mut R) -> io::Result<()> {
        let file_size = io::copy(reader, self)? as u32;
        let number = align_n_bytes(file_size, 4) as usize;
        let pad = vec![0_u8; number];
        self.write_all(&pad)
    }
}

pub struct CpioBuilder<W: Write> {
    writer: Option<W>,
    records: Vec<(FileEntry, Box<dyn Read>)>,
}

impl<W: Write + CpioWriter> CpioBuilder<W> {
    pub fn new(writer: W) -> Self {
        CpioBuilder {
            writer: Some(writer),
            records: Vec::new(),
        }
    }

    pub fn add_raw_file(mut self, path: &PathBuf) -> io::Result<Self> {
        let record: FileEntry = path.try_into()?;
        let reader = File::open(path)?;
        self.records.push((record, Box::new(reader)));
        Ok(self)
    }

    pub fn add_file(mut self, path: &str, as_path: &str) -> io::Result<Self> {
        let file = PathBuf::from(path);
        let mut record: FileEntry = (&file).try_into()?;
        record.name = as_path.to_owned();
        let reader = File::open(&file)?;
        self.records.push((record, Box::new(reader)));
        Ok(self)
    }

    pub fn build(self) -> io::Result<()> {
        match self {
            CpioBuilder {
                writer: Some(mut writer),
                records,
            } => {
                for (record, mut data) in records.into_iter() {
                    writer.write_cpio_record(record, &mut data)?;
                }
                writer.cpio_close()
            }
            _ => Err(io::Error::new(io::ErrorKind::Other, "Writer not found")),
        }
    }
}

impl CpioBuilder<File> {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let writer = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        Ok(CpioBuilder {
            writer: Some(writer),
            records: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpio_write_entry() -> io::Result<()> {
        let mut writer = Vec::new();
        writer.write_cpio_entry(FileEntry::default())?;
        let entry = FileEntry::read(&mut writer.as_slice())?;
        assert_eq!(entry, FileEntry::default());
        Ok(())
    }

    // Buffer size limit security tests
    #[test]
    fn test_cpio_rejects_oversized_file() {
        // Create CPIO data with file_size exceeding MAX_CPIO_ENTRY_SIZE
        let mut data = Vec::new();

        // Write magic
        data.extend_from_slice(b"070701");

        // Write header fields as hex (8 chars each)
        let write_hex = |data: &mut Vec<u8>, val: u32| {
            data.extend_from_slice(format!("{:08x}", val).as_bytes());
        };

        write_hex(&mut data, 0);  // ino
        write_hex(&mut data, 0);  // mode
        write_hex(&mut data, 0);  // uid
        write_hex(&mut data, 0);  // gid
        write_hex(&mut data, 0);  // nlink
        write_hex(&mut data, 0);  // mtime
        write_hex(&mut data, MAX_CPIO_ENTRY_SIZE + 1);  // file_size - OVERSIZED!

        let mut reader = std::io::Cursor::new(data);
        let result = FileEntry::read(&mut reader);

        assert!(result.is_err(), "Should reject oversized file");
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("file size"));
        assert!(err.to_string().contains("exceeds maximum"));
    }

    #[test]
    fn test_cpio_rejects_oversized_name() {
        // Create CPIO data with name_size exceeding MAX_NAME_SIZE
        let mut data = Vec::new();

        // Write magic
        data.extend_from_slice(b"070701");

        // Write header fields as hex
        let write_hex = |data: &mut Vec<u8>, val: u32| {
            data.extend_from_slice(format!("{:08x}", val).as_bytes());
        };

        write_hex(&mut data, 0);  // ino
        write_hex(&mut data, 0);  // mode
        write_hex(&mut data, 0);  // uid
        write_hex(&mut data, 0);  // gid
        write_hex(&mut data, 0);  // nlink
        write_hex(&mut data, 0);  // mtime
        write_hex(&mut data, 100);  // file_size - reasonable
        write_hex(&mut data, 0);  // dev_major
        write_hex(&mut data, 0);  // dev_minor
        write_hex(&mut data, 0);  // rdev_major
        write_hex(&mut data, 0);  // rdev_minor
        write_hex(&mut data, MAX_NAME_SIZE + 1);  // name_size - OVERSIZED!

        let mut reader = std::io::Cursor::new(data);
        let result = FileEntry::read(&mut reader);

        assert!(result.is_err(), "Should reject oversized name");
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("name size"));
        assert!(err.to_string().contains("exceeds maximum"));
    }

    #[test]
    fn test_cpio_accepts_size_at_limits() {
        // Test that sizes exactly at the limits are accepted
        // (This test validates the boundary condition)
        let mut data = Vec::new();

        data.extend_from_slice(b"070701");

        let write_hex = |data: &mut Vec<u8>, val: u32| {
            data.extend_from_slice(format!("{:08x}", val).as_bytes());
        };

        write_hex(&mut data, 0);  // ino
        write_hex(&mut data, 0);  // mode
        write_hex(&mut data, 0);  // uid
        write_hex(&mut data, 0);  // gid
        write_hex(&mut data, 0);  // nlink
        write_hex(&mut data, 0);  // mtime
        write_hex(&mut data, MAX_CPIO_ENTRY_SIZE);  // file_size - at limit
        write_hex(&mut data, 0);  // dev_major
        write_hex(&mut data, 0);  // dev_minor
        write_hex(&mut data, 0);  // rdev_major
        write_hex(&mut data, 0);  // rdev_minor
        write_hex(&mut data, MAX_NAME_SIZE);  // name_size - at limit
        data.extend_from_slice(&[0u8; 8]);  // checksum

        // Add name data (MAX_NAME_SIZE bytes)
        data.extend_from_slice(&vec![b'a'; MAX_NAME_SIZE as usize]);

        let mut reader = std::io::Cursor::new(data);
        let result = FileEntry::read(&mut reader);

        // Should not fail with size limit error
        if let Err(e) = result {
            assert_ne!(
                e.kind(),
                io::ErrorKind::InvalidData,
                "Should not reject size at limit: {}",
                e
            );
        }
    }

    // Path traversal security tests
    #[test]
    fn test_is_safe_path_rejects_parent_dir_components() {
        // Reject paths with ".." components
        assert!(!is_safe_path(Path::new("../../etc/passwd")));
        assert!(!is_safe_path(Path::new("foo/../../../etc/passwd")));
        assert!(!is_safe_path(Path::new("foo/bar/../../../etc/passwd")));
        assert!(!is_safe_path(Path::new("../etc/passwd")));
        assert!(!is_safe_path(Path::new("foo/..")));
    }

    #[test]
    fn test_is_safe_path_rejects_absolute_paths() {
        // Reject absolute paths
        assert!(!is_safe_path(Path::new("/etc/passwd")));
        assert!(!is_safe_path(Path::new("/tmp/test")));

        // On Windows, also reject paths like C:\
        #[cfg(windows)]
        {
            assert!(!is_safe_path(Path::new("C:\\Windows\\System32")));
        }
    }

    #[test]
    fn test_is_safe_path_accepts_valid_relative_paths() {
        // Accept valid relative paths
        assert!(is_safe_path(Path::new("file.txt")));
        assert!(is_safe_path(Path::new("dir/file.txt")));
        assert!(is_safe_path(Path::new("dir/subdir/file.txt")));
        assert!(is_safe_path(Path::new("./file.txt")));
        assert!(is_safe_path(Path::new("./dir/file.txt")));
    }

    #[test]
    fn test_is_safe_path_edge_cases() {
        // Edge cases
        assert!(is_safe_path(Path::new(".")));
        assert!(is_safe_path(Path::new("")));

        // Paths that look suspicious but are actually safe
        assert!(is_safe_path(Path::new("file..txt")));  // ".." in filename
        assert!(is_safe_path(Path::new("dir/file..txt")));
    }
}
