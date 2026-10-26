use rpm_utils::header::{RawHeader, RawTag, Type};
use rpm_utils::payload::extract_entries;
use rpm_utils::{RPMBuilder, RPMEditor, RPMFile, RPMInfo, lint_rpm, repair_rpm};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn package(directory: &Path, compressor: &str) -> io::Result<PathBuf> {
    let source = directory.join("source");
    std::fs::write(&source, b"original content")?;
    let path = directory.join(format!("{compressor}.rpm"));
    RPMBuilder::new()
        .filename(&path)
        .package_name("test")
        .version("1")
        .compression(compressor)
        .add_file_as(source, "/usr/share/test/file")
        .build_package()?;
    Ok(path)
}
fn compressed_payload(path: &Path) -> io::Result<Vec<u8>> {
    let mut rpm = RPMFile::open(path)?;
    rpm.file.seek(SeekFrom::Start(rpm.payload_offset))?;
    let mut bytes = Vec::new();
    rpm.file.read_to_end(&mut bytes)?;
    Ok(bytes)
}
fn header(path: &Path) -> io::Result<RawHeader> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(96))?;
    let start = file.stream_position()?;
    RawHeader::read(&mut file)?;
    let size = file.stream_position()? - start;
    file.seek(SeekFrom::Current(((8 - size % 8) % 8) as i64))?;
    RawHeader::read(&mut file)
}

#[test]
fn metadata_edits_preserve_compressed_payload_unknown_values_and_locales() -> io::Result<()> {
    for compressor in ["gzip", "bzip2", "xz", "lzma", "zstd", "none"] {
        let directory = tempdir()?;
        let input = package(directory.path(), compressor)?;
        assert!(lint_rpm(&input).is_clean(), "{:?}", lint_rpm(&input));
        let original = compressed_payload(&input)?;
        let staged = directory.path().join("staged.rpm");
        let mut editor = RPMEditor::open(&input)?;
        editor.set_tag(RawTag::new(65000, Type::Bin, 4, vec![0, 255, 1, 0])?)?;
        editor.set_tag(RawTag::new(65001, Type::Char, 3, vec![1, 2, 3])?)?;
        editor.set_tag(RawTag::strings_value(
            100,
            Type::StringArray,
            &["C".into(), "uk".into()],
        )?)?;
        for id in [1004, 1005, 1016] {
            editor.set_tag(RawTag::strings_value(
                id,
                Type::I18nstring,
                &["Default".into(), "Українська".into()],
            )?)?;
        }
        editor.save(&staged)?;
        let output = directory.path().join("edited.rpm");
        let mut editor = RPMEditor::open(&staged)?;
        editor.set_text(1004, "Edited summary")?;
        editor.save(&output)?;
        let bytes = std::fs::read(&output)?;
        let signature = RawHeader::read(&mut &bytes[96..])?;
        assert!(signature.get(273).is_some());
        assert!(signature.get(272).is_none());
        assert_eq!(compressed_payload(&output)?, original);
        let raw = header(&output)?;
        assert_eq!(raw.get(65000).unwrap().data, [0, 255, 1, 0]);
        assert_eq!(raw.get(65001).unwrap().data, [1, 2, 3]);
        assert_eq!(raw.strings(1004)?, ["Edited summary", "Українська"]);
        assert!(lint_rpm(&output).is_clean(), "{:?}", lint_rpm(&output));
    }
    Ok(())
}

