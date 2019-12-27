use filetime::{set_file_mtime, FileTime};
use std::convert::{TryFrom, TryInto};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};

use crate::utils::{align_n_bytes, HexReader, HexWriter};

const MAGIC: &[u8] = b"070701";
const TRAILER: &str = "TRAILER!!!";

#[derive(Debug)]
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
    pub fn read<R: Read + Seek>(reader: &mut R) -> Result<Self, io::Error> {
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
        let size = (name_size - 1) as usize;
        let name =
            String::from_utf8(name_bytes[0..size].to_vec()).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Error: incorrect utf8 symbol: {}", e),
                )
            })?;

        // aligning to 4 bytes: name +
        let position = align_n_bytes(name_size + 6, 4);
        reader.seek(io::SeekFrom::Current(position.into()))?;

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
    ((x >> 8) & 0x7F)
}

fn minor(x: u32) -> u32 {
    (x & 0xFF)
}

pub fn read_entries<R: Read + Seek>(reader: &mut R) -> Result<Vec<FileEntry>, io::Error> {
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
) -> Result<(FileEntry, u64), io::Error> {
    let entry = FileEntry::read(reader)?;
    let number = io_copy_exact(reader, writer, entry.file_size)?;
    let position = align_n_bytes(entry.file_size, 4);
    reader.seek(io::SeekFrom::Current(position.into()))?;
    Ok((entry, number.into()))
}

pub fn extract_entry<R: Read + Seek>(
    reader: &mut R,
    dir: &PathBuf,
    creates_dir: bool,
    change_owner: bool,
) -> Result<(FileEntry, u64), io::Error> {
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

            let mut writer = OpenOptions::new().create(true).write(true).open(&path)?;
            number = io_copy_exact(reader, &mut writer, entry.file_size)?;

            let position = align_n_bytes(entry.file_size, 4);
            reader.seek(io::SeekFrom::Current(position.into()))?;
        }

        #[cfg(all(unix))]
        {
            if change_owner {
                use nix::unistd::{chown, Gid, Uid};
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
    dir: &PathBuf,
    creates_dir: bool,
    change_owner: bool,
) -> Result<Vec<FileEntry>, io::Error> {
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

fn io_copy_exact<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    count: u32,
) -> Result<u32, io::Error> {
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

pub trait CpioWriter {
    fn write_cpio_entry(&mut self, entry: FileEntry) -> Result<(), io::Error>;

    fn write_cpio_entry_payload<R: Read>(&mut self, reader: &mut R) -> Result<(), io::Error>;

    fn write_cpio_file(&mut self, path: &PathBuf) -> Result<(), io::Error> {
        let entry: FileEntry = path.try_into()?;
        self.write_cpio_entry(entry)?;
        let mut file = File::open(path)?;
        self.write_cpio_entry_payload(&mut file)
    }

    fn write_cpio_files(&mut self, paths: Vec<PathBuf>) -> Result<(), io::Error> {
        for path in &paths {
            self.write_cpio_file(path)?
        }
        self.cpio_close()
    }

    fn write_cpio_record<R: Read>(
        &mut self,
        record: FileEntry,
        data: &mut R,
    ) -> Result<(), io::Error> {
        self.write_cpio_entry(record)?;
        self.write_cpio_entry_payload(data)
    }

    fn write_cpio_records<R: Read>(
        &mut self,
        records: Vec<(FileEntry, &mut R)>,
    ) -> Result<(), io::Error> {
        for (record, data) in records.into_iter() {
            self.write_cpio_record(record, data)?;
        }
        self.cpio_close()
    }

    fn cpio_close(&mut self) -> Result<(), io::Error> {
        self.write_cpio_entry(FileEntry::default())
    }
}

impl<W> CpioWriter for W
where
    W: Write,
{
    fn write_cpio_entry(&mut self, entry: FileEntry) -> Result<(), io::Error> {
        self.write_all(MAGIC)?;
        self.write_u32_as_hex(entry.ino)?;
        self.write_u32_as_hex(entry.ino)?;
        self.write_u32_as_hex(entry.mode)?;
        self.write_u32_as_hex(entry.uid)?;
        self.write_u32_as_hex(entry.gid)?;
        self.write_u32_as_hex(entry.nlink)?;
        self.write_u32_as_hex(entry.mtime)?;
        self.write_u32_as_hex(entry.file_size)?;
        self.write_u32_as_hex(entry.dev_major)?;
        self.write_u32_as_hex(entry.dev_minor)?;
        self.write_u32_as_hex(entry.rdev_major)?;
        self.write_u32_as_hex(entry.rdev_minor)?;
        self.write_u32_as_hex(entry.name.len() as u32)?;
        self.write_all(&[0_u8; 8])?;

        let mut name = entry.name.as_bytes().to_vec();
        name.push(0_u8);
        self.write_all(&name)?;

        // aligning to 4 bytes
        let position = align_n_bytes(entry.name.len() as u32 + 6, 4) as u8;
        let pad = vec![0_u8, position];
        self.write_all(&pad)?;

        Ok(())
    }

    fn write_cpio_entry_payload<R: Read>(&mut self, reader: &mut R) -> Result<(), io::Error> {
        io::copy(reader, self)?;
        Ok(())
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

    pub fn add_raw_file(mut self, path: &PathBuf) -> Result<Self, io::Error> {
        let record: FileEntry = path.try_into()?;
        let reader = File::open(path)?;
        self.records.push((record, Box::new(reader)));
        Ok(self)
    }

    pub fn add_file(mut self, path: &str, as_path: &str) -> Result<Self, io::Error> {
        let file = PathBuf::from(path);
        let mut record: FileEntry = (&file).try_into()?;
        record.name = as_path.to_owned();
        let reader = File::open(&file)?;
        self.records.push((record, Box::new(reader)));
        Ok(self)
    }

    pub fn build(self) -> Result<(), io::Error> {
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
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, io::Error> {
        let writer = OpenOptions::new().create(true).write(true).open(path)?;
        Ok(CpioBuilder {
            writer: Some(writer),
            records: Vec::new(),
        })
    }
}
