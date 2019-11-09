use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::FromPrimitive;
use std::fmt;
use std::io::{self, Read, Seek};
use strum_macros::Display;

pub const MAGIC: [u8; 4] = [237, 171, 238, 219];

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Display)]
pub enum Type {
    Binary = 0,
    Source = 1,
}

pub struct Lead {
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

impl Lead {
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

        match (major, minor) {
            (3, 0) | (3, 1) | (4, 0) => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "Error: rpm format version is not supported {}.{}",
                        major, minor
                    ),
                ))
            }
        }

        let mut rpm_type_be = [0_u8; 2];
        fh.read_exact(&mut rpm_type_be)?;
        let rpm_type = Type::from_u16(u16::from_be_bytes(rpm_type_be)).ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Error: can not read the rpm type")
        })?;
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

        Ok(Lead {
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

impl fmt::Display for Lead {
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

impl fmt::Debug for Lead {
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

impl Default for Lead {
    fn default() -> Self {
        Lead {
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
