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

#[cfg(unix)]
#[test]
fn symlinks_roundtrip_without_modifying_their_targets() -> io::Result<()> {
    use std::os::unix::fs::symlink;
    let source = tempdir()?;
    let outside = tempdir()?;
    let target = outside.path().join("target");
    std::fs::write(&target, b"unchanged")?;
    let timestamp = filetime::FileTime::from_unix_time(123456, 0);
    filetime::set_file_mtime(&target, timestamp)?;
    let link = source.path().join("link");
    symlink(&target, &link)?;
    let mut archive = Vec::new();
    CpioBuilder::new(&mut archive)
        .add_raw_file(&link)?
        .build()?;
    let destination = tempdir()?;
    // A slice has no Seek implementation: extraction works directly from streams.
    extract_entries(&mut archive.as_slice(), destination.path(), true, false)?;
    assert_eq!(std::fs::read_link(destination.path().join("link"))?, target);
    assert_eq!(
        filetime::FileTime::from_last_modification_time(&std::fs::metadata(&target)?),
        timestamp
    );
    assert_eq!(std::fs::read(&target)?, b"unchanged");
    Ok(())
}

#[cfg(unix)]
#[test]
fn hard_links_share_data_even_when_payload_is_last() -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let mut archive = Vec::new();
    for (name, data) in [
        ("first", &b""[..]),
        ("second", &b""[..]),
        ("last", &b"hello"[..]),
    ] {
        let mut entry = record(name, 0o100444, 3, data.len() as u32);
        entry.ino = 42;
        archive.write_cpio_record(entry, &mut &data[..])?;
    }
    archive.cpio_close()?;
    let destination = tempdir()?;
    extract_entries(&mut archive.as_slice(), destination.path(), true, false)?;
    let first = std::fs::metadata(destination.path().join("first"))?;
    for name in ["first", "second", "last"] {
        assert_eq!(std::fs::read(destination.path().join(name))?, b"hello");
        assert_eq!(
            std::fs::metadata(destination.path().join(name))?.ino(),
            first.ino()
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn restores_readonly_directory_metadata_after_children() -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut archive = Vec::new();
    let mut directory = record("dir", 0o040555, 2, 0);
    directory.mtime = 123456;
    archive.write_cpio_record(directory, &mut io::empty())?;
    archive.write_cpio_record(record("dir/file", 0o100644, 1, 5), &mut &b"hello"[..])?;
    archive.cpio_close()?;
    let destination = tempdir()?;
    extract_entries(&mut archive.as_slice(), destination.path(), true, false)?;
    let metadata = std::fs::metadata(destination.path().join("dir"))?;
    assert_eq!(metadata.permissions().mode() & 0o777, 0o555);
    assert_eq!(
        filetime::FileTime::from_last_modification_time(&metadata).unix_seconds(),
        123456
    );
    std::fs::set_permissions(
        destination.path().join("dir"),
        std::fs::Permissions::from_mode(0o755),
    )?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn overwriting_existing_hardlink_does_not_modify_external_inode() -> io::Result<()> {
    let outside = tempdir()?;
    let destination = tempdir()?;
    let target = outside.path().join("target");
    std::fs::write(&target, b"unchanged")?;
    std::fs::hard_link(&target, destination.path().join("file"))?;
    let mut archive = Vec::new();
    archive.write_cpio_record(record("file", 0o100644, 1, 3), &mut &b"new"[..])?;
    archive.cpio_close()?;
    extract_entries(&mut archive.as_slice(), destination.path(), true, false)?;
    assert_eq!(std::fs::read(&target)?, b"unchanged");
    assert_eq!(std::fs::read(destination.path().join("file"))?, b"new");
    Ok(())
}

#[test]
fn rejects_truncated_payload_and_alignment_on_streams() -> io::Result<()> {
    let mut archive = Vec::new();
    archive.write_cpio_record(record("file", 0o100644, 1, 5), &mut &b"hello"[..])?;
    // Remove one padding byte. Seeking over it used to hide the truncation.
    archive.pop();
    let mut stream = archive.as_slice();
    let entry = stream.read_cpio_entry()?;
    assert_eq!(
        stream
            .read_cpio_entry_payload(&entry, &mut io::sink())
            .unwrap_err()
            .kind(),
        io::ErrorKind::UnexpectedEof
    );
    Ok(())
}

#[test]
fn crc_archives_validate_checksums_on_read_write_and_extraction() -> io::Result<()> {
    let mut entry = record("crc", 0o100644, 1, 5);
    entry.checksum = Some(532); // ASCII sum of hello.
    let mut archive = Vec::new();
    archive.write_cpio_record(entry, &mut &b"hello"[..])?;
    archive.cpio_close()?;
    assert_eq!(&archive[..6], b"070702");
    let mut reader = archive.as_slice();
    let entry = reader.read_cpio_entry()?;
    assert_eq!(entry.checksum, Some(532));
    let mut data = Vec::new();
    reader.read_cpio_entry_payload(&entry, &mut data)?;
    assert_eq!(data, b"hello");
    let destination = tempdir()?;
    extract_entries(&mut archive.as_slice(), destination.path(), true, false)?;
    assert_eq!(std::fs::read(destination.path().join("crc"))?, b"hello");
    // newc header + 4-byte name padded to 116 bytes.
    archive[116] ^= 1;
    assert_eq!(
        rpm_utils::payload::read_entries(&mut archive.as_slice())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    let destination = tempdir()?;
    assert!(extract_entries(&mut archive.as_slice(), destination.path(), true, false).is_err());
    let mut bad = record("bad", 0o100644, 1, 5);
    bad.checksum = Some(0);
    assert!(
        Vec::new()
            .write_cpio_record(bad, &mut &b"hello"[..])
            .is_err()
    );
    Ok(())
}
