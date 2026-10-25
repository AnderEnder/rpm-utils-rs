use filetime::{FileTime, set_file_mtime};
use std::collections::{HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::utils::{HexReader, HexWriter, align_n_bytes};

const MAGIC: &[u8] = b"070701";
const CRC_MAGIC: &[u8] = b"070702";
const TRAILER: &str = "TRAILER!!!";

/// Maximum allowed CPIO entry name size (4 KB) - prevents OOM attacks
const MAX_NAME_SIZE: u32 = 4096;

/// Check if a path is safe for extraction (no path traversal attempts)
///
/// Returns false if the path:
/// - Contains ".." components (path traversal)
/// - Is an absolute path (including Unix-style paths like "/etc" on Windows)
/// - Starts with a root directory component (cross-platform absolute path detection)
fn is_safe_path(path: &Path) -> bool {
    let has_traversal = path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)));
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

    while match std::fs::symlink_metadata(&existing_ancestor) {
        Ok(_) => false,
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        Err(error) => return Err(error),
    } {
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
    /// Sum of payload bytes for the SVR4 crc format; None for newc.
    pub checksum: Option<u32>,
}

impl FileEntry {
    pub fn read<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut magic = [0_u8; 6];
        reader.read_exact(&mut magic)?;

        if magic != MAGIC && magic != CRC_MAGIC {
            return Err(io::Error::other(format!(
                "Error: incorrect magic of cpio entry {:x?}",
                magic
            )));
        }

        let ino = reader.read_hex_as_u32()?;
        let mode = reader.read_hex_as_u32()?;
        let uid = reader.read_hex_as_u32()?;
        let gid = reader.read_hex_as_u32()?;
        let nlink = reader.read_hex_as_u32()?;
        let mtime = reader.read_hex_as_u32()?;
        let file_size = reader.read_hex_as_u32()?;

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

        let checksum_value = reader.read_hex_as_u32()?;
        let checksum = if magic == CRC_MAGIC {
            Some(checksum_value)
        } else {
            if checksum_value != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Nonzero newc checksum",
                ));
            }
            None
        };
        if file_size == 0 && checksum.is_some_and(|value| value != 0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid checksum for empty CPIO entry",
            ));
        }

        // optimise later
        let mut name_bytes = vec![0_u8; name_size as usize];
        reader.read_exact(&mut name_bytes)?;
        let end = name_bytes.iter().position(|b| *b == 0).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "CPIO name is not NUL terminated",
            )
        })?;
        if end == 0 || name_bytes[end..].iter().any(|b| *b != 0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid CPIO name",
            ));
        }
        let name = String::from_utf8(name_bytes[..end].to_vec())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if name == TRAILER && file_size != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "CPIO trailer has data",
            ));
        }

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
            checksum,
        })
    }

    fn write<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        if self.name.is_empty()
            || self.name.contains('\0')
            || self.name.len() >= MAX_NAME_SIZE as usize
            || (self.name == TRAILER && self.file_size != 0)
            || (self.file_size == 0 && self.checksum.is_some_and(|value| value != 0))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid CPIO entry",
            ));
        }
        writer.write_all(if self.checksum.is_some() {
            CRC_MAGIC
        } else {
            MAGIC
        })?;
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
        writer.write_u32_as_hex(self.checksum.unwrap_or(0))?;

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
            checksum: None,
        }
    }
}

impl TryFrom<&PathBuf> for FileEntry {
    type Error = io::Error;

    fn try_from(f: &PathBuf) -> Result<Self, Self::Error> {
        let meta = f.symlink_metadata()?;
        if !meta.is_file() && !meta.is_dir() && !meta.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Only regular files, directories and symbolic links can be archived",
            ));
        }
        let file_size = if meta.is_dir() {
            0
        } else {
            let length = if meta.file_type().is_symlink() {
                symlink_bytes(f)?.len() as u64
            } else {
                meta.len()
            };
            u32::try_from(length).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "CPIO file is too large")
            })?
        };
        let name = f
            .file_name()
            .ok_or_else(|| io::Error::other(format!("cannot find filename from path {:?}", f)))?
            .to_str()
            .ok_or_else(|| io::Error::other(format!("cannot parse path {:?} to string", f)))?
            .to_owned();

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Ok(FileEntry {
                name,
                ino: meta.ino() as u32,
                mode: meta.mode(),
                uid: meta.uid(),
                gid: meta.gid(),
                nlink: meta.nlink() as u32,
                mtime: u32::try_from(meta.mtime()).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "CPIO timestamp must fit u32")
                })?,
                file_size,
                dev_major: nix::libc::major(meta.dev() as nix::libc::dev_t) as u32,
                dev_minor: nix::libc::minor(meta.dev() as nix::libc::dev_t) as u32,
                rdev_major: nix::libc::major(meta.rdev() as nix::libc::dev_t) as u32,
                rdev_minor: nix::libc::minor(meta.rdev() as nix::libc::dev_t) as u32,
                checksum: None,
            })
        }
        #[cfg(windows)]
        {
            Ok(FileEntry {
                name,
                ino: 1,
                mode: if meta.is_dir() {
                    0o040755
                } else if meta.file_type().is_symlink() {
                    0o120777
                } else {
                    0o100644
                },
                uid: 0,
                gid: 0,
                nlink: 0,
                mtime: meta
                    .modified()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as u32,
                file_size,
                dev_major: 0,
                dev_minor: 0,
                rdev_major: 0,
                rdev_minor: 0,
                checksum: None,
            })
        }
    }
}

