use rpm_utils::header::{RType, SignatureTag, Tag, Tags, TagsWrite};
use rpm_utils::lead::Lead;
use rpm_utils::{RPMBuilder, RPMFile, RPMInfo};
use std::io::{self, Cursor, Write};
use tempfile::tempdir;

#[test]
fn payload_roundtrip_finishes_every_compression_format() -> io::Result<()> {
    let dir = tempdir()?;
    let input = dir.path().join("payload.cpio");
    let content = b"payload data including \0 binary bytes".repeat(2048);
    std::fs::write(&input, &content)?;
    for compressor in ["gzip", "bzip2", "xz", "lzma", "zstd", "none"] {
        let path = dir.path().join(format!("{compressor}.rpm"));
        let mut rpm = RPMBuilder::new()
            .filename(&path)
            .package_name("sample")
            .version("1.0")
            .release("2")
            .epoch(1000)
            .compression(compressor)
            .build()?;
        // Force signature padding, and a main header ending at an unaligned byte.
        rpm.signature_tags
            .insert(SignatureTag::MD5, RType::Bin(vec![1, 2, 3]));
        rpm.header_tags
            .insert(Tag::PayloadDigestAlt, RType::Bin(vec![5]));
        let mut header = Vec::new();
        header.write_header(&rpm.header_tags)?;
        if header.len() % 8 == 0 {
            rpm.header_tags
                .insert(Tag::PayloadDigestAlt, RType::Bin(vec![5, 6]));
        }
        rpm.write_head()?;
        let payload_start = std::fs::metadata(&path)?.len();
        assert_ne!(payload_start % 8, 0);
        assert_eq!(rpm.write_payload(&input)?, content.len() as u64);
        let parsed = RPMFile::open(&path)?;
        assert_eq!(parsed.payload_offset, payload_start);
        assert_eq!(parsed.header_tags.get_as_string(Tag::Release), "2");
        assert_eq!(parsed.header_tags.get_as_u32(Tag::Epoch), 1000);
        let output = dir.path().join(format!("{compressor}.cpio"));
        assert_eq!(parsed.copy_payload(&output)?, content.len() as u64);
        assert_eq!(std::fs::read(output)?, content, "{compressor}");
    }
    Ok(())
}

struct FailingWriter;
impl Write for FailingWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("output failed"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("output failed"))
    }
}

#[test]
fn compression_finalization_propagates_output_errors() -> io::Result<()> {
    let dir = tempdir()?;
    let input = dir.path().join("empty");
    std::fs::write(&input, [])?;
    for compressor in ["gzip", "bzip2", "xz", "lzma", "zstd"] {
        let rpm = RPMInfo {
            payload: rpm_utils::payload::RPMPayload {
                compressor: compressor.into(),
                ..Default::default()
            },
            ..Default::default()
        }
        .try_into_rpm(FailingWriter)?;
        assert!(rpm.write_payload(&input).is_err(), "{compressor}");
    }
    Ok(())
}

#[test]
fn info_handles_optional_tags_single_file_arrays_and_large_epoch() {
    let mut tags = Tags::new();
    tags.insert(Tag::Epoch, RType::Int32(1000))
        .insert(Tag::BaseNames, RType::StringArray(vec!["one".into()]))
        .insert(Tag::DirNames, RType::StringArray(vec!["/usr/".into()]))
        .insert(Tag::DirIndexes, RType::Int32(0))
        .insert(Tag::FileSizes, RType::Int32(123))
        .insert(Tag::FileModes, RType::Int16(0o100755))
        .insert(Tag::FileLinktos, RType::StringArray(vec!["target".into()]))
        .insert(Tag::FileGroupName, RType::StringArray(vec!["root".into()]))
        .insert(Tag::BuildTime, RType::Int64(123456));
    let mut rpm = RPMFile {
        lead: Lead::default(),
        signature_tags: Tags::new(),
        header_tags: tags,
        payload_offset: 0,
        file: Cursor::new([]),
    };
    let info = RPMInfo::from(&rpm);
    assert_eq!(info.epoch, 1000);
    assert_eq!(info.build_time, 123456);
    assert_eq!(info.payload.files.len(), 1);
    let file = &info.payload.files[0];
    assert_eq!(file.name, "/usr/one");
    assert_eq!(file.size, 123);
    assert_eq!(file.mode, 0o100755);
    assert_eq!(file.linkname, "target");
    // Malformed parallel arrays or optional tag types must not panic.
    rpm.header_tags
        .insert(Tag::DirIndexes, RType::Int32(u32::MAX));
    rpm.header_tags
        .insert(Tag::Summary, RType::Int32Array(vec![1, 2]));
    assert_eq!(RPMInfo::from(&rpm).payload.files[0].name, "one");
    assert!(
        !RPMInfo {
            build_time: i64::MAX,
            ..Default::default()
        }
        .to_string()
        .is_empty()
    );
}

#[test]
fn metadata_conversion_retains_file_entries_and_digest_algorithm() -> io::Result<()> {
    let directory = tempdir()?;
    let source = directory.path().join("file");
    std::fs::write(&source, b"metadata")?;
    let rpm = RPMBuilder::new()
        .filename(directory.path().join("test.rpm"))
        .package_name("metadata")
        .version("1")
        .add_file_as(&source, "/usr/share/test/file")
        .build_package()?;
    let info = RPMInfo::from(&rpm);
    let expected_digest = info.payload.files[0].digest.clone();
    assert_eq!(info.payload.file_digest_algorithm, 8);
    let mut converted = info.try_into_rpm(Cursor::new(Vec::new()))?;
    converted.write_head()?;
    let parsed = RPMFile::read(Cursor::new(converted.file.into_inner()))?;
    let info = RPMInfo::from(&parsed);
    assert_eq!(info.payload.files.len(), 1);
    assert_eq!(info.payload.files[0].name, "/usr/share/test/file");
    assert_eq!(info.payload.files[0].digest, expected_digest);
    assert_eq!(info.payload.file_digest_algorithm, 8);
    assert!(
        RPMInfo {
            build_time: -1,
            ..Default::default()
        }
        .try_into_rpm(Vec::new())
        .is_err()
    );
    Ok(())
}
