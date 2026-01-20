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
/// - Starts with a root directory component (cross-platform absolute path detection)
fn is_safe_path(path: &Path) -> bool {
    let has_traversal = path.components().any(|c| matches!(c, Component::ParentDir));
    let is_absolute = path.is_absolute();

    // On Windows, is_absolute() returns false for Unix-style paths like "/etc/passwd"
    // So we also check if the path starts with a root directory component
    let starts_with_root = matches!(path.components().next(), Some(Component::RootDir));

    !has_traversal && !is_absolute && !starts_with_root
}

/// Compute the expected canonical path without creating any filesystem entries.
/// This walks up the path tree to find an existing ancestor, canonicalizes it,
/// then joins the remaining path components.
///
/// Returns the expected canonical path and validates that all existing path
/// components that are symlinks resolve to locations within the base directory.
fn compute_safe_canonical_path(path: &Path, canonical_base: &Path) -> io::Result<PathBuf> {
    // Find the deepest existing ancestor
    let mut existing_ancestor = path.to_path_buf();
    let mut components_to_add: Vec<std::ffi::OsString> = Vec::new();

    while !existing_ancestor.exists() {
        if let Some(file_name) = existing_ancestor.file_name() {
            components_to_add.push(file_name.to_os_string());
        }
        if !existing_ancestor.pop() {
            // We've reached the root without finding an existing path
            // This shouldn't happen if base directory exists
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "No existing ancestor directory found",
            ));
        }
    }

    // Canonicalize the existing ancestor (this resolves symlinks)
    let canonical_ancestor = existing_ancestor.canonicalize().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Cannot canonicalize existing path: {}", e),
        )
    })?;

    // Verify the existing ancestor is within the base directory
    // This catches symlink attacks where an existing directory is a symlink
    // pointing outside the extraction directory
    if !canonical_ancestor.starts_with(canonical_base) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Path contains symlink escaping extraction directory",
        ));
    }

    // Build the expected canonical path by adding back the non-existing components
    let mut result = canonical_ancestor;
    for component in components_to_add.into_iter().rev() {
        result.push(component);
    }

    Ok(result)
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
                format!(
                    "CPIO entry file size {} exceeds maximum allowed size {}",
                    file_size, MAX_CPIO_ENTRY_SIZE
                ),
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
                format!(
                    "CPIO entry name size {} exceeds maximum allowed size {}",
                    name_size, MAX_NAME_SIZE
                ),
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

        // === PATH VALIDATION (before any filesystem modifications) ===

        // First check: reject paths with ".." components or absolute paths
        if !is_safe_path(Path::new(&entry.name)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Unsafe path in archive (potential path traversal): {}",
                    entry.name
                ),
            ));
        }

        // Canonicalize the extraction directory
        let canonical_dir = dir.canonicalize().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Cannot canonicalize extraction directory: {}", e),
            )
        })?;

        // Second check: compute expected canonical path and validate it stays within
        // the extraction directory. This also detects symlink attacks where existing
        // path components are symlinks pointing outside the extraction directory.
        let canonical_path = compute_safe_canonical_path(&path, &canonical_dir)?;

        if !canonical_path.starts_with(&canonical_dir) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Path escapes extraction directory: {}", entry.name),
            ));
        }

        // === FILESYSTEM OPERATIONS (only after validation passes) ===

        let mut number = 0;

        if entry.nlink == 2 {
            // Entry is a directory
            if !path.exists() {
                std::fs::create_dir_all(&path)?;
            }
        } else {
            // Entry is a file - ensure parent directory exists
            if let Some(parent) = path.parent()
                && !parent.exists()
            {
                if creates_dir {
                    std::fs::create_dir_all(parent)?;
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("Parent directory does not exist: {}", parent.display()),
                    ));
                }
            }

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

        write_hex(&mut data, 0); // ino
        write_hex(&mut data, 0); // mode
        write_hex(&mut data, 0); // uid
        write_hex(&mut data, 0); // gid
        write_hex(&mut data, 0); // nlink
        write_hex(&mut data, 0); // mtime
        write_hex(&mut data, MAX_CPIO_ENTRY_SIZE + 1); // file_size - OVERSIZED!

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

        write_hex(&mut data, 0); // ino
        write_hex(&mut data, 0); // mode
        write_hex(&mut data, 0); // uid
        write_hex(&mut data, 0); // gid
        write_hex(&mut data, 0); // nlink
        write_hex(&mut data, 0); // mtime
        write_hex(&mut data, 100); // file_size - reasonable
        write_hex(&mut data, 0); // dev_major
        write_hex(&mut data, 0); // dev_minor
        write_hex(&mut data, 0); // rdev_major
        write_hex(&mut data, 0); // rdev_minor
        write_hex(&mut data, MAX_NAME_SIZE + 1); // name_size - OVERSIZED!

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

        write_hex(&mut data, 0); // ino
        write_hex(&mut data, 0); // mode
        write_hex(&mut data, 0); // uid
        write_hex(&mut data, 0); // gid
        write_hex(&mut data, 0); // nlink
        write_hex(&mut data, 0); // mtime
        write_hex(&mut data, MAX_CPIO_ENTRY_SIZE); // file_size - at limit
        write_hex(&mut data, 0); // dev_major
        write_hex(&mut data, 0); // dev_minor
        write_hex(&mut data, 0); // rdev_major
        write_hex(&mut data, 0); // rdev_minor
        write_hex(&mut data, MAX_NAME_SIZE); // name_size - at limit
        data.extend_from_slice(&[0u8; 8]); // checksum

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
        // Edge cases: current directory and empty path are considered safe by is_safe_path
        // (empty paths will fail later during extraction when file_name() returns None)
        assert!(is_safe_path(Path::new(".")));
        assert!(is_safe_path(Path::new("")));

        // "file..txt" is safe - the ".." is part of the filename, not a ParentDir component
        assert!(is_safe_path(Path::new("file..txt")));
        assert!(is_safe_path(Path::new("dir/file..txt")));
    }

    #[test]
    fn test_compute_safe_canonical_path_valid_paths() {
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let base = temp_dir.path();
        let canonical_base = base.canonicalize().unwrap();

        // Create a subdirectory
        fs::create_dir(base.join("subdir")).unwrap();

        // Valid path within base directory
        let path = base.join("subdir/file.txt");
        let result = compute_safe_canonical_path(&path, &canonical_base);
        assert!(result.is_ok());
        assert!(result.unwrap().starts_with(&canonical_base));

        // Valid path in non-existing nested directory
        let path = base.join("subdir/nested/file.txt");
        let result = compute_safe_canonical_path(&path, &canonical_base);
        assert!(result.is_ok());
        assert!(result.unwrap().starts_with(&canonical_base));
    }

    #[cfg(unix)]
    #[test]
    fn test_compute_safe_canonical_path_detects_symlink_escape() {
        use std::os::unix::fs::symlink;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let base = temp_dir.path();
        let canonical_base = base.canonicalize().unwrap();

        // Create an external directory (outside base)
        let external_dir = tempdir().unwrap();

        // Create a symlink inside base that points outside
        let symlink_path = base.join("escape_link");
        symlink(external_dir.path(), &symlink_path).unwrap();

        // Attempt to access a file through the escaping symlink
        let path = base.join("escape_link/secret.txt");
        let result = compute_safe_canonical_path(&path, &canonical_base);

        // Should fail because symlink points outside base directory
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("symlink escaping"));
    }

    #[cfg(unix)]
    #[test]
    fn test_compute_safe_canonical_path_allows_internal_symlinks() {
        use std::fs;
        use std::os::unix::fs::symlink;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let base = temp_dir.path();
        let canonical_base = base.canonicalize().unwrap();

        // Create directories inside base
        fs::create_dir(base.join("real_dir")).unwrap();

        // Create a symlink that stays within base
        let symlink_path = base.join("internal_link");
        symlink(base.join("real_dir"), &symlink_path).unwrap();

        // Access through internal symlink should work
        let path = base.join("internal_link/file.txt");
        let result = compute_safe_canonical_path(&path, &canonical_base);

        assert!(result.is_ok());
        assert!(result.unwrap().starts_with(&canonical_base));
    }

    // Integration tests for extract_entry with malicious archives

    /// Helper to create a CPIO archive with a single file entry
    fn create_cpio_archive(name: &str, content: &[u8]) -> Vec<u8> {
        let mut archive = Vec::new();

        // Write the file entry
        let entry = FileEntry {
            name: name.to_string(),
            ino: 1,
            mode: 0o100644, // regular file
            uid: 1000,
            gid: 1000,
            nlink: 1,
            mtime: 0,
            file_size: content.len() as u32,
            dev_major: 0,
            dev_minor: 0,
            rdev_major: 0,
            rdev_minor: 0,
        };
        archive.write_cpio_entry(entry).unwrap();
        archive.write_all(content).unwrap();
        // Pad to 4-byte alignment
        let padding = align_n_bytes(content.len() as u32, 4) as usize;
        archive.write_all(&vec![0u8; padding]).unwrap();

        // Write trailer
        archive.write_cpio_entry(FileEntry::default()).unwrap();

        archive
    }

    #[test]
    fn test_extract_entry_rejects_path_traversal() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let extract_dir = temp_dir.path();

        // Create a malicious CPIO archive with path traversal
        let archive = create_cpio_archive("../../etc/passwd", b"malicious content");
        let mut reader = std::io::Cursor::new(archive);

        // Attempt to extract - should fail
        let result = extract_entry(&mut reader, extract_dir, true, false);

        assert!(result.is_err(), "Should reject path with '..' components");
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("path traversal"),
            "Error should mention path traversal: {}",
            err
        );

        // Verify no file was created outside the extraction directory
        assert!(!extract_dir.join("..").join("..").join("etc").exists());
    }

    #[test]
    fn test_extract_entry_rejects_absolute_path() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let extract_dir = temp_dir.path();

        // Create a malicious CPIO archive with absolute path
        let archive = create_cpio_archive("/etc/passwd", b"malicious content");
        let mut reader = std::io::Cursor::new(archive);

        // Attempt to extract - should fail
        let result = extract_entry(&mut reader, extract_dir, true, false);

        assert!(result.is_err(), "Should reject absolute path");
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("path traversal"),
            "Error should mention path traversal: {}",
            err
        );
    }

    #[test]
    fn test_extract_entry_rejects_complex_traversal() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let extract_dir = temp_dir.path();

        // Create a malicious CPIO archive with complex path traversal
        // This path has valid-looking prefix but escapes via multiple ..
        let archive = create_cpio_archive("foo/bar/../../../etc/passwd", b"malicious content");
        let mut reader = std::io::Cursor::new(archive);

        // Attempt to extract - should fail
        let result = extract_entry(&mut reader, extract_dir, true, false);

        assert!(result.is_err(), "Should reject complex path traversal");
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn test_extract_entry_accepts_valid_paths() {
        use std::fs;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let extract_dir = temp_dir.path();

        // Create a valid CPIO archive
        let archive = create_cpio_archive("subdir/file.txt", b"valid content");
        let mut reader = std::io::Cursor::new(archive);

        // Extract should succeed
        let result = extract_entry(&mut reader, extract_dir, true, false);
        assert!(result.is_ok(), "Should accept valid relative path");

        // Verify the file was created
        let extracted_file = extract_dir.join("subdir/file.txt");
        assert!(extracted_file.exists(), "File should be extracted");
        let content = fs::read_to_string(&extracted_file).unwrap();
        assert_eq!(content, "valid content");
    }

    #[cfg(unix)]
    #[test]
    fn test_extract_entry_rejects_symlink_escape() {
        use std::fs;
        use std::os::unix::fs::symlink;
        use tempfile::tempdir;

        // Create extraction directory and an external target
        let temp_dir = tempdir().unwrap();
        let extract_dir = temp_dir.path().join("extract");
        fs::create_dir(&extract_dir).unwrap();

        let external_dir = tempdir().unwrap();
        let external_file = external_dir.path().join("secret.txt");

        // Create a symlink inside extract_dir pointing outside
        let symlink_path = extract_dir.join("escape_link");
        symlink(external_dir.path(), &symlink_path).unwrap();

        // Create a CPIO archive trying to write through the symlink
        let archive = create_cpio_archive("escape_link/secret.txt", b"malicious content");
        let mut reader = std::io::Cursor::new(archive);

        // Attempt to extract - should fail because symlink escapes
        let result = extract_entry(&mut reader, &extract_dir, true, false);

        assert!(
            result.is_err(),
            "Should reject path through escaping symlink"
        );
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("symlink escaping"),
            "Error should mention symlink escape: {}",
            err
        );

        // Verify no file was created at the external location
        assert!(
            !external_file.exists(),
            "File should not be created outside extraction directory"
        );
    }
}
