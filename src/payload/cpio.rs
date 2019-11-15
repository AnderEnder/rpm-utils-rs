use filetime::{set_file_mtime, FileTime};
use hex::FromHex;
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, Write};
use std::path::PathBuf;

const MAGIC: &[u8] = b"070701";
const TRAILER: &str = "TRAILER!!!";

#[derive(Debug, Default)]
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

        let ino = u32_from_hex(reader)?;
        let mode = u32_from_hex(reader)?;
        let uid = u32_from_hex(reader)?;
        let gid = u32_from_hex(reader)?;
        let nlink = u32_from_hex(reader)?;
        let mtime = u32_from_hex(reader)?;
        let file_size = u32_from_hex(reader)?;
        let dev_major = u32_from_hex(reader)?;
        let dev_minor = u32_from_hex(reader)?;
        let rdev_major = u32_from_hex(reader)?;
        let rdev_minor = u32_from_hex(reader)?;
        let name_size = u32_from_hex(reader)?;
        let mut checksum = [0_u8; 8];
        reader.read_exact(&mut checksum)?;

        // optimise later
        let mut name_bytes = vec![0_u8; name_size as usize];
        reader.read_exact(&mut name_bytes)?;
        let name =
            String::from_utf8(name_bytes[0..(name_size - 1) as usize].to_vec()).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Error: incorrect utf8 symbol: {}", e),
                )
            })?;

        // aligning to 4 bytes: name +
        let position = align_bytes(name_size + 6, 4);
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

    pub fn write<W: Write>(
        writer: &mut W,
        entry: FileEntry,
        file: &PathBuf,
    ) -> Result<(), io::Error> {
        let mut magic = [0_u8; 6];
        writer.write_all(MAGIC)?;
        u32_to_hex(writer, entry.ino)?;
        u32_to_hex(writer, entry.ino)?;
        u32_to_hex(writer, entry.mode)?;
        u32_to_hex(writer, entry.uid)?;
        u32_to_hex(writer, entry.gid)?;
        u32_to_hex(writer, entry.nlink)?;
        u32_to_hex(writer, entry.mtime)?;
        u32_to_hex(writer, entry.file_size)?;
        u32_to_hex(writer, entry.dev_major)?;
        u32_to_hex(writer, entry.dev_minor)?;
        u32_to_hex(writer, entry.rdev_major)?;
        u32_to_hex(writer, entry.rdev_minor)?;
        u32_to_hex(writer, entry.name.len() as u32)?;
        writer.write_all(&[0_u8; 8])?;

        let mut name = entry.name.as_bytes().to_vec();
        name.push(0_u8);
        writer.write_all(&name)?;

        // aligning to 4 bytes
        let position = align_bytes(entry.name.len() as u32 + 6, 4) as u8;
        let pad = vec![0_u8, position];
        writer.write_all(&pad)?;

        Ok(())
    }
}

impl From<&PathBuf> for FileEntry {
    fn from(f: &PathBuf) -> Self {
        let meta = f.metadata().unwrap();
        let name = f.file_name().unwrap().to_str().unwrap().to_owned();
        #[cfg(all(unix))]
        {
            use std::os::unix::fs::MetadataExt;
            FileEntry {
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
            }
        }
        #[cfg(all(windows))]
        {
            // TODO: reimplement properly for Windows
            use std::os::windows::fs::MetadataExt;
            FileEntry {
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
            }
        }
    }
}

fn major(x: u32) -> u32 {
    ((x >> 8) & 0x7F)
}

fn minor(x: u32) -> u32 {
    (x & 0xFF)
}

fn align_bytes(from: u32, n: u32) -> u32 {
    (n - from % n) % n
}

fn u32_to_hex<W: Write>(writer: &mut W, from: u32) -> Result<(), io::Error> {
    writer.write_all(format!("{:x}", from).as_bytes())?;
    Ok(())
}

fn u32_from_hex<R: Read + Seek>(reader: &mut R) -> Result<u32, io::Error> {
    let mut raw_bytes = [0_u8; 8];
    reader.read_exact(&mut raw_bytes)?;

    let v = Vec::from_hex(raw_bytes).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Error: can not parse hex {}", e),
        )
    })?;

    let bytes = [v[0], v[1], v[2], v[3]];
    Ok(u32::from_be_bytes(bytes))
}

pub fn read_entries<R: Read + Seek>(reader: &mut R) -> Result<Vec<FileEntry>, io::Error> {
    let mut entries = Vec::new();

    loop {
        let entry = FileEntry::read(reader)?;
        let position = align_bytes(entry.file_size, 4) + entry.file_size;
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
    let position = align_bytes(entry.file_size, 4);
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

            let position = align_bytes(entry.file_size, 4);
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
            println!("Extracting {}", &entry.name);
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
        let position = align_bytes(entry.file_size, 4) + entry.file_size;
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

struct CpioWrite<T> {
    writer: T,
}

impl<T: Read + Seek> CpioWrite<T> {
    pub fn new(writer: T) -> Self {
        CpioWrite { writer }
    }

    pub fn write(file: &PathBuf) -> Result<(), io::Error> {
        let entry: FileEntry = file.into();

        Ok(())
    }
}
