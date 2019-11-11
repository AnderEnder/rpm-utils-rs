use hex::FromHex;
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, Write};
use std::path::PathBuf;

const MAGIC: &[u8] = b"070701";

#[derive(Debug, Default)]
pub struct FileEntry {
    name: String,
    ino: u32,
    mode: u32,
    uid: u32,
    gid: u32,
    nlink: u32,
    mtime: u32,
    file_size: u32,
    dev_major: u32,
    dev_minor: u32,
    rdev_major: u32,
    rdev_minor: u32,
}

impl FileEntry {
    pub fn read<R: Read + Seek>(fh: &mut R) -> Result<Self, io::Error> {
        let mut magic = [0_u8; 6];
        fh.read_exact(&mut magic)?;

        if magic != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Error: incorrect magic of cpio entry {:x?}", magic),
            ));
        }

        let ino = u32_from_hex(fh)?;
        let mode = u32_from_hex(fh)?;
        let uid = u32_from_hex(fh)?;
        let gid = u32_from_hex(fh)?;
        let nlink = u32_from_hex(fh)?;
        let mtime = u32_from_hex(fh)?;
        let file_size = u32_from_hex(fh)?;
        let dev_major = u32_from_hex(fh)?;
        let dev_minor = u32_from_hex(fh)?;
        let rdev_major = u32_from_hex(fh)?;
        let rdev_minor = u32_from_hex(fh)?;
        let name_size = u32_from_hex(fh)?;
        let mut checksum = [0_u8; 8];
        fh.read_exact(&mut checksum)?;

        // optimise later
        let mut name_bytes = vec![0_u8; name_size as usize];
        fh.read_exact(&mut name_bytes)?;
        let name = parse_string(&name_bytes);

        // aligning to 4 bytes: name +
        let pos = align_bytes(name_size + 6, 4);
        fh.seek(io::SeekFrom::Current(pos.into()))?;

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

fn parse_string(bytes: &[u8]) -> String {
    let position = bytes.iter().position(|&x| x == 0).unwrap_or(0);
    let bytes2 = &bytes[0..position];
    String::from_utf8_lossy(bytes2).to_string()
}

fn u32_from_hex<R: Read + Seek>(fh: &mut R) -> Result<u32, io::Error> {
    let mut raw_bytes = [0_u8; 8];
    fh.read_exact(&mut raw_bytes)?;

    let v =
        Vec::from_hex(raw_bytes).map_err(|_| io::Error::new(io::ErrorKind::Other, "Error: "))?;

    let bytes = [v[0], v[1], v[2], v[3]];
    Ok(u32::from_be_bytes(bytes))
}

pub fn cpio_read_entries<R: Read + Seek>(fh: &mut R) -> Result<Vec<FileEntry>, io::Error> {
    let mut entries = Vec::new();

    loop {
        let entry = FileEntry::read(fh)?;
        let pos = align_bytes(entry.file_size, 4) + entry.file_size;
        fh.seek(io::SeekFrom::Current(pos.into()))?;
        if &entry.name == "TRAILER!!!" {
            break;
        }
        entries.push(entry);
    }
    Ok(entries)
}

pub fn cpio_read_entry<R: Read + Seek, W: Write>(
    fh: &mut R,
    wfh: &mut W,
) -> Result<(FileEntry, u64), io::Error> {
    let entry = FileEntry::read(fh)?;
    let number = io::copy(fh, wfh)?;
    let pos = align_bytes(entry.file_size, 4);
    fh.seek(io::SeekFrom::Current(pos.into()))?;
    Ok((entry, number))
}

pub fn cpio_extract_entry<R: Read + Seek, W: Write>(
    reader: &mut R,
    dir: &PathBuf,
) -> Result<u64, io::Error> {
    let entry = FileEntry::read(reader)?;
    let path = dir.join(entry.name);
    let mut writer = OpenOptions::new().create(true).write(true).open(path)?;
    let number = io::copy(reader, &mut writer)?;
    let pos = align_bytes(entry.file_size, 4);
    reader.seek(io::SeekFrom::Current(pos.into()))?;
    Ok(number)
}
