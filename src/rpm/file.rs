use bzip2::read::BzDecoder;
use bzip2::write::BzEncoder;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use xz2::read::XzDecoder;
use xz2::stream::{LzmaOptions, Stream};
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

        check_package_format(&header_tags)?;
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
        let parent = path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut output = tempfile::NamedTempFile::new_in(parent)?;
        let count = self.copy_payload_to(&mut output)?;
        output.as_file_mut().sync_all()?;
        output.persist(path).map_err(|error| error.error)?;
        Ok(count)
    }

    /// Decompress the payload into an arbitrary destination, such as stdout.
    pub fn copy_payload_to<W: Write>(self, writer: &mut W) -> io::Result<u64> {
        let mut reader = self.into_uncompress_reader()?;
        io::copy(&mut reader, writer)
    }

    fn into_uncompress_reader(mut self) -> io::Result<Box<dyn Read>> {
        self.file.seek(SeekFrom::Start(self.payload_offset))?;

        let compressor: String = self
            .header_tags
            .get_value(Tag::PayloadCompressor)
            .ok_or_else(|| io::Error::other("Compression is not defined"))?
            .as_string()
            .ok_or_else(|| io::Error::other("Compression is not defined"))?;

        decompress(self.file, &compressor)
    }
}

impl<T: 'static + Write> RPMFile<T> {
    pub fn write_head(&mut self) -> io::Result<()> {
        check_package_format(&self.header_tags)?;
        self.file.write_lead(&self.lead)?;
        let mut signature = Vec::new();
        signature.write_header(&self.signature_tags)?;
        self.file.write_all(&signature)?;
        self.file
            .write_all(&vec![0; align_n_bytes(signature.len() as u32, 8) as usize])?;
        self.file.write_header(&self.header_tags)?;
        Ok(())
    }

    pub fn write_payload(self, path: &Path) -> io::Result<u64> {
        let mut reader = File::open(path)?;
        let compressor: String = self
            .header_tags
            .get_value(Tag::PayloadCompressor)
            .ok_or_else(|| io::Error::other("Compression is not defined"))?
            .as_string()
            .ok_or_else(|| io::Error::other("Compression is not defined"))?;

        let count = match compressor.as_str() {
            "gzip" => {
                let mut writer = GzEncoder::new(self.file, flate2::Compression::best());
                let count = io::copy(&mut reader, &mut writer)?;
                writer.finish()?;
                count
            }
            "bzip2" => {
                let mut writer = BzEncoder::new(self.file, bzip2::Compression::best());
                let count = io::copy(&mut reader, &mut writer)?;
                writer.finish()?;
                count
            }
            "zstd" => {
                let mut writer = Encoder::new(self.file, 3)?;
                let count = io::copy(&mut reader, &mut writer)?;
                writer.finish()?;
                count
            }
            "xz" | "lzma" => {
                let mut writer = if compressor == "lzma" {
                    XzEncoder::new_stream(
                        self.file,
                        Stream::new_lzma_encoder(&LzmaOptions::new_preset(3)?)?,
                    )
                } else {
                    XzEncoder::new(self.file, 3)
                };
                let count = io::copy(&mut reader, &mut writer)?;
                writer.finish()?;
                count
            }
            "none" => io::copy(&mut reader, &mut { self.file })?,
            format => {
                return Err(io::Error::other(format!(
                    "Compressor \"{}\" is not implemented",
                    format
                )));
            }
        };
        Ok(count)
    }
}

// The historical lead version is not the package format version. In particular,
// v6 packages can use a v4 lead. Do not silently treat an explicit v6 header as v4.
fn check_package_format(tags: &Tags<Tag>) -> io::Result<()> {
    if let Some(value) = tags.get_value(Tag::RpmFormat) {
        let format = value.as_u32().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Invalid RPM package format tag")
        })?;
        if format != 3 && format != 4 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("RPM package format v{format} is not supported (supported: v3/v4)"),
            ));
        }
    }
    Ok(())
}

pub(super) fn decompress<'a, R: Read + 'a>(
    reader: R,
    compressor: &str,
) -> io::Result<Box<dyn Read + 'a>> {
    match compressor {
        "gzip" => Ok(Box::new(GzDecoder::new(reader))),
        "bzip2" => Ok(Box::new(BzDecoder::new(reader))),
        "zstd" => Ok(Box::new(Decoder::new(reader)?)),
        "xz" => Ok(Box::new(XzDecoder::new(reader))),
        "lzma" => Ok(Box::new(XzDecoder::new_stream(
            reader,
            Stream::new_lzma_decoder(u64::MAX)?,
        ))),
        "none" => Ok(Box::new(reader)),
        format => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("Decompressor {format:?} is not implemented"),
        )),
    }
}
