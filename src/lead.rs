use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};
use omnom::prelude::*;
use std::fmt;
use std::io::{self, Read, Seek, Write};
use strum_macros::Display;

use crate::utils::parse_string;

pub const MAGIC: [u8; 4] = [237, 171, 238, 219];

#[derive(Debug, PartialEq, FromPrimitive, ToPrimitive, Display, Clone)]
pub enum Type {
    Binary = 0,
    Source = 1,
}

#[derive(Clone)]
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
    pub fn read<R: Read + Seek>(fh: &mut R) -> io::Result<Self> {
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

        let rpm_type_id: u16 = fh.read_be()?;
        let rpm_type = Type::from_u16(rpm_type_id).ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Error: can not read the rpm type")
        })?;
        let archnum: u16 = fh.read_be()?;

        let mut name = [0_u8; 66];
        fh.read_exact(&mut name)?;
        let osnum: u16 = fh.read_be()?;
        let signature_type: u16 = fh.read_be()?;

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

    pub fn write<R: Write>(&self, fh: &mut R) -> io::Result<()> {
        fh.write_all(&MAGIC)?;
        fh.write_all(&[self.major, self.minor])?;

        let rpm_type = self.rpm_type.to_u16().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "Error: rpm type is not correct")
        })?;
        fh.write_be(rpm_type)?;
        fh.write_be(self.archnum)?;

        fh.write_all(&self.name)?;

        fh.write_be(self.osnum)?;
        fh.write_be(self.signature_type)?;

        // reserve
        fh.write_all(&[0_u8; 16])?;
        Ok(())
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
            signature_type: 5,
            reserved: [0; 16],
        }
    }
}

pub trait LeadWriter {
    fn write_lead(&mut self, lead: Lead) -> io::Result<()>;
}

impl<W: Write> LeadWriter for W {
    fn write_lead(&mut self, lead: Lead) -> io::Result<()> {
        lead.write(self)
    }
}
