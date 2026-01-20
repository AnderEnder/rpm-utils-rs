use bzip2::read::BzDecoder;
use bzip2::write::BzEncoder;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use xz2::read::XzDecoder;
use xz2::write::XzEncoder;
use zstd::stream::read::Decoder;
use zstd::stream::write::Encoder;

use crate::header::{HeaderLead, IndexArray, SignatureTag, Tag, Tags, TagsWrite};
use crate::lead::{Lead, LeadWriter};
use crate::utils::align_n_bytes;

#[derive(Debug)]
pub struct RPMFile<T> {
    pub lead: Lead,
    pub signature_tags: Tags<SignatureTag>,
    pub header_tags: Tags<Tag>,
    pub payload_offset: u64,
    pub file: T,
}

impl RPMFile<File> {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).open(path)?;
        Self::read(file)
    }
}

impl<T: 'static + Read + Seek> RPMFile<T> {
    pub fn read(mut reader: T) -> io::Result<Self> {
        let lead = Lead::read(&mut reader)?;

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

        let payload_offset = reader.stream_position()?;

        Ok(RPMFile {
            lead,
            signature_tags,
            header_tags,
            file: reader,
            payload_offset,
        })
    }

    pub fn copy_payload(self, path: &Path) -> io::Result<u64> {
        let mut writer = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        let mut reader = self.into_uncompress_reader()?;
        io::copy(&mut reader, &mut writer)
    }

    fn into_uncompress_reader(mut self) -> io::Result<Box<dyn Read>> {
        self.file.seek(SeekFrom::Start(self.payload_offset))?;

        let compressor: String = self
            .header_tags
            .get_value(Tag::PayloadCompressor)
            .ok_or_else(|| io::Error::other("Compression is not defined"))?
            .as_string()
            .ok_or_else(|| io::Error::other("Compression is not defined"))?;

        match compressor.as_str() {
            "gzip" => Ok(Box::new(GzDecoder::new(self.file))),
            "bzip2" => Ok(Box::new(BzDecoder::new(self.file))),
            "zstd" => Ok(Box::new(Decoder::new(self.file)?)),
            "xz" | "lzma" => Ok(Box::new(XzDecoder::new(self.file))),
            format => Err(io::Error::other(format!(
                "Decompressor \"{}\" is not implemented",
                format
            ))),
        }
    }
}

impl<T: 'static + Write> RPMFile<T> {
    pub fn write_head(&mut self) -> io::Result<()> {
        self.file.write_lead(&self.lead)?;
        self.file.write_header(&self.signature_tags)?;
        self.file.write_header(&self.header_tags)?;
        Ok(())
    }

    pub fn write_payload(self, path: &Path) -> io::Result<u64> {
        let mut reader = OpenOptions::new().open(path)?;
        let mut writer = self.into_compress_writer()?;
        io::copy(&mut reader, &mut writer)
    }

    fn into_compress_writer(self) -> io::Result<Box<dyn Write>> {
        let compressor: String = self
            .header_tags
            .get_value(Tag::PayloadCompressor)
            .ok_or_else(|| io::Error::other("Compression is not defined"))?
            .as_string()
            .ok_or_else(|| io::Error::other("Compression is not defined"))?;

        match compressor.as_str() {
            "gzip" => Ok(Box::new(GzEncoder::new(
                self.file,
                flate2::Compression::best(),
            ))),
            "bzip2" => Ok(Box::new(BzEncoder::new(
                self.file,
                bzip2::Compression::best(),
            ))),
            "zstd" => Ok(Box::new(Encoder::new(self.file, 3)?)),
            "xz" | "lzma" => Ok(Box::new(XzEncoder::new(self.file, 3))),

            format => Err(io::Error::other(format!(
                "Decompressor \"{}\" is not implemented",
                format
            ))),
        }
    }
}
