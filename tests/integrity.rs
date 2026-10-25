use rpm_utils::{RPMBuilder, RPMFile};
use std::io::{self, Cursor, Seek, SeekFrom};
use tempfile::tempdir;

fn package(compressor: &str) -> io::Result<Vec<u8>> {
    let directory = tempdir()?;
    let source = directory.path().join("file");
    std::fs::write(&source, b"integrity test data")?;
    let destination = directory.path().join("test.rpm");
    RPMBuilder::new()
        .filename(&destination)
        .package_name("integrity")
        .version("1")
        .compression(compressor)
        .add_file_as(&source, "/usr/share/test/file")
        .build_package()?;
    std::fs::read(destination)
}

#[test]
fn verifies_every_supported_compressor_and_preserves_cursor() -> io::Result<()> {
    for compressor in ["gzip", "bzip2", "xz", "lzma", "zstd", "none"] {
        let mut rpm = RPMFile::read(Cursor::new(package(compressor)?))?;
        rpm.file.seek(SeekFrom::Start(17))?;
        rpm.verify_integrity()?;
        assert_eq!(rpm.file.position(), 17);
    }
    Ok(())
}

#[test]
fn rejects_header_and_payload_corruption_and_truncation() -> io::Result<()> {
    let original = package("gzip")?;
    let parsed = RPMFile::read(Cursor::new(original.clone()))?;
    let header_name = original
        .windows(b"integrity\0".len())
        .rposition(|bytes| bytes == b"integrity\0")
        .unwrap();
    for mutation in [
        header_name,
        parsed.payload_offset as usize,
        original.len() - 1,
    ] {
        let mut bytes = original.clone();
        bytes[mutation] ^= 1;
        let mut rpm = RPMFile::read(Cursor::new(bytes))?;
        let position = rpm.file.position();
        assert_eq!(
            rpm.verify_integrity().unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(rpm.file.position(), position);
    }
    let mut bytes = original;
    bytes.truncate(bytes.len() - 3);
    assert!(
        RPMFile::read(Cursor::new(bytes))?
            .verify_integrity()
            .is_err()
    );
    Ok(())
}

#[test]
fn failed_decompression_preserves_destination() -> io::Result<()> {
    let directory = tempdir()?;
    let mut bytes = package("gzip")?;
    let parsed = RPMFile::read(Cursor::new(bytes.clone()))?;
    bytes[parsed.payload_offset as usize] ^= 255;
    let output = directory.path().join("existing.cpio");
    std::fs::write(&output, b"keep me")?;
    assert!(
        RPMFile::read(Cursor::new(bytes))?
            .copy_payload(&output)
            .is_err()
    );
    assert_eq!(std::fs::read(output)?, b"keep me");
    Ok(())
}

#[test]
fn cli_verify_reports_corruption_with_failure_status() -> io::Result<()> {
    let directory = tempdir()?;
    let path = directory.path().join("test.rpm");
    let mut bytes = package("gzip")?;
    std::fs::write(&path, &bytes)?;
    let verified = std::process::Command::new(env!("CARGO_BIN_EXE_rpm-info"))
        .arg("--verify")
        .arg(&path)
        .output()?;
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(&path, bytes)?;
    let rejected = std::process::Command::new(env!("CARGO_BIN_EXE_rpm-info"))
        .arg("--verify")
        .arg(path)
        .output()?;
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("mismatch"));
    Ok(())
}
