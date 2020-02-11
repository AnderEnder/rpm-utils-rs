use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use xz2::read::XzDecoder;
use zstd::stream::read::Decoder;

use crate::header::{HeaderLead, IndexArray, SignatureTag, Tag, Tags};
use crate::lead::Lead;
use crate::utils::align_n_bytes;

#[derive(Debug)]
pub struct RPMFile<T> {
    pub signature_tags: Tags<SignatureTag>,
    pub header_tags: Tags<Tag>,
    pub payload_offset: u64,
    pub file: T,
}

impl RPMFile<File> {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut file = OpenOptions::new().read(true).open(path)?;

        let _lead = Lead::read(&mut file)?;

        let signature_lead = HeaderLead::read(&mut file)?;
        let signature_indexes = IndexArray::read(&mut file, signature_lead.nindex)?;
        let signature_tags =
            Tags::read(&mut file, &signature_indexes, signature_lead.hsize as usize)?;

        // aligning to 8 bytes
        let pos = align_n_bytes(signature_lead.hsize, 8);

        file.seek(io::SeekFrom::Current(pos.into()))?;

        let header = HeaderLead::read(&mut file)?;
        let header_indexes = IndexArray::read(&mut file, header.nindex)?;
        let header_tags = Tags::read(&mut file, &header_indexes, header.hsize as usize)?;

        let payload_offset = file.seek(SeekFrom::Current(0))?;

        Ok(RPMFile {
            signature_tags,
            header_tags,
            file,
            payload_offset,
        })
    }
}

impl<T: 'static + Read + Seek> RPMFile<T> {
    pub fn read(mut reader: T) -> io::Result<Self> {
        let _lead = Lead::read(&mut reader)?;

        let signature_lead = HeaderLead::read(&mut reader)?;
        let signature_indexes = IndexArray::read(&mut reader, signature_lead.nindex)?;
        let signature_tags = Tags::read(
            &mut reader,
            &signature_indexes,
            signature_lead.hsize as usize,
        )?;

        // aligning to 8 bytes
        let pos = align_n_bytes(signature_lead.hsize, 8);

        reader.seek(io::SeekFrom::Current(pos.into()))?;

        let header = HeaderLead::read(&mut reader)?;
        let header_indexes = IndexArray::read(&mut reader, header.nindex)?;
        let header_tags = Tags::read(&mut reader, &header_indexes, header.hsize as usize)?;

        let payload_offset = reader.seek(SeekFrom::Current(0))?;

        Ok(RPMFile {
            signature_tags,
            header_tags,
            file: reader,
            payload_offset,
        })
    }

    pub fn copy_payload(self, path: &Path) -> io::Result<u64> {
        let mut writer = OpenOptions::new().create(true).write(true).open(path)?;
        let mut reader = self.into_uncompress_reader()?;
        io::copy(&mut reader, &mut writer)
    }

    fn into_uncompress_reader(mut self) -> io::Result<Box<dyn Read>> {
        self.file.seek(SeekFrom::Start(self.payload_offset))?;

        let compressor: String = self
            .header_tags
            .get_value(Tag::PayloadCompressor)
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Compression is not defined"))?
            .as_string()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Compression is not defined"))?;

        match compressor.as_str() {
            "gzip" => Ok(Box::new(GzDecoder::new(self.file))),
            "bzip2" => Ok(Box::new(BzDecoder::new(self.file))),
            "zstd" => Ok(Box::new(Decoder::new(self.file)?)),
            "xz" | "lzma" => Ok(Box::new(XzDecoder::new(self.file))),
            format => Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Decompressor \"{}\" is not implemented", format),
            )),
        }
    }
}