#[test]
fn replaces_adds_removes_and_changes_permissions_with_valid_integrity() -> io::Result<()> {
    let directory = tempdir()?;
    let input = package(directory.path(), "zstd")?;
    let replacement = directory.path().join("replacement");
    std::fs::write(&replacement, b"new data")?;
    let output = directory.path().join("edited.rpm");
    let mut editor = RPMEditor::open(&input)?;
    editor.replace_file("/usr/share/test/file", &replacement)?;
    editor.add_file(&replacement, "/usr/bin/added")?;
    editor.set_file_mode("/usr/bin/added", 0o755)?;
    let report = editor.save(&output)?;
    assert!(report.payload_changed);
    let mut rpm = RPMFile::open(&output)?;
    rpm.verify_integrity()?;
    let info = RPMInfo::from(&rpm);
    assert_eq!(info.payload.files.len(), 2);
    let mut bytes = Vec::new();
    rpm.copy_payload_to(&mut bytes)?;
    let extraction = directory.path().join("extract");
    std::fs::create_dir(&extraction)?;
    extract_entries(&mut bytes.as_slice(), &extraction, true, false)?;
    assert_eq!(
        std::fs::read(extraction.join("usr/share/test/file"))?,
        b"new data"
    );
    assert_eq!(
        std::fs::read(extraction.join("usr/bin/added"))?,
        b"new data"
    );
    assert_eq!(
        info.payload
            .files
            .iter()
            .find(|file| file.name == "/usr/bin/added")
            .unwrap()
            .mode
            & 0o777,
        0o755
    );
    #[cfg(target_os = "macos")]
    {
        let native = std::process::Command::new("/usr/bin/tar")
            .arg("-xOf")
            .arg(&output)
            .arg("./usr/bin/added")
            .output()?;
        assert!(
            native.status.success(),
            "{}",
            String::from_utf8_lossy(&native.stderr)
        );
        assert_eq!(native.stdout, b"new data");
    }
    let removed = directory.path().join("removed.rpm");
    let mut editor = RPMEditor::open(&output)?;
    editor.remove_file("/usr/share/test/file")?;
    editor.save(&removed)?;
    assert_eq!(
        RPMInfo::from(&RPMFile::open(&removed)?).payload.files.len(),
        1
    );
    assert!(lint_rpm(&removed).is_clean());
    Ok(())
}

#[test]
fn repairs_stale_digest_metadata_but_refuses_missing_identity() -> io::Result<()> {
    let directory = tempdir()?;
    let input = package(directory.path(), "gzip")?;
    let mut bytes = std::fs::read(&input)?;
    let offset = bytes
        .windows(b"test\0".len())
        .rposition(|value| value == b"test\0")
        .unwrap();
    bytes[offset..offset + 4].copy_from_slice(b"edit");
    std::fs::write(&input, &bytes)?;
    let before = lint_rpm(&input);
    assert!(before.has_errors());
    assert!(
        before
            .issues
            .iter()
            .any(|issue| issue.code == "HEADER_DIGEST" && issue.fixable)
    );
    let output = directory.path().join("repaired.rpm");
    assert!(repair_rpm(&input, &output)?.is_clean());
    RPMFile::open(&output)?.verify_integrity()?;
    // Empty package identity cannot be reconstructed from the payload.
    let name = bytes
        .windows(b"edit\0".len())
        .rposition(|value| value == b"edit\0")
        .unwrap();
    bytes[name] = 0;
    std::fs::write(&input, bytes)?;
    std::fs::write(&output, b"keep this")?;
    assert!(repair_rpm(&input, &output).is_err());
    assert_eq!(std::fs::read(&output)?, b"keep this");
    Ok(())
}

#[test]
fn clean_repair_is_byte_identical_and_invalid_edits_preserve_output() -> io::Result<()> {
    let directory = tempdir()?;
    let input = package(directory.path(), "xz")?;
    let output = directory.path().join("copy.rpm");
    repair_rpm(&input, &output)?;
    assert_eq!(std::fs::read(&input)?, std::fs::read(&output)?);
    let original = std::fs::read(&output)?;
    let mut editor = RPMEditor::open(&input)?;
    assert!(
        editor
            .add_file(directory.path().join("source"), "../escape")
            .is_err()
    );
    assert!(editor.remove_tag(1125).is_err());
    editor.set_text(1000, "")?;
    assert!(editor.save(&output).is_err());
    assert_eq!(std::fs::read(&output)?, original);
    Ok(())
}

