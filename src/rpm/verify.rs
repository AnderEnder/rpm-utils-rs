use super::RPMFile;
use super::file::decompress;
use crate::header::{HeaderLead, RType, SignatureTag, Tag};
use crate::utils::align_n_bytes;
use sha2::Digest;
use std::io::{self, Read, Seek, SeekFrom};

impl<T: Read + Seek> RPMFile<T> {
    /// Verify stored integrity digests over the original header and payload.
    /// This detects corruption; it does not authenticate OpenPGP signatures.
    /// At least one supported digest must cover each of header and payload.
    /// The caller's stream position is preserved, including on verification errors.
    pub fn verify_integrity(&mut self) -> io::Result<()> {
        let saved = self.file.stream_position()?;
        let result = self.verify_digests();
        let restored = self.file.seek(SeekFrom::Start(saved));
        result.and(restored.map(|_| ()))
    }

    fn verify_digests(&mut self) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(96))?;
        let signature = HeaderLead::read(&mut self.file)?;
        let header_start = 96
            + 16
            + signature.nindex as u64 * 16
            + u64::from(signature.hsize)
            + u64::from(align_n_bytes(signature.hsize, 8));
        let header_size = self
            .payload_offset
            .checked_sub(header_start)
            .filter(|size| *size <= 64 * 1024 * 1024)
            .ok_or_else(|| invalid("Invalid header bounds"))?;
        let mut header = vec![0; header_size as usize];
        self.file.seek(SeekFrom::Start(header_start))?;
        self.file.read_exact(&mut header)?;
        let mut header_covered = false;
        let mut payload_covered = false;
        if let Some(expected) = self.signature_tags.get_value(SignatureTag::SHA1Header) {
            compare_hex(&expected, &sha1::Sha1::digest(&header), "header SHA1")?;
            header_covered = true;
        }
        if let Some(expected) = self.signature_tags.get_value(SignatureTag::SHA256Header) {
            compare_hex(&expected, &sha2::Sha256::digest(&header), "header SHA256")?;
            header_covered = true;
        }
        let mut md5 = md5::Md5::new();
        md5.update(&header);
        let mut sha256 = sha2::Sha256::new();
        let mut buffer = [0; 8192];
        let mut compressed_size = 0_u64;
        loop {
            let count = self.file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            md5.update(&buffer[..count]);
            sha256.update(&buffer[..count]);
            compressed_size += count as u64;
        }
        if let Some(expected) = self.signature_tags.get_value(SignatureTag::MD5) {
            match expected {
                RType::Bin(bytes) if bytes.as_slice() == md5.finalize().as_slice() => {}
                _ => return Err(invalid("RPM header+payload MD5 mismatch")),
            }
            header_covered = true;
            payload_covered = true;
        }
        let payload_digest = self.header_tags.get_value(Tag::PayloadDigest);
        let alternate = self.header_tags.get_value(Tag::PayloadDigestAlt);
        if payload_digest.is_some() || alternate.is_some() {
            let algorithm = match self.header_tags.get_value(Tag::PayloadDigestAlgo) {
                Some(value) => value
                    .as_u32()
                    .ok_or_else(|| invalid("Invalid payload digest algorithm"))?,
                None => 8,
            };
            if algorithm != 8 {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!("Unsupported payload digest algorithm {algorithm}"),
                ));
            }
        }
        if let Some(expected) = payload_digest {
            compare_hex(&expected, &sha256.finalize(), "payload SHA256")?;
            payload_covered = true;
        }
        let size = self
            .signature_tags
            .get_value(SignatureTag::LongSigSize)
            .or_else(|| self.signature_tags.get_value(SignatureTag::Size));
        if let Some(expected) = size
            && expected.as_u64() != Some(header_size + compressed_size)
        {
            return Err(invalid("RPM header+payload size mismatch"));
        }
        if let Some(expected) = alternate {
            self.file.seek(SeekFrom::Start(self.payload_offset))?;
            let compressor = self
                .header_tags
                .get_value(Tag::PayloadCompressor)
                .and_then(|value| value.as_string())
                .ok_or_else(|| invalid("Missing payload compressor"))?;
            let mut reader = decompress(&mut self.file, &compressor)?;
            let mut digest = sha2::Sha256::new();
            let mut size = 0_u64;
            loop {
                let count = reader.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                digest.update(&buffer[..count]);
                size += count as u64;
            }
            compare_hex(&expected, &digest.finalize(), "uncompressed payload SHA256")?;
            let expected_size = self
                .signature_tags
                .get_value(SignatureTag::LongArchiveSize)
                .or_else(|| self.signature_tags.get_value(SignatureTag::PayloadSize));
            if let Some(expected) = expected_size
                && expected.as_u64() != Some(size)
            {
                return Err(invalid("RPM uncompressed payload size mismatch"));
            }
            payload_covered = true;
        }
        if !header_covered || !payload_covered {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "No supported integrity digest covers the complete package",
            ));
        }
        Ok(())
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn compare_hex(expected: &RType, actual: &[u8], label: &str) -> io::Result<()> {
    let encoded = match expected {
        RType::String(value) => value,
        RType::StringArray(values) if values.len() == 1 => &values[0],
        _ => return Err(invalid("Invalid integrity digest type or count")),
    };
    let bytes = hex::decode(encoded).map_err(|_| invalid("Invalid integrity digest encoding"))?;
    if bytes != actual {
        return Err(invalid(&format!("RPM {label} mismatch")));
    }
    Ok(())
}
