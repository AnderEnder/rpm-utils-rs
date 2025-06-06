use filetime::{FileTime, set_file_mtime};
use std::convert::{TryFrom, TryInto};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};

use crate::utils::{HexReader, HexWriter, align_n_bytes};

const MAGIC: &[u8] = b"070701";
const TRAILER: &str = "TRAILER!!!";

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
        let dev_major = reader.read_hex_as_u32()?;
        let dev_minor = reader.read_hex_as_u32()?;
        let rdev_major = reader.read_hex_as_u32()?;
        let rdev_minor = reader.read_hex_as_u32()?;
        let name_size = reader.read_hex_as_u32()?;
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
        let mut number = 0;

        if entry.nlink == 2 {
            std::fs::create_dir_all(&path)?;
        } else {
            if creates_dir {
                let parent = path.parent();
                if let Some(p) = parent {
                    std::fs::create_dir_all(p)?;
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
}