#[cfg(feature = "cli")]
#[test]
fn editing_and_linting_clis_support_json_and_in_place_repair() -> io::Result<()> {
    let directory = tempdir()?;
    let input = package(directory.path(), "gzip")?;
    let output = directory.path().join("cli.rpm");
    let edit = std::process::Command::new(env!("CARGO_BIN_EXE_rpm-edit"))
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .args(["--set", "summary=CLI edit", "--epoch", "1000", "--json"])
        .output()?;
    assert!(
        edit.status.success(),
        "{}",
        String::from_utf8_lossy(&edit.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&edit.stdout)?;
    assert_eq!(json["payload_changed"], false);
    let lint = std::process::Command::new(env!("CARGO_BIN_EXE_rpm-lint"))
        .arg(&output)
        .arg("--json")
        .output()?;
    assert!(
        lint.status.success(),
        "{}",
        String::from_utf8_lossy(&lint.stdout)
    );
    assert!(
        serde_json::from_slice::<serde_json::Value>(&lint.stdout)?["issues"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut bytes = std::fs::read(&output)?;
    let offset = bytes
        .windows(b"CLI edit\0".len())
        .position(|value| value == b"CLI edit\0")
        .unwrap();
    bytes[offset] = b'X';
    std::fs::write(&output, bytes)?;
    let fix = std::process::Command::new(env!("CARGO_BIN_EXE_rpm-lint"))
        .arg(&output)
        .args(["--fix", "--in-place"])
        .output()?;
    assert!(
        fix.status.success(),
        "{}",
        String::from_utf8_lossy(&fix.stderr)
    );
    RPMFile::open(output)?.verify_integrity()?;
    Ok(())
}

fn variant(
    input: &Path,
    output: &Path,
    change: impl FnOnce(&mut RawHeader, &mut RawHeader) -> io::Result<()>,
) -> io::Result<()> {
    use std::io::Write;
    let mut source = std::fs::File::open(input)?;
    let mut lead = [0; 96];
    source.read_exact(&mut lead)?;
    let start = source.stream_position()?;
    let mut signature = RawHeader::read(&mut source)?;
    let size = source.stream_position()? - start;
    source.seek(SeekFrom::Current(((8 - size % 8) % 8) as i64))?;
    let mut metadata = RawHeader::read(&mut source)?;
    let mut payload = Vec::new();
    source.read_to_end(&mut payload)?;
    change(&mut signature, &mut metadata)?;
    let signature = signature.encode_region(62)?;
    let mut destination = std::fs::File::create(output)?;
    destination.write_all(&lead)?;
    destination.write_all(&signature)?;
    destination.write_all(&[0; 7][..(8 - signature.len() % 8) % 8])?;
    destination.write_all(&metadata.encode_region(63)?)?;
    destination.write_all(&payload)?;
    Ok(())
}

#[test]
fn signed_inputs_are_copied_without_changes_or_explicitly_reported_unsigned_after_edit()
-> io::Result<()> {
    let directory = tempdir()?;
    let input = package(directory.path(), "gzip")?;
    let signed = directory.path().join("signed.rpm");
    variant(&input, &signed, |signature, _| {
        signature.set(RawTag::new(268, Type::Bin, 3, vec![1, 2, 3])?)
    })?;
    let lint = lint_rpm(&signed);
    assert!(!lint.has_errors(), "{lint:?}");
    assert!(
        lint.issues
            .iter()
            .any(|issue| issue.code == "SIGNATURE_NOT_VERIFIED")
    );
    let copy = directory.path().join("copy.rpm");
    repair_rpm(&signed, &copy)?;
    assert_eq!(std::fs::read(&signed)?, std::fs::read(&copy)?);
    let output = directory.path().join("unsigned.rpm");
    let mut editor = RPMEditor::open(&signed)?;
    editor.set_text(1004, "Changed")?;
    let report = editor.save(&output)?;
    assert_eq!(report.removed_signatures, [268]);
    assert!(
        RPMFile::open(&output)?
            .signature_tags
            .get_value(rpm_utils::header::SignatureTag::RSAHeader)
            .is_none()
    );
    RPMFile::open(output)?.verify_integrity()?;
    Ok(())
}

#[test]
fn repairs_file_sizes_and_digests_without_guessing_conflicting_modes() -> io::Result<()> {
    let directory = tempdir()?;
    let input = package(directory.path(), "none")?;
    let damaged = directory.path().join("damaged.rpm");
    variant(&input, &damaged, |_, header| {
        header.set(RawTag::new(
            1028,
            Type::Int32,
            1,
            9999_u32.to_be_bytes().to_vec(),
        )?)?;
        header.set(RawTag::strings_value(
            1035,
            Type::StringArray,
            &["0".repeat(64)],
        )?)
    })?;
    let report = lint_rpm(&damaged);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == "FILE_SIZE" && issue.fixable)
    );
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == "FILE_DIGEST" && issue.fixable)
    );
    let output = directory.path().join("repaired.rpm");
    assert!(repair_rpm(&damaged, &output)?.is_clean());
    variant(&input, &damaged, |_, header| {
        header.set(RawTag::new(
            1030,
            Type::Int16,
            1,
            0o100777_u16.to_be_bytes().to_vec(),
        )?)
    })?;
    let old = std::fs::read(&output)?;
    assert!(
        lint_rpm(&damaged)
            .issues
            .iter()
            .any(|issue| issue.code == "FILE_MODE" && !issue.fixable)
    );
    assert!(repair_rpm(&damaged, &output).is_err());
    assert_eq!(std::fs::read(&output)?, old);
    Ok(())
}

