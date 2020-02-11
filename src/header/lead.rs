use omnom::prelude::*;
use std::io::{self, Read, Write};

pub const MAGIC_HEADER: [u8; 4] = [142, 173, 232, 1];

#[derive(Debug)]
pub struct HeaderLead {
    pub magic: [u8; 4],
    pub reserved: [u8; 4],
    pub nindex: usize,
    pub hsize: u32,
}

impl HeaderLead {
    pub fn read<R: Read>(fh: &mut R) -> io::Result<Self> {
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

        let nindex: u32 = fh.read_be()?;
        let hsize: u32 = fh.read_be()?;

        Ok(HeaderLead {
            magic,
            reserved,
            nindex: nindex as usize,
            hsize,
        })
    }

    pub fn write<W: Write>(&self, fh: &mut W) -> io::Result<()> {
        fh.write_all(&MAGIC_HEADER)?;
        fh.write_all(&self.reserved)?;
        fh.write_be(self.nindex)?;
        fh.write_be(self.hsize)?;
        Ok(())
    }
}

impl Default for HeaderLead {
    fn default() -> Self {
        HeaderLead {
            magic: MAGIC_HEADER,
            reserved: [0; 4],
            nindex: 0,
            hsize: 0,
        }
    }
}