pub fn read_entries<R: Read>(reader: &mut R) -> io::Result<Vec<FileEntry>> {
    let mut entries = Vec::new();

    loop {
        let entry = FileEntry::read(reader)?;
        copy_entry_payload(reader, &mut io::sink(), &entry)?;
        io_copy_exact(reader, &mut io::sink(), align_n_bytes(entry.file_size, 4))?;
        if entry.name == TRAILER {
            break;
        }
        entries.push(entry);
    }
    Ok(entries)
}

pub fn read_entry<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> io::Result<(FileEntry, u64)> {
    let entry = FileEntry::read(reader)?;
    let number = copy_entry_payload(reader, writer, &entry)?;
    let position = align_n_bytes(entry.file_size, 4);
    io_copy_exact(reader, &mut io::sink(), position)?;
    Ok((entry, number.into()))
}

pub fn extract_entry<R: Read>(
    reader: &mut R,
    dir: &Path,
    creates_dir: bool,
    change_owner: bool,
) -> io::Result<(FileEntry, u64)> {
    let entry = FileEntry::read(reader)?;
    extract_entry_inner(
        reader,
        entry,
        dir,
        creates_dir,
        change_owner,
        false,
        &mut HashMap::new(),
    )
}

type LinkMap = HashMap<(u32, u32, u32), PathBuf>;

fn extract_entry_inner<R: Read>(
    reader: &mut R,
    entry: FileEntry,
    dir: &Path,
    creates_dir: bool,
    change_owner: bool,
    defer_directories: bool,
    links: &mut LinkMap,
) -> io::Result<(FileEntry, u64)> {
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

        let file_type = entry.mode & 0o170000;
        if !matches!(file_type, 0o040000 | 0o100000 | 0o120000) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Only regular files, directories and symbolic links can be extracted",
            ));
        }
        if file_type == 0o040000 {
            if entry.file_size != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "CPIO directory has data",
                ));
            }
            // Entry is a directory - create_dir_all is idempotent
            std::fs::create_dir_all(&path)?;
        } else {
            // Entry is a file - ensure parent directory exists
            if let Some(parent) = path.parent() {
                if creates_dir {
                    // create_dir_all is idempotent, no need to check existence first
                    std::fs::create_dir_all(parent)?;
                } else if !parent.exists() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("Parent directory does not exist: {}", parent.display()),
                    ));
                }
            }

            if file_type == 0o120000 {
                if entry.file_size == 0 || entry.file_size > MAX_NAME_SIZE {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid symlink target size",
                    ));
                }
                let mut target = Vec::new();
                copy_entry_payload(reader, &mut target, &entry)?;
                create_symlink(&target, &canonical_path)?;
                number = entry.file_size;
            } else {
                let key = (entry.dev_major, entry.dev_minor, entry.ino);
                let previous = if entry.nlink > 1 {
                    links.get(&key).cloned()
                } else {
                    None
                };
                if let Some(previous) = previous {
                    let previous = compute_safe_canonical_path(&previous, &canonical_dir)?;
                    if entry.file_size > 0 {
                        let mut options = OpenOptions::new();
                        options.write(true).truncate(true);
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::OpenOptionsExt;
                            options.custom_flags(nix::libc::O_NOFOLLOW);
                        }
                        let mut writer = options.open(&previous)?;
                        number = copy_entry_payload(reader, &mut writer, &entry)?;
                    }
                    // Refuse an existing destination rather than truncate an alias.
                    std::fs::hard_link(&previous, &canonical_path)?;
                } else {
                    let mut writer = open_regular_file(&canonical_path)?;
                    number = copy_entry_payload(reader, &mut writer, &entry)?;
                    if entry.nlink > 1 {
                        links.insert(key, canonical_path.clone());
                    }
                }
            }
            let position = align_n_bytes(entry.file_size, 4);
            io_copy_exact(reader, &mut io::sink(), position)?;
        }

        if !(defer_directories
            && (file_type == 0o040000 || (file_type == 0o100000 && entry.nlink > 1)))
        {
            restore_metadata(&canonical_path, &entry, change_owner)?;
        }
        Ok((entry, number.into()))
    } else {
        Ok((entry, 0))
    }
}

