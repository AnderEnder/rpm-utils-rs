pub mod header;
pub mod signature;

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek};
use std::path::Path;

use header::{Index, Type};

const MAGIC: [u8; 4] = [237, 171, 238, 219];
const MAGIC_HEADER: [u8; 4] = [142, 173, 232, 1];

pub struct RawLead {
    pub magic: [u8; 4],
    pub major: u8,
    pub minor: u8,
    pub rpm_type: i16,
    pub archnum: i16,
    pub name: [u8; 66],
    pub osnum: i16,
    pub signature_type: i16,
    pub reserved: [u8; 16],
}

impl RawLead {
    pub fn read<R: Read + Seek>(fh: &mut R) -> Result<Self, io::Error> {
        fh.seek(io::SeekFrom::Start(0))?;
        let mut magic = [0_u8; 4];
        fh.read_exact(&mut magic)?;

        if magic != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Error: File is not rpm",
            ));
        }

        let mut head = [0_u8; 2];
        fh.read_exact(&mut head)?;
        let [major, minor] = head;

        let mut rpm_type_be = [0_u8; 2];
        fh.read_exact(&mut rpm_type_be)?;
        let rpm_type = i16::from_be_bytes(rpm_type_be);
        let mut archnum_be = [0_u8; 2];
        fh.read_exact(&mut archnum_be)?;
        let archnum = i16::from_be_bytes(archnum_be);

        let mut name = [0_u8; 66];
        fh.read_exact(&mut name)?;
        let mut osnum_be = [0_u8; 2];
        fh.read_exact(&mut osnum_be)?;
        let osnum = i16::from_be_bytes(osnum_be);
        let signature_type_be = [0_u8; 2];
        fh.read_exact(&mut osnum_be)?;
        let signature_type = i16::from_be_bytes(signature_type_be);
        let mut reserved = [0_u8; 16];
        fh.read_exact(&mut reserved)?;

        Ok(Self {
            magic,
            major,
            minor,
            rpm_type,
            archnum,
            name,
            osnum,
            signature_type,
            reserved,
        })
    }
}

impl fmt::Display for RawLead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "magic: {:?}", self.magic)?;
        writeln!(f, "major: {}", self.major)?;
        writeln!(f, "minor: {}", self.minor)?;
        writeln!(f, "rpm_type: {}", self.rpm_type)?;
        writeln!(f, "archnum: {}", self.archnum)?;
        writeln!(f, "name: {}", parse_string(&self.name))?;
        writeln!(f, "osnum: {}", self.osnum)?;
        writeln!(f, "signature_type: {}", self.signature_type)?;
        writeln!(f, "reserved: {}", parse_string(&self.reserved))?;
        Ok(())
    }
}

impl fmt::Debug for RawLead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "magic: {:?}", self.magic)?;
        writeln!(f, "major: {}", self.major)?;
        writeln!(f, "minor: {}", self.minor)?;
        writeln!(f, "rpm_type: {}", self.rpm_type)?;
        writeln!(f, "archnum: {}", self.archnum)?;
        writeln!(f, "name: {:?}", &&self.name[..])?;
        writeln!(f, "osnum: {}", self.osnum)?;
        writeln!(f, "signature_type: {}", self.signature_type)?;
        writeln!(f, "reserved: {:?}", self.reserved)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct RawHeader {
    pub magic: [u8; 4],
    pub reserved: [u8; 4],
    pub nindex: i32,
    pub hsize: i32,
}

impl RawHeader {
    pub fn read<R: Read + Seek>(fh: &mut R) -> Result<Self, io::Error> {
        let mut magic = [0_u8; 4];
        fh.read_exact(&mut magic)?;

        if magic != MAGIC_HEADER {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Error: File is not rpm",
            ));
        }

        let mut reserved = [0_u8; 4];
        fh.read_exact(&mut reserved)?;

        let mut nindex_be = [0_u8; 4];
        fh.read_exact(&mut nindex_be)?;
        let nindex = i32::from_be_bytes(nindex_be);

        let mut hsize_be = [0_u8; 4];
        fh.read_exact(&mut hsize_be)?;
        let hsize = i32::from_be_bytes(hsize_be);

        Ok(RawHeader {
            magic,
            reserved,
            nindex,
            hsize,
        })
    }
}

