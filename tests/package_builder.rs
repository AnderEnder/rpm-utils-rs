use rpm_utils::header::{RType, SignatureTag, Tag};
use rpm_utils::payload::extract_entries;
use rpm_utils::{RPMBuilder, RPMInfo};
use sha2::Digest;
use std::io;
use tempfile::tempdir;

#[test]
fn builds_complete_v4_packages_with_independently_checked_digests() -> io::Result<()> {
    let directory = tempdir()?;
    let source = directory.path().join("source");
    std::fs::write(&source, b"package data\0with binary")?;
    for compressor in ["gzip", "bzip2", "xz", "lzma", "zstd", "none"] {
        let destination = directory.path().join(format!("{compressor}.rpm"));
        let rpm = RPMBuilder::default()
            .filename(&destination)
            .package_name("sample")
            .version("1.2")
            .release("3")
            .compression(compressor)
            .default_user("tester")
            .default_group("staff")
            .add_file_as(&source, "/usr/share/sample/data")
            .build_package()?;
        let info = RPMInfo::from(&rpm);
        assert_eq!(info.payload.files.len(), 1);
        let file = &info.payload.files[0];
        assert_eq!(file.name, "/usr/share/sample/data");
        assert_eq!(file.user, "tester");
        assert_eq!(file.group, "staff");
        assert_eq!(file.size, 24);
        assert_eq!(
            file.digest,
            hex::encode(sha2::Sha256::digest(std::fs::read(&source)?))
        );
        assert!(info.payload.size > 0);
        let bytes = std::fs::read(&destination)?;
        let read_u32 =
            |offset| u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let signature_size = 16 + read_u32(104) * 16 + read_u32(108);
        let header_start = 96 + signature_size.div_ceil(8) * 8;
        let payload_start = rpm.payload_offset as usize;
        let header = &bytes[header_start..payload_start];
        assert_eq!(
            rpm.signature_tags.get_as_string(SignatureTag::SHA1Header),
            hex::encode(sha1::Sha1::digest(header))
        );
        assert_eq!(
            rpm.signature_tags.get_as_string(SignatureTag::SHA256Header),
            hex::encode(sha2::Sha256::digest(header))
        );
        assert_eq!(
            rpm.signature_tags.get_value(SignatureTag::MD5),
            Some(RType::Bin(
                md5::Md5::digest(&bytes[header_start..]).to_vec()
            ))
        );
        assert_eq!(
            rpm.signature_tags.get_as_u64(SignatureTag::Size),
            (bytes.len() - header_start) as u64
        );
        assert_eq!(
            rpm.header_tags.get_as_string_array_or(Tag::PayloadDigest),
            [hex::encode(sha2::Sha256::digest(&bytes[payload_start..]))]
        );
        let count = read_u32(header_start + 8);
        assert_eq!(read_u32(header_start + 16), 63);
        let trailer = header_start + 16 + count * 16 + read_u32(header_start + 24);
        assert_eq!(read_u32(trailer), 63);
        assert_eq!(read_u32(trailer + 8) as u32 as i32, -(count as i32 * 16));
        let mut payload = Vec::new();
        rpm.copy_payload_to(&mut payload)?;
        let extraction = directory.path().join(format!("extract-{compressor}"));
        std::fs::create_dir(&extraction)?;
        extract_entries(&mut payload.as_slice(), &extraction, true, false)?;
        assert_eq!(
            std::fs::read(extraction.join("usr/share/sample/data"))?,
            std::fs::read(&source)?
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn recursive_directories_include_symlinks_and_hardlinks() -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, symlink};
    let directory = tempdir()?;
    let source = directory.path().join("source");
    std::fs::create_dir(&source)?;
    std::fs::write(source.join("file"), b"content")?;
    std::fs::hard_link(source.join("file"), source.join("alias"))?;
    symlink("file", source.join("link"))?;
    let rpm = RPMBuilder::new()
        .filename(directory.path().join("tree.rpm"))
        .package_name("tree")
        .version("1")
        .add_file_as(&source, "/opt/tree")
        .build_package()?;
    let info = RPMInfo::from(&rpm);
    assert_eq!(info.payload.files.len(), 4);
    assert_eq!(
        info.payload
            .files
            .iter()
            .find(|file| file.name.ends_with("/link"))
            .unwrap()
            .linkname,
        "file"
    );
    let mut payload = Vec::new();
    rpm.copy_payload_to(&mut payload)?;
    let destination = directory.path().join("extract");
    std::fs::create_dir(&destination)?;
    extract_entries(&mut payload.as_slice(), &destination, true, false)?;
    let tree = destination.join("opt/tree");
    assert_eq!(
        std::fs::read_link(tree.join("link"))?,
        std::path::Path::new("file")
    );
    assert_eq!(
        std::fs::metadata(tree.join("file"))?.ino(),
        std::fs::metadata(tree.join("alias"))?.ino()
    );
    Ok(())
}

#[test]
fn validation_errors_preserve_existing_outputs() -> io::Result<()> {
    let directory = tempdir()?;
    let destination = directory.path().join("existing.rpm");
    std::fs::write(&destination, b"keep me")?;
    let base = RPMBuilder::new()
        .filename(&destination)
        .package_name("test")
        .version("1");
    for builder in [
        base.clone().compression("invalid"),
        base.clone().build_time(-1),
        base.clone().build_time(i64::MAX),
        base.clone().pre_install("/no/such/script"),
        base.clone().add_file("/no/such/file"),
        base.clone().summary("bad\0text"),
        base.clone().add_file_as("/no/such/file", "../escape"),
    ] {
        assert!(builder.clone().build().is_err());
        assert!(builder.build_package().is_err());
        assert_eq!(std::fs::read(&destination)?, b"keep me");
    }
    let missing_parent = directory.path().join("missing/test.rpm");
    assert!(base.filename(missing_parent).build_package().is_err());
    assert_eq!(std::fs::read(&destination)?, b"keep me");
    Ok(())
}

#[test]
fn native_archive_reader_can_read_generated_rpm() -> io::Result<()> {
    // Run where the system tar uses libarchive and understands RPM envelopes.
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    let directory = tempdir()?;
    let source = directory.path().join("file");
    std::fs::write(&source, b"native interoperability")?;
    let path = directory.path().join("native.rpm");
    RPMBuilder::new()
        .filename(&path)
        .package_name("native")
        .version("1")
        .add_file_as(&source, "/usr/share/native/file")
        .build_package()?;
    let output = std::process::Command::new("/usr/bin/tar")
        .arg("-tf")
        .arg(path)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("usr/share/native/file"));
    Ok(())
}
