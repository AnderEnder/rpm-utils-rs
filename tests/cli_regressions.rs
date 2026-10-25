use rpm_utils::RPMBuilder;
use rpm_utils::payload::{CpioWriter, FileEntry};
use std::io;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn rpm2cpio_supports_stdout_and_explicit_output() -> io::Result<()> {
    let dir = tempdir()?;
    let input = dir.path().join("payload.cpio");
    let content = b"binary\0payload\xff";
    std::fs::write(&input, content)?;
    let package = dir.path().join("test.rpm");
    let mut rpm = RPMBuilder::new()
        .filename(&package)
        .package_name("test")
        .build()?;
    rpm.write_head()?;
    rpm.write_payload(&input)?;
    let output = Command::new(env!("CARGO_BIN_EXE_rpm2cpio"))
        .arg(&package)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, content);
    let destination = dir.path().join("output.cpio");
    let output = Command::new(env!("CARGO_BIN_EXE_rpm2cpio"))
        .arg(&package)
        .arg("--output")
        .arg(&destination)
        .output()?;
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read(destination)?, content);
    let info = Command::new(env!("CARGO_BIN_EXE_rpm-info"))
        .arg(&package)
        .output()?;
    assert!(
        info.status.success(),
        "{}",
        String::from_utf8_lossy(&info.stderr)
    );
    assert!(String::from_utf8_lossy(&info.stdout).contains("Release     : 1"));
    Ok(())
}

#[test]
fn cpio_extract_creates_destination_directory() -> io::Result<()> {
    let dir = tempdir()?;
    let archive = dir.path().join("archive.cpio");
    let mut bytes = Vec::new();
    bytes.write_cpio_record(
        FileEntry {
            name: "file".into(),
            mode: 0o100644,
            file_size: 5,
            ..Default::default()
        },
        &mut &b"hello"[..],
    )?;
    bytes.cpio_close()?;
    std::fs::write(&archive, bytes)?;
    let destination = dir.path().join("new/nested");
    let output = Command::new(env!("CARGO_BIN_EXE_cpio-extract"))
        .arg(&archive)
        .arg("-e")
        .arg(&destination)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(destination.join("file"))?, b"hello");
    Ok(())
}
