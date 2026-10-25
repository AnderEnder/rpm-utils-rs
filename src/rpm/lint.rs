//! Structural, metadata, payload and integrity diagnostics with conservative repair.
use super::package::*;
use crate::header::{RawHeader, RawTag, Type};
use sha2::Digest;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
    Error,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct LintIssue {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub tag: Option<u32>,
    pub fixable: bool,
}
#[derive(Debug, serde::Serialize)]
pub struct LintReport {
    pub path: PathBuf,
    pub issues: Vec<LintIssue>,
}
impl LintReport {
    pub fn has_errors(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == Severity::Error)
    }
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }
    fn error(
        &mut self,
        code: &'static str,
        message: impl Into<String>,
        tag: Option<u32>,
        fixable: bool,
    ) {
        self.issues.push(LintIssue {
            code,
            severity: Severity::Error,
            message: message.into(),
            tag,
            fixable,
        });
    }
    fn warning(
        &mut self,
        code: &'static str,
        message: impl Into<String>,
        tag: Option<u32>,
        fixable: bool,
    ) {
        self.issues.push(LintIssue {
            code,
            severity: Severity::Warning,
            message: message.into(),
            tag,
            fixable,
        });
    }
}
/// Read-only inspection. Parser and filesystem failures become non-fixable issues.
pub fn lint_rpm(path: impl AsRef<Path>) -> LintReport {
    let mut report = LintReport {
        path: path.as_ref().to_owned(),
        issues: Vec::new(),
    };
    match Package::open(path.as_ref()) {
        Ok(mut package) => {
            if let Err(error) = inspect(&mut package, &mut report) {
                report.error("INSPECTION_FAILED", error.to_string(), None, false);
            }
        }
        Err(error) => report.error("PARSE_ERROR", error.to_string(), None, false),
    }
    report
}
/// Repair reconstructable metadata and integrity values atomically. This does
/// not recover corrupted content or infer missing names, owners or translations.
/// A modified package is unsigned; a clean package is copied byte-for-byte.
pub fn repair_rpm(input: impl AsRef<Path>, output: impl AsRef<Path>) -> io::Result<LintReport> {
    let input = input.as_ref();
    let output = output.as_ref();
    let before = lint_rpm(input);
    if let Some(issue) = before
        .issues
        .iter()
        .find(|issue| issue.severity == Severity::Error && !issue.fixable)
    {
        return Err(invalid(&format!(
            "Cannot safely repair {}: {}",
            issue.code, issue.message
        )));
    }
    if before.issues.iter().any(|issue| issue.fixable) {
        super::RPMEditor::open(input)?.save(output)?;
    } else {
        let parent = output
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut source = File::open(input)?;
        let permissions = match output.metadata() {
            Ok(metadata) => metadata.permissions(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                source.metadata()?.permissions()
            }
            Err(error) => return Err(error),
        };
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        io::copy(&mut source, &mut temporary)?;
        temporary.as_file_mut().sync_all()?;
        temporary.as_file().set_permissions(permissions)?;
        drop(source);
        temporary.persist(output).map_err(|error| error.error)?;
    }
    Ok(lint_rpm(output))
}
fn inspect(package: &mut Package, report: &mut LintReport) -> io::Result<()> {
    layout(&package.header, 63, "metadata", report);
    layout(&package.signature, 62, "signature", report);
    if !package.lead.name.contains(&0) {
        report.error("LEAD_NAME", "Lead name is not NUL terminated", None, true);
    }
    if package.lead.reserved.iter().any(|value| *value != 0) {
        report.warning("LEAD_RESERVED", "Nonzero reserved lead bytes", None, true);
    }
    for id in [1000, 1001, 1002, 1022] {
        match package.header.get(id) {
            Some(tag)
                if tag.kind == Type::String
                    && tag.count == 1
                    && tag.strings().is_ok_and(|values| !values[0].is_empty()) => {}
            _ => report.error(
                "REQUIRED_TAG",
                format!("Required string tag {id} is missing, empty or invalid"),
                Some(id),
                false,
            ),
        }
    }
    for id in [1003, 1006] {
        if let Some(tag) = package.header.get(id)
            && (tag.kind != Type::Int32 || tag.count != 1)
        {
            report.error(
                "NUMERIC_TYPE",
                format!("Tag {id} must be a scalar INT32"),
                Some(id),
                tag.number().is_some_and(|value| value <= u32::MAX.into()),
            );
        }
    }
    for (id, kind) in [
        (1028, Type::Int32),
        (1030, Type::Int16),
        (1034, Type::Int32),
        (1035, Type::StringArray),
        (1036, Type::StringArray),
        (1037, Type::Int32),
        (1039, Type::StringArray),
        (1040, Type::StringArray),
        (1095, Type::Int32),
        (1096, Type::Int32),
        (1116, Type::Int32),
        (1117, Type::StringArray),
        (1118, Type::StringArray),
        (5008, Type::Int64),
    ] {
        if let Some(tag) = package.header.get(id)
            && tag.kind != kind
        {
            report.error(
                "FILE_TAG_TYPE",
                format!("File tag {id} has type {}, expected {kind}", tag.kind),
                Some(id),
                true,
            );
        }
    }
    let locales = package.header.get(100).map(|tag| tag.count);
    for tag in &package.header.entries {
        if tag.kind == Type::I18nstring && locales != Some(tag.count) {
            report.error(
                "I18N_COUNT",
                format!(
                    "Translation count for tag {} does not match locale table",
                    tag.id
                ),
                Some(tag.id),
                false,
            );
        }
    }
    if package
        .header
        .get(100)
        .is_some_and(|tag| tag.kind != Type::StringArray)
    {
        report.error(
            "LOCALE_TYPE",
            "Locale table must be a string array",
            Some(100),
            false,
        );
    }
    if let Some(tag) = package
        .signature
        .entries
        .iter()
        .find(|tag| matches!(tag.id, 267 | 268 | 278 | 1002 | 1005 | 1006))
    {
        report.warning(
            "SIGNATURE_NOT_VERIFIED",
            "OpenPGP signatures are present; lint checks integrity, not signer authenticity",
            Some(tag.id),
            false,
        );
    }
    check_integrity(package, report)?;
    let mut payload = match Payload::read(package) {
        Ok(payload) => payload,
        Err(error) => {
            report.error("PAYLOAD_INVALID", error.to_string(), None, false);
            return Ok(());
        }
    };
    let files = match file_infos(&package.header) {
        Ok(files) => files,
        Err(error) => {
            report.error("FILE_TABLE", error.to_string(), None, false);
            return Ok(());
        }
    };
    let count = files.len() as u32;
    for id in [
        1029, 1031, 1032, 1033, 1045, 1097, 1140, 1141, 1143, 1144, 5010, 5090, 5115,
    ] {
        if let Some(tag) = package.header.get(id)
            && tag.count != count
        {
            report.error(
                "FILE_ARRAY_LENGTH",
                format!(
                    "File array {id} has {} entries; expected {count}",
                    tag.count
                ),
                Some(id),
                false,
            );
        }
    }
    for (id, kind) in [
        (1033, Type::Int16),
        (1045, Type::Int32),
        (1097, Type::StringArray),
        (1140, Type::Int32),
        (1141, Type::Int32),
        (1143, Type::Int32),
        (1144, Type::Int32),
        (5010, Type::StringArray),
        (5090, Type::StringArray),
        (5115, Type::Int32),
    ] {
        if let Some(tag) = package.header.get(id)
            && tag.kind != kind
        {
            report.error(
                "EXTRA_FILE_TAG_TYPE",
                format!("File array {id} has unsupported type {}", tag.kind),
                Some(id),
                false,
            );
        }
    }
    let paths: HashMap<_, _> = payload
        .records
        .iter()
        .enumerate()
        .map(|(i, record)| (record.path.clone(), i))
        .collect();
    let mut covered = HashSet::new();
    let mut size_inodes = HashSet::new();
    let mut installed_size = 0_u64;
    let algorithm = package.header.number(5011).unwrap_or(1) as u32;
    for file in &files {
        let path = package_path(&file.name)?;
        if let Some(&index) = paths.get(&path) {
            covered.insert(index);
            let record = &payload.records[index];
            let data = record.data;
            if linkable(&record.entry)
                && (record.entry.nlink <= 1
                    || size_inodes.insert((
                        record.entry.dev_major,
                        record.entry.dev_minor,
                        record.entry.ino,
                    )))
            {
                installed_size += u64::from(data.size);
            }
            if linkable(&record.entry) && file.size != u64::from(data.size) {
                report.error(
                    "FILE_SIZE",
                    format!("{} has incorrect stored size", file.name),
                    Some(1028),
                    true,
                );
            }
            if file.mode != record.entry.mode as u16 {
                report.error(
                    "FILE_MODE",
                    format!("{} has conflicting payload and metadata modes", file.name),
                    Some(1030),
                    false,
                );
            }
            if file.mtime != record.entry.mtime {
                report.error(
                    "FILE_MTIME",
                    format!(
                        "{} has conflicting payload and metadata timestamps",
                        file.name
                    ),
                    Some(1034),
                    false,
                );
            }
            if regular(&record.entry) {
                match payload.digest(data, algorithm) {
                    Ok(digest) if !digest.eq_ignore_ascii_case(&file.digest) => report.error(
                        "FILE_DIGEST",
                        format!("{} has an incorrect file digest", file.name),
                        Some(1035),
                        true,
                    ),
                    Err(error) if error.kind() == io::ErrorKind::Unsupported => {
                        report.warning("FILE_DIGEST_ALGORITHM", error.to_string(), Some(5011), true)
                    }
                    Err(error) => return Err(error),
                    _ => {}
                }
            } else if record.entry.mode & 0o170000 == 0o120000
                && payload.bytes(data)? != file.linkname.as_bytes()
            {
                report.error(
                    "FILE_LINK",
                    format!("{} has conflicting symlink targets", file.name),
                    Some(1036),
                    false,
                );
            }
        } else if file.flags & 64 == 0 {
            report.error(
                "MISSING_PAYLOAD_FILE",
                format!("{} is absent from payload", file.name),
                None,
                false,
            );
        } else {
            if matches!(file.mode & 0o170000, 0o100000 | 0o120000) {
                installed_size += file.size;
            }
        }
    }
    if covered.len() != payload.records.len() {
        report.error(
            "UNLISTED_PAYLOAD_FILE",
            "Payload has files absent from metadata",
            None,
            false,
        );
    }
    if package
        .header
        .number(5009)
        .or_else(|| package.header.number(1009))
        != Some(installed_size)
    {
        report.error(
            "INSTALLED_SIZE",
            "Installed size does not match file sizes",
            Some(1009),
            true,
        );
    }
    Ok(())
}
fn layout(header: &RawHeader, region: u32, name: &str, report: &mut LintReport) {
    let mut ids = HashSet::new();
    if header
        .entries
        .iter()
        .any(|tag| tag.id < region && !matches!(tag.id, 61..=64))
    {
        report.error(
            "RESERVED_TAG",
            "Reserved tag ID cannot be placed in a canonical immutable header",
            None,
            false,
        );
    }

    for tag in &header.entries {
        if !ids.insert(tag.id) {
            report.error(
                "DUPLICATE_TAG",
                format!("Duplicate {name} tag {}", tag.id),
                Some(tag.id),
                false,
            );
        }
    }
    if header
        .entries
        .windows(2)
        .any(|pair| pair[0].id >= pair[1].id)
    {
        report.warning(
            "INDEX_ORDER",
            format!("{name} indexes are not strictly sorted"),
            None,
            true,
        );
    }
    for (tag, offset) in header.entries.iter().zip(&header.source_offsets) {
        if !(*offset as usize).is_multiple_of(tag.alignment()) {
            report.error(
                "TAG_ALIGNMENT",
                format!("Tag {} is misaligned", tag.id),
                Some(tag.id),
                true,
            );
        }
    }
    if header.reserved != [0; 4] {
        report.warning(
            "HEADER_RESERVED",
            format!("{name} reserved bytes are nonzero"),
            None,
            true,
        );
    }
    if let Some(tag) = header.get(region) {
        let expected = -(header.entries.len() as i32 * 16);
        let valid = tag.kind == Type::Bin
            && tag.count == 16
            && tag.data.len() == 16
            && u32::from_be_bytes(tag.data[0..4].try_into().expect("region id")) == region
            && u32::from_be_bytes(tag.data[4..8].try_into().expect("region type")) == 7
            && i32::from_be_bytes(tag.data[8..12].try_into().expect("region size")) == expected
            && u32::from_be_bytes(tag.data[12..16].try_into().expect("region count")) == 16;
        if !valid {
            report.error(
                "IMMUTABLE_REGION",
                format!("Invalid {name} immutable region"),
                Some(region),
                true,
            );
        }
    } else {
        report.warning(
            "MISSING_REGION",
            format!("{name} has no v4 immutable region"),
            Some(region),
            true,
        );
    }
}
fn check_integrity(package: &mut Package, report: &mut LintReport) -> io::Result<()> {
    let header = package.original_header()?;
    for (id, digest) in [
        (269, sha1::Sha1::digest(&header).to_vec()),
        (272, sha2::Sha256::digest(&header).to_vec()),
    ] {
        if let Some(expected) = package.signature.get(id)
            && !matches_hex(expected, &digest)
        {
            report.error(
                "HEADER_DIGEST",
                format!("Header digest {id} does not match"),
                Some(id),
                true,
            );
        }
    }
    let mut md5 = md5::Md5::new();
    md5.update(&header);
    package.file.seek(SeekFrom::Start(package.payload_offset))?;
    let mut compressed = sha2::Sha256::new();
    let mut size = 0_u64;
    let mut bytes = [0; 8192];
    use std::io::Read;
    loop {
        let count = package.file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        md5.update(&bytes[..count]);
        compressed.update(&bytes[..count]);
        size += count as u64;
    }
    let mut whole = false;
    let mut payload_covered = false;
    if let Some(expected) = package.signature.get(1004) {
        if expected.kind != Type::Bin || expected.data != md5.finalize().as_slice() {
            report.error(
                "PACKAGE_DIGEST",
                "Header+payload MD5 does not match",
                Some(1004),
                true,
            );
        } else {
            whole = true;
        }
    }
    if let Some(expected) = package.header.get(5092) {
        if package.header.number(5093).unwrap_or(8) == 8 {
            if !matches_hex(expected, &compressed.finalize()) {
                report.error(
                    "PAYLOAD_DIGEST",
                    "Compressed payload digest does not match",
                    Some(5092),
                    true,
                );
            } else {
                payload_covered = true;
            }
        } else {
            report.warning(
                "PAYLOAD_DIGEST_ALGORITHM",
                "Payload hash algorithm is not supported by this linter",
                Some(5093),
                true,
            );
        }
    }
    let header_covered = [269, 272]
        .iter()
        .any(|id| package.signature.get(*id).is_some());
    if !whole && !(header_covered && payload_covered) {
        report.error(
            "MISSING_INTEGRITY",
            "No valid supported digest covers the complete package",
            None,
            true,
        );
    }
    if let Some(expected) = package
        .signature
        .number(270)
        .or_else(|| package.signature.number(1000))
        && expected != header.len() as u64 + size
    {
        report.error(
            "PACKAGE_SIZE",
            "Header+payload size is incorrect",
            Some(1000),
            true,
        );
    }
    // Also check the uncompressed representation when advertised.
    if package.header.get(5097).is_some()
        || package.signature.get(1007).is_some()
        || package.signature.get(271).is_some()
    {
        let compressor = package.compressor()?;
        package.file.seek(SeekFrom::Start(package.payload_offset))?;
        let mut reader = super::file::decompress(&mut package.file, &compressor)?;
        let mut digest = sha2::Sha256::new();
        let mut raw_size = 0_u64;
        loop {
            let count = reader.read(&mut bytes)?;
            if count == 0 {
                break;
            }
            digest.update(&bytes[..count]);
            raw_size += count as u64;
        }
        if let Some(expected) = package.header.get(5097)
            && !matches_hex(expected, &digest.finalize())
        {
            report.error(
                "RAW_PAYLOAD_DIGEST",
                "Uncompressed payload digest does not match",
                Some(5097),
                true,
            );
        }
        if let Some(expected) = package
            .signature
            .number(271)
            .or_else(|| package.signature.number(1007))
            && expected != raw_size
        {
            report.error(
                "RAW_PAYLOAD_SIZE",
                "Uncompressed payload size is incorrect",
                Some(1007),
                true,
            );
        }
    }
    Ok(())
}
fn matches_hex(tag: &RawTag, digest: &[u8]) -> bool {
    if tag.count != 1 {
        return false;
    }
    tag.strings()
        .ok()
        .filter(|values| values.len() == 1)
        .and_then(|values| hex::decode(&values[0]).ok())
        .is_some_and(|bytes| bytes == digest)
}