pub fn extract_entries<R: Read>(
    reader: &mut R,
    dir: &Path,
    creates_dir: bool,
    change_owner: bool,
) -> io::Result<Vec<FileEntry>> {
    let mut entries = Vec::new();
    let mut links = HashMap::new();
    let mut names = HashSet::new();
    loop {
        // Normalize names so aliases such as ./dir/file cannot overwrite a link
        // group member and silently destroy its data.
        let next = FileEntry::read(reader)?;
        let normalized: PathBuf = Path::new(&next.name)
            .components()
            .filter(|component| *component != Component::CurDir)
            .collect();
        if next.name != TRAILER && !names.insert(normalized) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Duplicate CPIO extraction path",
            ));
        }
        let (entry, _) = extract_entry_inner(
            reader,
            next,
            dir,
            creates_dir,
            change_owner,
            true,
            &mut links,
        )?;
        if entry.name == TRAILER {
            break;
        }
        entries.push(entry);
    }
    // A hard-link group's data can occur last. Keep the shared inode writable
    // until all of its members have been processed.
    for entry in entries
        .iter()
        .filter(|entry| entry.mode & 0o170000 == 0o100000 && entry.nlink > 1)
    {
        restore_metadata(&dir.join(&entry.name), entry, change_owner)?;
    }
    // Children must be created before restoring a read-only directory's mode and
    // mtime. Restore deeper directories first even for archives in unusual order.
    let mut directories: Vec<_> = entries
        .iter()
        .filter(|entry| entry.mode & 0o170000 == 0o040000)
        .collect();
    directories.sort_by_key(|entry| std::cmp::Reverse(Path::new(&entry.name).components().count()));
    for entry in directories {
        restore_metadata(&dir.join(&entry.name), entry, change_owner)?;
    }
    Ok(entries)
}

fn open_regular_file(path: &Path) -> io::Result<File> {
    // Do not truncate a pre-existing hard link: it may refer to data outside the
    // extraction root. Unlinking the directory entry leaves other aliases intact.
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            std::fs::remove_file(path)?;
        }
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Extraction destination is not a regular file",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    OpenOptions::new().create_new(true).write(true).open(path)
}

fn restore_metadata(path: &Path, entry: &FileEntry, change_owner: bool) -> io::Result<()> {
    let symlink = entry.mode & 0o170000 == 0o120000;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if change_owner {
            use nix::fcntl::AtFlags;
            use nix::unistd::{Gid, Uid, fchownat};
            fchownat(
                nix::fcntl::AT_FDCWD,
                path,
                Some(Uid::from_raw(entry.uid)),
                Some(Gid::from_raw(entry.gid)),
                AtFlags::AT_SYMLINK_NOFOLLOW,
            )
            .map_err(io::Error::from)?;
        }
        if !symlink {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(entry.mode & 0o7777))?;
        }
    }
    let mtime = FileTime::from_unix_time(entry.mtime.into(), 0);
    if symlink {
        filetime::set_symlink_file_times(path, mtime, mtime)
    } else {
        set_file_mtime(path, mtime)
    }
}

fn symlink_bytes(path: &Path) -> io::Result<Vec<u8>> {
    let target = std::fs::read_link(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(target.as_os_str().as_bytes().to_vec())
    }
    #[cfg(not(unix))]
    {
        Ok(target.to_string_lossy().as_bytes().to_vec())
    }
}

fn create_symlink(target: &[u8], path: &Path) -> io::Result<()> {
    if target.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "NUL in symlink target",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(target), path)
    }
    #[cfg(not(unix))]
    {
        let _ = (target, path);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Symlink extraction is only supported on Unix",
        ))
    }
}

pub(crate) fn entry_reader(path: &Path, entry: &FileEntry) -> io::Result<Box<dyn Read>> {
    match entry.mode & 0o170000 {
        0o040000 => Ok(Box::new(io::empty())),
        0o120000 => Ok(Box::new(io::Cursor::new(symlink_bytes(path)?))),
        _ => Ok(Box::new(File::open(path)?)),
    }
}

