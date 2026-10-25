use rpm_utils::{PackageTarget, RPMBuilder, RPMInfo, payload::CpioRead};
use std::io;

#[test]
fn linux_targets_and_aliases_are_explicit_and_host_independent() -> io::Result<()> {
    for (name, arch) in [
        ("amd64", "x86_64"),
        ("arm64", "aarch64"),
        ("aarch64-unknown-linux-musl", "aarch64"),
        ("x86_64-linux-gnu", "x86_64"),
        ("powerpc64le-unknown-linux-gnu", "ppc64le"),
        ("noarch", "noarch"),
    ] {
        let target: PackageTarget = name.parse()?;
        assert_eq!(target.arch(), arch);
        assert_eq!(target.os(), "linux");
    }
    for bad in [
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
        "unknown",
        "x86_64-unknown-none",
    ] {
        assert!(bad.parse::<PackageTarget>().is_err());
    }
    Ok(())
}
#[test]
fn checks_elf_machine_endianness_and_native_noarch_content() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("binary");
    let mut elf = [0u8; 64];
    elf[..4].copy_from_slice(b"\x7fELF");
    elf[4] = 2;
    elf[5] = 1;
    elf[18] = 183;
    std::fs::write(&file, elf)?;
    "aarch64".parse::<PackageTarget>()?.validate_file(&file)?;
    assert!(
        "x86_64"
            .parse::<PackageTarget>()?
            .validate_file(&file)
            .is_err()
    );
    assert!(
        "noarch"
            .parse::<PackageTarget>()?
            .validate_file(&file)
            .is_err()
    );
    elf[5] = 2;
    std::fs::write(&file, elf)?;
    assert!(
        "aarch64"
            .parse::<PackageTarget>()?
            .validate_file(&file)
            .is_err()
    );
    for magic in [
        vec![0xcf, 0xfa, 0xed, 0xfe],
        b"MZbinary".to_vec(),
        vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 2],
    ] {
        std::fs::write(&file, magic)?;
        assert!(
            "aarch64"
                .parse::<PackageTarget>()?
                .validate_file(&file)
                .is_err()
        );
    }
    for data in [
        b"PK\x03\x04example.jar".as_slice(),
        &[0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 61],
        b"#!/bin/sh\ntrue",
    ] {
        std::fs::write(&file, data)?;
        "noarch".parse::<PackageTarget>()?.validate_file(&file)?;
    }
    Ok(())
}
#[test]
fn builds_noarch_jvm_packages_with_target_ownership_and_preserves_failed_output() -> io::Result<()>
{
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("app.jar");
    std::fs::write(&source, b"PK\x03\x04jar contents")?;
    let output = dir.path().join("app.rpm");
    let mut rpm = RPMBuilder::new()
        .filename(&output)
        .package_name("jvm-app")
        .version("1")
        .target("noarch")?
        .add_file_as(&source, "/usr/share/app/app.jar")
        .build_package()?;
    assert_eq!(RPMInfo::from(&rpm).arch, "noarch");
    rpm.verify_integrity()?;
    let mut bytes = Vec::new();
    rpm.copy_payload_to(&mut bytes)?;
    let mut payload = std::io::Cursor::new(bytes);
    let entry = payload.read_cpio_entry()?;
    assert_eq!((entry.uid, entry.gid), (0, 0));
    drop(payload);

    let original = std::fs::read(&output)?;
    std::fs::write(&source, b"\x7fELFtruncated")?;
    assert!(
        RPMBuilder::new()
            .filename(&output)
            .package_name("jvm-app")
            .version("1")
            .target("noarch")?
            .add_file_as(&source, "/app.jar")
            .build_package()
            .is_err()
    );
    assert_eq!(std::fs::read(output)?, original);
    Ok(())
}

#[test]
fn reproducible_packages_ignore_host_timestamps_and_external_hardlinks() -> io::Result<()> {
    let dir = tempfile::tempdir()?;
    let source = dir.path().join("source");
    std::fs::write(&source, b"repeatable")?;
    let mut originals = Vec::new();
    for compressor in ["gzip", "bzip2", "xz", "lzma", "zstd", "none"] {
        let path = dir.path().join(format!("{compressor}.rpm"));
        RPMBuilder::new()
            .filename(&path)
            .package_name("repeatable")
            .version("1")
            .target("noarch")?
            .compression(compressor)
            .reproducible(1_700_000_000)
            .file_mode("/data", 0o644)?
            .add_file_as(&source, "/data")
            .build_package()?;
        originals.push((compressor, std::fs::read(path)?));
    }
    filetime::set_file_mtime(
        &source,
        filetime::FileTime::from_unix_time(1_800_000_000, 0),
    )?;
    std::fs::hard_link(&source, dir.path().join("external-link"))?;
    for (compressor, original) in originals {
        let path = dir.path().join(format!("{compressor}.rpm"));
        RPMBuilder::new()
            .filename(&path)
            .package_name("repeatable")
            .version("1")
            .target("noarch")?
            .compression(compressor)
            .reproducible(1_700_000_000)
            .file_mode("/data", 0o644)?
            .add_file_as(&source, "/data")
            .build_package()?;
        assert_eq!(std::fs::read(path)?, original, "{compressor}");
    }
    Ok(())
}
