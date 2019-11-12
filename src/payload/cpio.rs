use hex::FromHex;
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

const MAGIC: &[u8] = b"070701";

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
}

fn align_bytes(from: u32, n: u32) -> u32 {
    (n - from % n) % n
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
        if &entry.name == "TRAILER!!!" {
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
    let number = io::copy(reader, writer)?;
    let position = align_bytes(entry.file_size, 4);
    reader.seek(io::SeekFrom::Current(position.into()))?;
    Ok((entry, number))
}

pub fn extract_entry<R: Read + Seek>(
    reader: &mut R,
    dir: &PathBuf,
    creates_dir: bool,
) -> Result<(FileEntry, u64), io::Error> {
    let entry = FileEntry::read(reader)?;

    // write content to file only if it is not a last pseudo
    if &entry.name != "TRAILER!!!" {
        let path = dir.join(&entry.name);

        if entry.nlink == 2 {
            std::fs::create_dir_all(path)?;
            Ok((entry, 0))
        } else {
            if creates_dir {
                let parent = path.parent();
                if let Some(p) = parent {
                    std::fs::create_dir_all(p)?;
                }
            }

            let mut writer = OpenOptions::new().create(true).write(true).open(path)?;

            let metadata = writer.metadata()?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(entry.mode);
            writer.set_permissions(permissions)?;

            let number = io_copy_exact(reader, &mut writer, entry.file_size)?;
            let position = align_bytes(entry.file_size, 4);
            reader.seek(io::SeekFrom::Current(position.into()))?;
            Ok((entry, number.into()))
        }
    } else {
        Ok((entry, 0))
    }
}

pub fn extract_entries<R: Read + Seek>(
    reader: &mut R,
    dir: &PathBuf,
    creates_dir: bool,
) -> Result<Vec<FileEntry>, io::Error> {
    let mut entries = Vec::new();
    loop {
        let (entry, _) = extract_entry(reader, dir, creates_dir)?;
        if &entry.name == "TRAILER!!!" {
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
