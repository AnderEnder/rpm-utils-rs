use rpm_utils::{PackageManifest, RPMFile, RPMInfo, header::Tag, payload::CpioRead};
use std::{io, path::Path, process::Command};

fn manifest(dir: &Path) -> io::Result<std::path::PathBuf> {
    std::fs::write(dir.join("app.jar"), b"PK\x03\x04jvm app")?;
    let path = dir.join("rpm.toml");
    std::fs::write(
        &path,
        r#"
name = "jvm-app"
version = "1.2"
target = "noarch"
requires = ["java-headless"]
source-date-epoch = 1700000000
[[files]]
source = "app.jar"
dest = "/usr/share/jvm-app/app.jar"
mode = "0644"
"#,
    )?;
    Ok(path)
}
#[test]
fn manifests_build_repeatable_noarch_packages_with_runtime_requirements() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = manifest(dir.path())?;
    let spec = PackageManifest::from_toml(&std::fs::read_to_string(path)?)?;
    let output = spec.build(dir.path(), None)?;
    let first = std::fs::read(&output)?;
    spec.build(dir.path(), None)?;
    assert_eq!(std::fs::read(&output)?, first);
    let mut rpm = RPMFile::open(output)?;
    rpm.verify_integrity()?;
    assert_eq!(RPMInfo::from(&rpm).arch, "noarch");
    assert!(format!("{:?}", rpm.header_tags.get_value(Tag::RequireName)).contains("java-headless"));
    let mut payload = Vec::new();
    rpm.copy_payload_to(&mut payload)?;
    let entry = std::io::Cursor::new(payload).read_cpio_entry()?;
    assert_eq!(
        (entry.uid, entry.gid, entry.mode & 0o7777, entry.mtime),
        (0, 0, 0o644, 1700000000)
    );
    assert!(PackageManifest::from_toml("typo = true").is_err());
    Ok(())
}
#[test]
fn rpm_build_resolves_relative_paths_and_rejects_invalid_source_date_epoch() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let manifest = manifest(dir.path())?;
    let output = dir.path().join("built.rpm");
    let result = Command::new(env!("CARGO_BIN_EXE_rpm-build"))
        .arg("--manifest")
        .arg(&manifest)
        .arg("--output")
        .arg(&output)
        .output()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let original = std::fs::read(&output)?;
    let text = std::fs::read_to_string(&manifest)?.replace("source-date-epoch = 1700000000", "");
    std::fs::write(&manifest, text)?;
    let result = Command::new(env!("CARGO_BIN_EXE_rpm-build"))
        .arg("--manifest")
        .arg(&manifest)
        .arg("--output")
        .arg(&output)
        .env("SOURCE_DATE_EPOCH", "invalid")
        .output()?;
    assert!(!result.status.success());
    assert_eq!(std::fs::read(output)?, original);
    Ok(())
}
fn cargo_project(dir: &Path) -> io::Result<()> {
    std::fs::create_dir(dir.join("src"))?;
    std::fs::write(dir.join("src/main.rs"), "fn main() {}")?;
    std::fs::write(dir.join("app.jar"), b"PK\x03\x04cargo assets")?;
    std::fs::write(
        dir.join("Cargo.toml"),
        r#"
[package]
name = "cargo-rpm-fixture"
version = "2.3.4"
edition = "2024"
[workspace]
[package.metadata.rpm]
target = "noarch"
source-date-epoch = 1700000000
[[package.metadata.rpm.files]]
source = "app.jar"
dest = "/usr/share/fixture/app.jar"
mode = "0644"
"#,
    )?;
    Ok(())
}
#[test]
fn own_cargo_subcommand_builds_and_packages_metadata_assets() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    cargo_project(dir.path())?;
    let output = dir.path().join("cargo.rpm");
    let bin = Path::new(env!("CARGO_BIN_EXE_cargo-rpm"));
    let mut paths = vec![bin.parent().unwrap().to_owned()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let result = Command::new(env!("CARGO"))
        .args(["rpm", "--offline"])
        .arg("--manifest-path")
        .arg(dir.path().join("Cargo.toml"))
        .arg("--output")
        .arg(&output)
        .env("PATH", std::env::join_paths(paths).unwrap())
        .env("CARGO_TARGET_DIR", dir.path().join("cargo-target"))
        .output()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        dir.path()
            .join("cargo-target/release/cargo-rpm-fixture")
            .with_extension(std::env::consts::EXE_EXTENSION)
            .exists()
    );
    let mut rpm = RPMFile::open(output)?;
    rpm.verify_integrity()?;
    let info = RPMInfo::from(&rpm);
    assert_eq!(info.name, "cargo-rpm-fixture");
    assert_eq!(info.version, "2.3.4");
    assert_eq!(info.payload.files[0].name, "/usr/share/fixture/app.jar");
    Ok(())
}
#[test]
fn cargo_packaging_without_build_and_target_conflicts_preserve_output() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    cargo_project(dir.path())?;
    let output = dir.path().join("output.rpm");
    let bin = env!("CARGO_BIN_EXE_cargo-rpm");
    let invoke = |extra: &[&str]| {
        Command::new(bin)
            .args(["rpm", "--no-build", "--offline"])
            .arg("--manifest-path")
            .arg(dir.path().join("Cargo.toml"))
            .arg("--output")
            .arg(&output)
            .args(extra)
            .output()
    };
    let result = invoke(&[])?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let original = std::fs::read(&output)?;
    let result = invoke(&["--target", "aarch64-unknown-linux-musl"])?;
    assert!(!result.status.success());
    assert_eq!(std::fs::read(&output)?, original);
    assert!(!dir.path().join("target/release").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn standalone_build_commands_receive_target_environment_and_fail_atomically() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let manifest = manifest(dir.path())?;
    let output = dir.path().join("output.rpm");
    let result = Command::new(env!("CARGO_BIN_EXE_rpm-build"))
        .arg("--manifest").arg(&manifest).arg("--output").arg(&output)
        .args(["--build-env", "EXAMPLE_VALUE=with spaces", "--", "sh", "-c",
            "test \"$RPM_TARGET_ARCH\" = noarch && test \"$RPM_TARGET_OS\" = linux && test \"$SOURCE_DATE_EPOCH\" = 1700000000 && test \"$EXAMPLE_VALUE\" = 'with spaces' && printf 'built jar' > app.jar"])
        .output()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let original = std::fs::read(&output)?;
    let result = Command::new(env!("CARGO_BIN_EXE_rpm-build"))
        .arg("--manifest")
        .arg(&manifest)
        .arg("--output")
        .arg(&output)
        .args(["--", "sh", "-c", "exit 9"])
        .output()?;
    assert!(!result.status.success());
    assert_eq!(std::fs::read(output)?, original);
    Ok(())
}