#[cfg(unix)]
#[test]
fn replacing_one_hardlink_breaks_that_link_without_changing_the_other() -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let directory = tempdir()?;
    let tree = directory.path().join("tree");
    std::fs::create_dir(&tree)?;
    std::fs::write(tree.join("first"), b"original")?;
    std::fs::hard_link(tree.join("first"), tree.join("last"))?;
    let input = directory.path().join("linked.rpm");
    RPMBuilder::new()
        .filename(&input)
        .package_name("links")
        .version("1")
        .add_file_as(&tree, "/opt/tree")
        .build_package()?;
    assert_eq!(RPMInfo::from(&RPMFile::open(&input)?).size, 8);
    assert!(lint_rpm(&input).is_clean(), "{:?}", lint_rpm(&input));
    let source = directory.path().join("replacement");
    std::fs::write(&source, b"new")?;
    let output = directory.path().join("edited.rpm");
    let mut editor = RPMEditor::open(&input)?;
    editor.replace_file("/opt/tree/last", source)?;
    editor.save(&output)?;
    assert_eq!(RPMInfo::from(&RPMFile::open(&output)?).size, 11);
    let mut payload = Vec::new();
    RPMFile::open(&output)?.copy_payload_to(&mut payload)?;
    let target = directory.path().join("extract");
    std::fs::create_dir(&target)?;
    extract_entries(&mut payload.as_slice(), &target, true, false)?;
    assert_eq!(std::fs::read(target.join("opt/tree/first"))?, b"original");
    assert_eq!(std::fs::read(target.join("opt/tree/last"))?, b"new");
    assert_ne!(
        std::fs::metadata(target.join("opt/tree/first"))?.ino(),
        std::fs::metadata(target.join("opt/tree/last"))?.ino()
    );
    Ok(())
}

#[test]
fn removing_the_last_file_produces_a_valid_empty_package() -> io::Result<()> {
    let directory = tempdir()?;
    let input = package(directory.path(), "bzip2")?;
    let output = directory.path().join("empty.rpm");
    let mut editor = RPMEditor::open(input)?;
    editor.remove_file("/usr/share/test/file")?;
    editor.save(&output)?;
    assert!(
        RPMInfo::from(&RPMFile::open(&output)?)
            .payload
            .files
            .is_empty()
    );
    assert!(lint_rpm(output).is_clean());
    Ok(())
}

#[test]
fn migrates_legacy_file_names_without_losing_entries() -> io::Result<()> {
    let directory = tempdir()?;
    let input = package(directory.path(), "gzip")?;
    let legacy = directory.path().join("legacy.rpm");
    variant(&input, &legacy, |_, header| {
        header.set(RawTag::strings_value(
            1027,
            Type::StringArray,
            &["/usr/share/test/file".into()],
        )?)?;
        for id in [1116, 1117, 1118] {
            header.remove(id);
        }
        Ok(())
    })?;
    let output = directory.path().join("modern.rpm");
    let mut editor = RPMEditor::open(legacy)?;
    editor.set_text(1004, "Updated")?;
    editor.save(&output)?;
    let metadata = header(&output)?;
    assert!(metadata.get(1027).is_none());
    assert_eq!(metadata.strings(1117)?, ["file"]);
    assert!(lint_rpm(output).is_clean());
    Ok(())
}

#[cfg(unix)]
#[test]
fn chmod_applies_to_all_members_of_a_hardlink_group() -> io::Result<()> {
    let directory = tempdir()?;
    let tree = directory.path().join("tree");
    std::fs::create_dir(&tree)?;
    std::fs::write(tree.join("first"), b"data")?;
    std::fs::hard_link(tree.join("first"), tree.join("second"))?;
    let input = directory.path().join("input.rpm");
    RPMBuilder::new()
        .filename(&input)
        .package_name("modes")
        .version("1")
        .add_file_as(&tree, "/opt/tree")
        .build_package()?;
    let output = directory.path().join("output.rpm");
    let mut editor = RPMEditor::open(input)?;
    editor.set_file_mode("/opt/tree/first", 0o444)?;
    editor.save(&output)?;
    for file in RPMInfo::from(&RPMFile::open(&output)?).payload.files {
        if file.mode & 0o170000 == 0o100000 {
            assert_eq!(file.mode & 0o777, 0o444);
        }
    }
    assert!(lint_rpm(output).is_clean());
    Ok(())
}

