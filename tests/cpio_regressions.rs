use rpm_utils::payload::{
    CpioBuilder, CpioRead, CpioWriter, FileEntry, extract_entries, extract_entry,
};
use std::io::{self, Cursor};
use tempfile::tempdir;

fn record(name: &str, mode: u32, nlink: u32, size: u32) -> FileEntry {
    FileEntry {
        name: name.into(),
        mode,
        nlink,
        file_size: size,
        ..FileEntry::default()
    }
}

#[test]
fn payload_read_leaves_reader_at_next_header() -> io::Result<()> {
    let mut archive = Vec::new();
    archive.write_cpio_record(record("first", 0o100644, 1, 5), &mut &b"hello"[..])?;
    archive.write_cpio_record(record("second", 0o100644, 1, 3), &mut &b"bye"[..])?;
    archive.cpio_close()?;
    let mut reader = Cursor::new(archive);
    let first = reader.read_cpio_entry()?;
    let mut content = Vec::new();
    reader.read_cpio_entry_payload(&first, &mut content)?;
    assert_eq!(content, b"hello");
    assert_eq!(reader.read_cpio_entry()?.name, "second");
    Ok(())
}

#[test]
fn extraction_uses_file_type_and_preserves_permissions() -> io::Result<()> {
    let mut archive = Vec::new();
    archive.write_cpio_record(record("dir", 0o040755, 3, 0), &mut io::empty())?;
    archive.write_cpio_record(record("dir/file", 0o100755, 2, 5), &mut &b"hello"[..])?;
    archive.cpio_close()?;
    let dir = tempdir()?;
    let entries = extract_entries(&mut Cursor::new(archive), dir.path(), true, false)?;
    assert_eq!(entries.len(), 2);
    assert!(dir.path().join("dir").is_dir());
    assert_eq!(std::fs::read(dir.path().join("dir/file"))?, b"hello");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.path().join("dir/file"))?
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }
    Ok(())
}

#[test]
fn writes_ascii_checksum_and_rejects_unterminated_names() -> io::Result<()> {
    let mut archive = Vec::new();
    archive.write_cpio_entry(record("file", 0o100644, 1, 0))?;
    assert_eq!(&archive[102..110], b"00000000");
    archive[114] = b'x';
    assert_eq!(
        FileEntry::read(&mut archive.as_slice()).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    Ok(())
}

#[test]
fn record_writer_enforces_declared_payload_length() -> io::Result<()> {
    let mut archive = Vec::new();
    let error = archive
        .write_cpio_record(record("short", 0o100644, 1, 5), &mut &b"hi"[..])
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    let mut archive = Vec::new();
    archive.write_cpio_record(record("bounded", 0o100644, 1, 2), &mut &b"hello"[..])?;
    archive.cpio_close()?;
    let mut reader = Cursor::new(archive);
    let entry = reader.read_cpio_entry()?;
    let mut data = Vec::new();
    reader.read_cpio_entry_payload(&entry, &mut data)?;
    assert_eq!(data, b"he");
    assert_eq!(reader.read_cpio_entry()?.name, "TRAILER!!!");
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_dangling_symlinks_before_creating_external_files() -> io::Result<()> {
    use std::os::unix::fs::symlink;
    let dir = tempdir()?;
    let outside = tempdir()?;
    let target = outside.path().join("missing");
    symlink(&target, dir.path().join("link"))?;
    let mut archive = Vec::new();
    archive.write_cpio_record(record("link", 0o100644, 1, 5), &mut &b"hello"[..])?;
    assert!(extract_entry(&mut Cursor::new(archive), dir.path(), true, false).is_err());
    assert!(!target.exists());
    Ok(())
}

#[test]
fn builder_archives_directories_with_zero_payload_size() -> io::Result<()> {
    let source = tempdir()?;
    let path = source.path().join("directory");
    std::fs::create_dir(&path)?;
    let mut archive = Vec::new();
    CpioBuilder::new(&mut archive)
        .add_raw_file(&path)?
        .build()?;
    let mut reader = Cursor::new(archive);
    let entry = reader.read_cpio_entry()?;
    assert_eq!(entry.mode & 0o170000, 0o040000);
    assert_eq!(entry.file_size, 0);
    assert_eq!(reader.read_cpio_entry()?.name, "TRAILER!!!");
    Ok(())
}
