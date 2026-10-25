use rpm_utils::{
    RPMBuilder, RPMEditor, diff_rpm,
    header::{RawTag, Type},
};
use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
};

fn package(dir: &Path) -> io::Result<PathBuf> {
    let source = dir.join("source");
    std::fs::write(&source, b"unique original contents")?;
    let rpm = dir.join("input.rpm");
    RPMBuilder::new()
        .filename(&rpm)
        .package_name("diff-test")
        .version("1")
        .compression("none")
        .add_file_as(&source, "/usr/share/test")
        .build_package()?;
    Ok(rpm)
}
#[test]
fn compares_unknown_metadata_and_file_operations_in_path_order() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let input = package(dir.path())?;
    assert!(diff_rpm(&input, &input)?.is_empty());
    let output = dir.path().join("output.rpm");
    let mut editor = RPMEditor::open(&input)?;
    editor.set_tag(RawTag::new(65000, Type::Bin, 2, vec![0, 255])?)?;
    editor.set_text(1004, "a different summary")?;
    editor.add_file(dir.path().join("source"), "/added")?;
    editor.remove_file("/usr/share/test")?;
    editor.save(&output)?;
    let report = diff_rpm(&input, &output)?;
    assert_eq!(
        report
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        ["/added", "/usr/share/test"]
    );
    assert!(report.files[0].before.is_none());
    assert!(report.files[1].after.is_none());
    let opaque = report.metadata.iter().find(|tag| tag.id == 65000).unwrap();
    assert_eq!(opaque.after.as_ref().unwrap().data_hex, "00ff");
    assert!(opaque.before.is_none());
    let reverse = diff_rpm(&output, &input)?;
    assert!(reverse.files[0].after.is_none());
    assert!(!report.signatures.is_empty());
    Ok(())
}
#[test]
fn hashes_actual_content_even_when_header_digests_are_unchanged() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let input = package(dir.path())?;
    let modified = dir.path().join("tampered.rpm");
    let mut bytes = std::fs::read(&input)?;
    let at = bytes
        .windows(b"unique original contents".len())
        .position(|v| v == b"unique original contents")
        .unwrap();
    bytes[at] ^= 1;
    std::fs::write(&modified, bytes)?;
    let report = diff_rpm(&input, &modified)?;
    assert!(report.metadata.is_empty());
    assert!(report.signatures.is_empty());
    assert_eq!(report.files.len(), 1);
    let file = &report.files[0];
    assert_eq!(
        file.before.as_ref().unwrap().metadata,
        file.after.as_ref().unwrap().metadata
    );
    assert_ne!(
        file.before
            .as_ref()
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .sha256,
        file.after
            .as_ref()
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .sha256
    );
    Ok(())
}
#[test]
fn cli_reports_equal_different_and_error_statuses() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let input = package(dir.path())?;
    let bin = env!("CARGO_BIN_EXE_rpm-diff");
    let equal = Command::new(bin)
        .arg(&input)
        .arg(&input)
        .arg("--json")
        .output()?;
    assert!(equal.status.success());
    let report: serde_json::Value = serde_json::from_slice(&equal.stdout)?;
    assert_eq!(report["files"], serde_json::json!([]));
    let changed = dir.path().join("changed.rpm");
    let mut editor = RPMEditor::open(&input)?;
    editor.set_file_mode("/usr/share/test", 0o700)?;
    editor.save(&changed)?;
    let different = Command::new(bin)
        .arg(&input)
        .arg(&changed)
        .arg("--json")
        .output()?;
    assert_eq!(different.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&different.stdout)?;
    assert_eq!(report["files"][0]["path"], "/usr/share/test");
    assert_eq!(report["files"][0]["after"]["content"]["mode"], 0o100700);
    let error = Command::new(bin)
        .arg(&input)
        .arg(dir.path().join("missing"))
        .output()?;
    assert_eq!(error.status.code(), Some(2));
    Ok(())
}
#[cfg(unix)]
#[test]
fn compares_symlink_targets_and_hardlink_membership() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    let link = dir.path().join("link");
    std::fs::write(&first, b"linked")?;
    std::fs::hard_link(&first, &second)?;
    std::os::unix::fs::symlink("first", &link)?;
    let input = dir.path().join("links.rpm");
    RPMBuilder::new()
        .filename(&input)
        .package_name("links")
        .version("1")
        .add_file_as(&first, "/first")
        .add_file_as(&second, "/second")
        .add_file_as(&link, "/link")
        .build_package()?;
    let output = dir.path().join("edited.rpm");
    let mut editor = RPMEditor::open(&input)?;
    editor.replace_file("/first", &first)?;
    editor.save(&output)?;
    let report = diff_rpm(&input, &output)?;
    assert!(report.files.iter().any(|f| f.path == "/first"));
    assert!(report.files.iter().any(|f| f.path == "/second"));
    assert!(!report.files.iter().any(|f| f.path == "/link"));
    let second_change = report.files.iter().find(|f| f.path == "/second").unwrap();
    assert_ne!(
        second_change
            .before
            .as_ref()
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .hardlink_group,
        second_change
            .after
            .as_ref()
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .hardlink_group
    );
    std::fs::remove_file(&link)?;
    std::os::unix::fs::symlink("second", &link)?;
    let relinked = dir.path().join("relinked.rpm");
    RPMBuilder::new()
        .filename(&relinked)
        .package_name("links")
        .version("1")
        .add_file_as(&first, "/first")
        .add_file_as(&second, "/second")
        .add_file_as(&link, "/link")
        .build_package()?;
    let report = diff_rpm(&input, &relinked)?;
    assert!(report.files.iter().any(|f| f.path == "/link"));
    Ok(())
}