#[test]
fn malformed_inputs_have_structured_non_fixable_diagnostics() -> io::Result<()> {
    let directory = tempdir()?;
    let input = directory.path().join("bad.rpm");
    std::fs::write(&input, b"not an RPM")?;
    let before = std::fs::read(&input)?;
    let report = lint_rpm(&input);
    assert!(report.has_errors());
    assert!(report.issues.iter().all(|issue| !issue.fixable));
    assert!(repair_rpm(&input, &input).is_err());
    assert_eq!(std::fs::read(input)?, before);
    Ok(())
}

#[test]
fn refuses_to_guess_how_unknown_arrays_follow_changed_file_lists() -> io::Result<()> {
    let directory = tempdir()?;
    let input = package(directory.path(), "gzip")?;
    let unknown = directory.path().join("unknown.rpm");
    let mut editor = RPMEditor::open(input)?;
    editor.set_tag(RawTag::strings_value(
        65000,
        Type::StringArray,
        &["custom file annotation".into()],
    )?)?;
    editor.save(&unknown)?;
    let output = directory.path().join("output.rpm");
    std::fs::write(&output, b"preserve")?;
    let mut editor = RPMEditor::open(&unknown)?;
    editor.remove_file("/usr/share/test/file")?;
    assert_eq!(
        editor.save(&output).unwrap_err().kind(),
        io::ErrorKind::Unsupported
    );
    assert_eq!(std::fs::read(&output)?, b"preserve");
    let mut editor = RPMEditor::open(unknown)?;
    editor
        .remove_tag(65000)?
        .remove_file("/usr/share/test/file")?;
    editor.save(&output)?;
    assert!(lint_rpm(output).is_clean());
    Ok(())
}

#[test]
fn edits_packages_that_own_the_root_directory() -> io::Result<()> {
    use rpm_utils::payload::{CpioWriter, FileEntry};
    let directory = tempdir()?;
    let tree = directory.path().join("root");
    std::fs::create_dir(&tree)?;
    let original = directory.path().join("original.rpm");
    let rpm = RPMBuilder::new()
        .filename(&original)
        .package_name("root-owner")
        .version("1")
        .compression("none")
        .add_file_as(&tree, "/root")
        .build_package()?;
    let info = RPMInfo::from(&rpm).payload.files.remove(0);
    let input = directory.path().join("root.rpm");
    variant(&original, &input, |_, header| {
        header.set(RawTag::strings_value(
            1117,
            Type::StringArray,
            &["".into()],
        )?)?;
        header.set(RawTag::strings_value(
            1118,
            Type::StringArray,
            &["/".into()],
        )?)
    })?;
    let mut archive = Vec::new();
    archive.write_cpio_record(
        FileEntry {
            name: ".".into(),
            mode: info.mode.into(),
            mtime: info.mtime,
            ino: info.inode,
            nlink: 2,
            ..Default::default()
        },
        &mut io::empty(),
    )?;
    archive.cpio_close()?;
    let offset = RPMFile::open(&input)?.payload_offset;
    let mut bytes = std::fs::read(&input)?;
    bytes.truncate(offset as usize);
    bytes.extend_from_slice(&archive);
    std::fs::write(&input, bytes)?;
    let output = directory.path().join("edited.rpm");
    let mut editor = RPMEditor::open(input)?;
    editor.set_text(1004, "Root directory owner")?;
    editor.save(&output)?;
    assert_eq!(header(&output)?.strings(1117)?, [""]);
    assert!(lint_rpm(output).is_clean());
    Ok(())
}

#[cfg(unix)]
#[test]
fn in_place_edits_preserve_output_permissions_and_payload() -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempdir()?;
    let input = package(directory.path(), "gzip")?;
    std::fs::set_permissions(&input, std::fs::Permissions::from_mode(0o644))?;
    let before = compressed_payload(&input)?;
    let mut editor = RPMEditor::open(&input)?;
    editor.set_text(1004, "In-place edit")?;
    editor.save(&input)?;
    assert_eq!(
        std::fs::metadata(&input)?.permissions().mode() & 0o777,
        0o644
    );
    assert_eq!(compressed_payload(&input)?, before);
    assert!(lint_rpm(input).is_clean());
    Ok(())
}

