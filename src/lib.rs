use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek};
use std::path::Path;

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
        writeln!(f, "name: {}", String::from_utf8_lossy(&self.name))?;
        writeln!(f, "osnum: {}", self.osnum)?;
        writeln!(f, "signature_type: {}", self.signature_type)?;
        writeln!(f, "reserved: {}", String::from_utf8_lossy(&self.reserved))?;
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
pub enum IndexType {
    Null,
    Char,
    Int8,
    Int16,
    Int32,
    Int64,
    String,
    Bin,
    StringArray,
}

impl From<i32> for IndexType {
    fn from(tag: i32) -> Self {
        match tag {
            0 => IndexType::Null,
            1 => IndexType::Char,
            2 => IndexType::Int8,
            3 => IndexType::Int16,
            4 => IndexType::Int32,
            5 => IndexType::Int64,
            6 => IndexType::String,
            7 => IndexType::Bin,
            8 => IndexType::StringArray,
            _ => IndexType::Null,
        }
    }
}

#[derive(Debug)]
pub enum RPMSignatureTag {
    HeaderSignatures,
    HeaderImmutable,
    Headeri18Ntable,
    Size,
    Other(i32),
}

impl From<i32> for RPMSignatureTag {
    fn from(tag: i32) -> Self {
        match tag {
            62 => RPMSignatureTag::HeaderSignatures,
            63 => RPMSignatureTag::HeaderImmutable,
            100 => RPMSignatureTag::Headeri18Ntable,
            1000 => RPMSignatureTag::Size,
            x => RPMSignatureTag::Other(x),
        }
    }
}

#[derive(Debug)]
pub struct RPMHDRIndex {
    pub tag: RPMSignatureTag,
    pub itype: IndexType,
    pub offset: i32,
    pub count: i32,
}

impl RPMHDRIndex {
    pub fn read<R: Read + Seek>(fh: &mut R) -> Result<Self, io::Error> {
        let mut tag_be = [0_u8; 4];
        fh.read_exact(&mut tag_be)?;
        let tag = i32::from_be_bytes(tag_be).into();

        let mut itype_be = [0_u8; 4];
        fh.read_exact(&mut itype_be)?;
        let itype = i32::from_be_bytes(itype_be).into();

        let mut offset_be = [0_u8; 4];
        fh.read_exact(&mut offset_be)?;
        let offset = i32::from_be_bytes(offset_be);

        let mut count_be = [0_u8; 4];
        fh.read_exact(&mut count_be)?;
        let count = i32::from_be_bytes(count_be);

        Ok(RPMHDRIndex {
            tag,
            itype,
            offset,
            count,
        })
    }
}

#[derive(Debug)]
pub struct RPMFile {
    pub lead: RawLead,
    pub signature: RawHeader,
    pub indexes: Vec<RPMHDRIndex>,
    pub file: File,
}

impl RPMFile {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, io::Error> {
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        let lead = RawLead::read(&mut file)?;
        let signature = RawHeader::read(&mut file)?;

        let mut indexes = Vec::with_capacity(signature.nindex as usize);
        for _ in 0..signature.nindex {
            let index = RPMHDRIndex::read(&mut file)?;
            indexes.push(index);
        }

        Ok(Self {
            lead,
            signature,
            indexes,
            file,
        })
    }
}