#[derive(Debug)]
pub struct RPMFile {
    pub lead: RawLead,
    pub signature: RawHeader,
    pub indexes: Vec<Index>,
    pub header: RawHeader,
    pub h_indexes: Vec<Index>,
    pub file: File,
}

impl RPMFile {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, io::Error> {
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        let lead = RawLead::read(&mut file)?;

        let signature = RawHeader::read(&mut file)?;

        let mut indexes = Vec::with_capacity(signature.nindex as usize);
        for _ in 0..signature.nindex {
            let index = Index::read(&mut file)?;
            indexes.push(index);
        }

        // aligning to 8 bytes and move after index payload
        let pos = 8 * (signature.hsize / 8 + if signature.hsize % 8 != 0 { 1 } else { 0 });
        file.seek(io::SeekFrom::Current(pos.into()))?;

        let header = RawHeader::read(&mut file)?;

        let mut h_indexes = Vec::with_capacity(signature.nindex as usize);
        for _ in 0..header.nindex {
            let index = Index::read(&mut file)?;
            h_indexes.push(index);
        }

        h_indexes.sort_by_key(|k| k.offset);

        let mut data = vec![0_u8; header.hsize as usize];
        file.read_exact(&mut data)?;
        println!("Bytes: {:?}", data);
        for i in 0..h_indexes.len() {
            let item = &h_indexes[i];
            println!(
                "Name: {:?}, Type: {:?}, Offset: {}, Count: {}",
                item.tag, item.itype, item.offset, item.count
            );
            match item.itype {
                Type::Int8 => {
                    let v = i8::from_be_bytes([data[item.offset as usize]; 1]);
                    println!("Value: {}", v);
                }
                Type::Int16 => {
                    let ps = item.offset as usize;
                    let s: [u8; 2] = [data[ps], data[ps + 1]];
                    let v = i16::from_be_bytes(s);
                    println!("Value: {}", v);
                }
                Type::Int32 => {
                    let ps = item.offset as usize;
                    let s: [u8; 4] = [data[ps], data[ps + 1], data[ps + 2], data[ps + 3]];
                    let v = i32::from_be_bytes(s);
                    println!("Value: {}", v);
                }

                Type::String => {
                    let ps = item.offset as usize;
                    let ps2 = h_indexes[i + 1].offset as usize;
                    let bytes = &data[ps..ps2];
                    println!("Values: {:?}", bytes);
                    println!("String parse: {:?}", parse_string(bytes));
                }

                Type::StringArray => {
                    let ps = item.offset as usize;
                    let ps2 = h_indexes[i + 1].offset as usize;
                    let bytes = &data[ps..ps2];
                    println!("Values: {:?}", bytes);
                    println!("String parse: {:?}", parse_strings(bytes));
                }
                Type::Bin => {
                    let ps = item.offset as usize;
                    let ps2 = ps + item.count as usize;
                    let bytes = &data[ps..ps2];
                    println!("Values: {:?}", bytes);
                }
                _ => {}
            }
        }

        Ok(Self {
            lead,
            signature,
            indexes,
            header,
            h_indexes,
            file,
        })
    }
}

fn debug_some<R: Read + Seek>(file: &mut R) -> Result<(), io::Error> {
    let mut debug = [0_u8; 32];
    file.read_exact(&mut debug)?;
    println!("Bytes: {:?}", debug);
    Ok(())
}

fn parse_string(bytes: &[u8]) -> String {
    let position = bytes.iter().position(|&x| x == 0).unwrap_or(0);
    let bytes2 = &bytes[0..position];
    String::from_utf8_lossy(bytes2).to_string()
}

fn parse_strings(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|x| *x == 0)
        .filter(|x| x.len() != 0)
        .map(|b| String::from_utf8_lossy(b).to_string())
        .collect()
}