struct ChecksumReader<R> {
    inner: R,
    checksum: u32,
    enabled: bool,
}
impl<R: Read> Read for ChecksumReader<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(bytes)?;
        if self.enabled {
            self.checksum = bytes[..count].iter().fold(self.checksum, |sum, value| {
                sum.wrapping_add(u32::from(*value))
            });
        }
        Ok(count)
    }
}
fn copy_entry_payload<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    entry: &FileEntry,
) -> io::Result<u32> {
    let mut checked = ChecksumReader {
        inner: reader,
        checksum: 0,
        enabled: entry.checksum.is_some(),
    };
    let count = io_copy_exact(&mut checked, writer, entry.file_size)?;
    if entry
        .checksum
        .is_some_and(|expected| expected != checked.checksum)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "CPIO checksum mismatch",
        ));
    }
    Ok(count)
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
    R: Read,
{
    fn read_cpio_entry(&mut self) -> io::Result<FileEntry> {
        FileEntry::read(self)
    }

    fn read_cpio_entry_payload<W: Write>(
        &mut self,
        entry: &FileEntry,
        writer: &mut W,
    ) -> io::Result<()> {
        copy_entry_payload(self, writer, entry)?;
        let position = align_n_bytes(entry.file_size, 4);
        io_copy_exact(self, &mut io::sink(), position)?;
        Ok(())
    }
}

pub trait CpioWriter {
    fn write_cpio_entry(&mut self, entry: FileEntry) -> io::Result<()>;

    fn write_cpio_entry_payload<R: Read>(&mut self, reader: &mut R) -> io::Result<()>;

    fn write_cpio_file(&mut self, path: &PathBuf) -> io::Result<()> {
        let entry: FileEntry = path.try_into()?;
        let mut reader = entry_reader(path, &entry)?;
        self.write_cpio_record(entry, &mut reader)
    }

    fn write_cpio_files(&mut self, paths: Vec<PathBuf>) -> io::Result<()> {
        for path in &paths {
            self.write_cpio_file(path)?
        }
        self.cpio_close()
    }

    fn write_cpio_record<R: Read>(&mut self, record: FileEntry, data: &mut R) -> io::Result<()> {
        let size = record.file_size;
        let checksum = record.checksum;
        self.write_cpio_entry(record)?;
        let mut limited = data.take(u64::from(size));
        let mut checked = ChecksumReader {
            inner: &mut limited,
            checksum: 0,
            enabled: checksum.is_some(),
        };
        self.write_cpio_entry_payload(&mut checked)?;
        let actual = checked.checksum;
        if limited.limit() != 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Truncated CPIO payload",
            ));
        }
        if checksum.is_some_and(|expected| expected != actual) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "CPIO checksum mismatch",
            ));
        }
        Ok(())
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
        let file_size = io::copy(reader, self)?;
        let number = ((4 - file_size % 4) % 4) as usize;
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
        let reader = entry_reader(path, &record)?;
        self.records.push((record, reader));
        Ok(self)
    }

    pub fn add_file(mut self, path: &str, as_path: &str) -> io::Result<Self> {
        let file = PathBuf::from(path);
        let mut record: FileEntry = (&file).try_into()?;
        record.name = as_path.to_owned();
        let reader = entry_reader(&file, &record)?;
        self.records.push((record, reader));
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
            _ => Err(io::Error::other("Writer not found")),
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
    fn test_cpio_accepts_full_32_bit_file_sizes_without_allocating_payload() {
        let mut bytes = Vec::new();
        bytes
            .write_cpio_entry(FileEntry {
                name: "large".into(),
                mode: 0o100644,
                file_size: u32::MAX,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            FileEntry::read(&mut bytes.as_slice()).unwrap().file_size,
            u32::MAX
        );
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
        write_hex(&mut data, u32::MAX); // file_size - at limit
        write_hex(&mut data, 0); // dev_major
        write_hex(&mut data, 0); // dev_minor
        write_hex(&mut data, 0); // rdev_major
        write_hex(&mut data, 0); // rdev_minor
        write_hex(&mut data, MAX_NAME_SIZE); // name_size - at limit
        data.extend_from_slice(b"00000000"); // checksum

        // Add name data (MAX_NAME_SIZE bytes)
        data.extend_from_slice(&vec![b'a'; MAX_NAME_SIZE as usize - 1]);
        data.push(0);
        data.extend_from_slice(&[0; 2]);

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
            checksum: None,
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