#[test]
fn cargo_workspaces_require_selection_and_resolve_member_paths() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    for name in ["one", "two"] {
        let member = dir.path().join(name);
        std::fs::create_dir(&member)?;
        cargo_project(&member)?;
        let manifest = member.join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest)?
            .replace(
                "name = \"cargo-rpm-fixture\"",
                &format!("name = \"{name}\""),
            )
            .replace("[workspace]\n", "");
        std::fs::write(manifest, text)?;
    }
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = ['one', 'two']\nresolver = '3'\n",
    )?;
    let output = dir.path().join("member.rpm");
    let result = Command::new(env!("CARGO_BIN_EXE_cargo-rpm"))
        .args(["rpm", "--no-build", "--offline", "--package", "two"])
        .arg("--manifest-path")
        .arg(dir.path().join("Cargo.toml"))
        .arg("--output")
        .arg(&output)
        .output()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(RPMInfo::from(&RPMFile::open(&output)?).name, "two");
    let result = Command::new(env!("CARGO_BIN_EXE_cargo-rpm"))
        .args(["rpm", "--no-build", "--offline"])
        .arg("--manifest-path")
        .arg(dir.path().join("Cargo.toml"))
        .arg("--output")
        .arg(&output)
        .output()?;
    assert!(!result.status.success());
    assert_eq!(RPMInfo::from(&RPMFile::open(&output)?).name, "two");
    Ok(())
}
