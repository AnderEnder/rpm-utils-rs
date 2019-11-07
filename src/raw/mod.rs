use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};
use std::io::{self, Read, Seek};
use strum_macros::Display;
use std::fmt;

pub const MAGIC: [u8; 4] = [237, 171, 238, 219];
pub const MAGIC_HEADER: [u8; 4] = [142, 173, 232, 1];

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Display)]
pub enum Type {
    Binary = 0,
    Source = 1,
}

pub struct RawLead {
    pub magic: [u8; 4],
    pub major: u8,
    pub minor: u8,
    pub rpm_type: Type,
    pub archnum: u16,
    pub name: [u8; 66],
    pub osnum: u16,
    pub signature_type: u16,
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
        let rpm_type = Type::from_u16(u16::from_be_bytes(rpm_type_be)).unwrap();
        let mut archnum_be = [0_u8; 2];
        fh.read_exact(&mut archnum_be)?;
        let archnum = u16::from_be_bytes(archnum_be);

        let mut name = [0_u8; 66];
        fh.read_exact(&mut name)?;
        let mut osnum_be = [0_u8; 2];
        fh.read_exact(&mut osnum_be)?;
        let osnum = u16::from_be_bytes(osnum_be);
        let signature_type_be = [0_u8; 2];
        fh.read_exact(&mut osnum_be)?;
        let signature_type = u16::from_be_bytes(signature_type_be);
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
        writeln!(f, "reserved: {}", parse_string(&self.reserved))
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
        writeln!(f, "reserved: {:?}", self.reserved)
    }
}

impl Default for RawLead {
    fn default() -> Self {
        RawLead {
            magic: MAGIC,
            major: 0,
            minor: 0,
            rpm_type: Type::Binary,
            archnum: 0,
            name: [0; 66],
            osnum: 0,
            signature_type: 0,
            reserved: [0; 16],
        }
    }
}

fn parse_string(bytes: &[u8]) -> String {
    let position = bytes.iter().position(|&x| x == 0).unwrap_or(0);
    let bytes2 = &bytes[0..position];
    String::from_utf8_lossy(bytes2).to_string()
}

#[derive(Debug)]
pub struct RawHeader {
    pub magic: [u8; 4],
    pub reserved: [u8; 4],
    pub nindex: usize,
    pub hsize: u32,
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
        let nindex = u32::from_be_bytes(nindex_be);

        let mut hsize_be = [0_u8; 4];
        fh.read_exact(&mut hsize_be)?;
        let hsize = u32::from_be_bytes(hsize_be);

        Ok(RawHeader {
            magic,
            reserved,
            nindex: nindex as usize,
            hsize,
        })
    }
}

impl Default for RawHeader {
    fn default() -> Self {
        RawHeader {
            magic: MAGIC_HEADER,
      		reserved: [0; 4],
      		nindex: 0,
      		hsize: 0,
        }
    }
}