#[test]
fn excessive_file_counts_are_reported_before_expanding_file_names() -> io::Result<()> {
    let directory = tempdir()?;
    let input = package(directory.path(), "gzip")?;
    let bad = directory.path().join("large-table.rpm");
    variant(&input, &bad, |_, header| {
        header.set(RawTag::new(
            1117,
            Type::StringArray,
            262145,
            vec![0; 262145],
        )?)
    })?;
    let report = lint_rpm(&bad);
    assert!(report.issues.iter().any(|issue| issue.code == "FILE_TABLE"
        && !issue.fixable
        && issue.message.contains("resource")));
    assert!(RPMEditor::open(bad).is_err());
    Ok(())
}

#[test]
fn edit_previews_match_real_saves_without_modifying_input() -> io::Result<()> {
    for compressor in ["gzip", "bzip2", "xz", "lzma", "zstd", "none"] {
        let directory = tempdir()?;
        let input = package(directory.path(), compressor)?;
        let original = std::fs::read(&input)?;
        let replacement = directory.path().join("replacement");
        std::fs::write(&replacement, b"preview contents")?;
        let configure = || -> io::Result<RPMEditor> {
            let mut editor = RPMEditor::open(&input)?;
            editor.set_text(1004, "Preview summary")?;
            editor.replace_file("/usr/share/test/file", &replacement)?;
            editor.set_file_mode("/usr/share/test/file", 0o700)?;
            Ok(editor)
        };
        let preview = configure()?.preview()?;
        assert!(preview.payload_changed);
        assert_eq!(std::fs::read(&input)?, original);
        let output = directory.path().join("saved.rpm");
        let saved = configure()?.save(&output)?;
        let changes = rpm_utils::diff_rpm(&input, &output)?;
        assert_eq!(
            serde_json::to_value(&preview.changes)?,
            serde_json::to_value(changes)?
        );
        assert_eq!(preview.removed_signatures, saved.removed_signatures);
        assert_eq!(preview.changes.files.len(), 1);
        assert_eq!(std::fs::read(&input)?, original);
    }
    Ok(())
}

#[test]
fn metadata_preview_reports_signature_removal_without_changing_signed_input() -> io::Result<()> {
    let directory = tempdir()?;
    let input = package(directory.path(), "gzip")?;
    let signed = directory.path().join("signed.rpm");
    variant(&input, &signed, |signature, _| {
        signature.set(RawTag::new(268, Type::Bin, 3, vec![1, 2, 3])?)
    })?;
    let original = std::fs::read(&signed)?;
    let mut editor = RPMEditor::open(&signed)?;
    editor.set_text(1004, "Changed")?;
    let preview = editor.preview()?;
    assert!(!preview.payload_changed);
    assert_eq!(preview.removed_signatures, [268]);
    assert!(preview.changes.files.is_empty());
    assert!(
        preview
            .changes
            .signatures
            .iter()
            .any(|tag| tag.id == 268 && tag.after.is_none())
    );
    assert_eq!(std::fs::read(&signed)?, original);
    Ok(())
}

#[cfg(feature = "cli")]
#[test]
fn dry_run_cli_never_writes_requested_paths_and_validates_results() -> io::Result<()> {
    use std::process::Command;
    let directory = tempdir()?;
    let input = package(directory.path(), "none")?;
    let original = std::fs::read(&input)?;
    let output = directory.path().join("output.rpm");
    std::fs::write(&output, b"preserve destination")?;
    let bin = env!("CARGO_BIN_EXE_rpm-edit");
    let preview = Command::new(bin)
        .arg(&input)
        .args(["--dry-run", "--json", "--set", "summary=preview"])
        .arg("--output")
        .arg(&output)
        .output()?;
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&preview.stdout)?;
    assert_eq!(value["payload_changed"], false);
    assert!(
        value["changes"]["metadata"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tag| tag["id"] == 1004)
    );
    assert_eq!(std::fs::read(&output)?, b"preserve destination");
    let in_place = Command::new(bin)
        .arg(&input)
        .args(["--dry-run", "--in-place", "--set", "summary=preview"])
        .output()?;
    assert!(in_place.status.success());
    let no_output = Command::new(bin)
        .arg(&input)
        .args(["--dry-run", "--set", "summary=preview"])
        .output()?;
    assert!(no_output.status.success());
    let missing_output = directory.path().join("never-created.rpm");
    let failure = Command::new(bin)
        .arg(&input)
        .args(["--dry-run", "--set", "name="])
        .arg("--output")
        .arg(&missing_output)
        .output()?;
    assert!(!failure.status.success());
    assert!(!missing_output.exists());
    assert_eq!(std::fs::read(&input)?, original);
    assert_eq!(std::fs::read(&output)?, b"preserve destination");
    Ok(())
}
