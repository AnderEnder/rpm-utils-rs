//! Atomic RPM v3/v4 metadata and payload editing. Saved packages are unsigned.
use super::RPMFile;
use super::package::*;
use crate::header::{RType, RawHeader, RawTag, Tag, Tags, Type};
use crate::lead::Lead;
use crate::payload::{CpioWriter, FileInfo};
use crate::utils::align_n_bytes;
use num_traits::FromPrimitive;
use sha2::Digest;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// An editor preserves opaque metadata values. File edits do not infer new
/// dependency requirements; callers must update dependency metadata separately.
pub struct RPMEditor {
    package: Package,
    payload: Option<Payload>,
    files: Vec<(FileInfo, Option<usize>)>,
    payload_changed: bool,
}
#[derive(Debug, serde::Serialize)]
pub struct EditReport {
    pub output: PathBuf,
    pub payload_changed: bool,
    /// OpenPGP signature tags removed because the edited header invalidates them.
    pub removed_signatures: Vec<u32>,
    pub removed_file_signatures: Vec<u32>,
}
/// A validated edit preview. All temporary package files are removed on return.
#[derive(Debug, serde::Serialize)]
pub struct EditPreview {
    pub payload_changed: bool,
    pub removed_signatures: Vec<u32>,
    pub removed_file_signatures: Vec<u32>,
    pub changes: super::DiffReport,
}
impl RPMEditor {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let package = Package::open(path.as_ref())?;
        let files = file_infos(&package.header)?
            .into_iter()
            .enumerate()
            .map(|(i, file)| (file, Some(i)))
            .collect();
        Ok(Self {
            package,
            payload: None,
            files,
            payload_changed: false,
        })
    }
    pub fn header(&self) -> &RawHeader {
        &self.package.header
    }
    /// Structural, payload and file table tags are managed by file-edit methods.
    pub fn set_tag(&mut self, tag: RawTag) -> io::Result<&mut Self> {
        if managed(tag.id) {
            return Err(invalid("Tag is managed by the package editor"));
        }
        self.package.header.set(tag)?;
        Ok(self)
    }
    /// Change the first/default translation while preserving other locale bytes.
    pub fn set_text(&mut self, id: u32, text: &str) -> io::Result<&mut Self> {
        if text.contains('\0') {
            return Err(invalid("NUL in metadata text"));
        }
        let tag = if let Some(old) = self.package.header.get(id) {
            match old.kind {
                Type::String => RawTag::strings_value(id, Type::String, &[text.into()])?,
                Type::I18nstring => {
                    let end = old
                        .data
                        .iter()
                        .position(|byte| *byte == 0)
                        .ok_or_else(|| invalid("Missing default translation"))?;
                    let mut data = text.as_bytes().to_vec();
                    data.push(0);
                    data.extend_from_slice(&old.data[end + 1..]);
                    RawTag::new(id, old.kind.clone(), old.count, data)?
                }
                _ => {
                    return Err(invalid(
                        "Existing tag is not textual; use set_tag for typed values",
                    ));
                }
            }
        } else if matches!(id, 1004 | 1005 | 1016) {
            let count = self
                .package
                .header
                .get(100)
                .map_or(1, |tag| tag.count.max(1)) as usize;
            if count > 1024 {
                return Err(unsupported("Locale count exceeds editor resource limit"));
            }
            let mut values = vec![String::new(); count];
            values[0] = text.into();
            RawTag::strings_value(id, Type::I18nstring, &values)?
        } else {
            RawTag::strings_value(id, Type::String, &[text.into()])?
        };
        self.set_tag(tag)
    }
    pub fn remove_tag(&mut self, id: u32) -> io::Result<&mut Self> {
        if managed(id) || matches!(id, 1000..=1002 | 1022) {
            return Err(invalid("Required or managed tag cannot be removed"));
        }
        self.package.header.remove(id);
        Ok(self)
    }
    fn payload(&mut self) -> io::Result<&mut Payload> {
        if self.payload.is_none() {
            self.payload = Some(Payload::read(&mut self.package)?);
        }
        Ok(self.payload.as_mut().expect("payload initialized"))
    }
    pub fn replace_file(
        &mut self,
        destination: &str,
        source: impl AsRef<Path>,
    ) -> io::Result<&mut Self> {
        let path = package_path(destination)?;
        let payload = self.payload()?;
        let index = payload
            .records
            .iter()
            .position(|record| record.path == path)
            .ok_or_else(|| invalid("File does not exist in payload"))?;
        if !regular(&payload.records[index].entry) {
            return Err(invalid("Replacement target is not a regular file"));
        }
        let (source, data) = payload.append_source(source.as_ref())?;
        let inode = payload
            .records
            .iter()
            .map(|record| record.entry.ino)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| invalid("No free inode ID"))?;
        let record = &mut payload.records[index];
        record.data = data;
        record.entry.file_size = data.size;
        record.entry.mtime = source.mtime;
        record.entry.ino = inode;
        record.entry.nlink = 1;
        self.payload_changed = true;
        Ok(self)
    }
    pub fn add_file(
        &mut self,
        source: impl AsRef<Path>,
        destination: &str,
    ) -> io::Result<&mut Self> {
        let path = package_path(destination)?;
        if self
            .files
            .iter()
            .any(|(file, _)| file.name == installed_path(&path))
        {
            return Err(invalid("Package path already exists"));
        }
        let payload = self.payload()?;
        if payload.records.iter().any(|record| record.path == path) {
            return Err(invalid("Payload path already exists"));
        }
        let (mut entry, data) = payload.append_source(source.as_ref())?;
        entry.name = format!("./{path}");
        entry.nlink = 1;
        entry.ino = payload
            .records
            .iter()
            .map(|record| record.entry.ino)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| invalid("No free inode ID"))?;
        entry.dev_major = 0;
        entry.dev_minor = 0;
        let info = FileInfo {
            name: installed_path(&path),
            mode: entry.mode as u16,
            mtime: entry.mtime,
            size: data.size.into(),
            inode: entry.ino,
            ..Default::default()
        };
        payload.records.push(Record { entry, path, data });
        self.files.push((info, None));
        self.payload_changed = true;
        Ok(self)
    }
    pub fn remove_file(&mut self, destination: &str) -> io::Result<&mut Self> {
        let path = package_path(destination)?;
        let name = installed_path(&path);
        if !self.files.iter().any(|(file, _)| file.name == name) {
            return Err(invalid("Package file does not exist"));
        }
        if self
            .files
            .iter()
            .any(|(file, _)| file.name.starts_with(&format!("{name}/")))
        {
            return Err(invalid("Remove directory children first"));
        }
        self.payload()?.records.retain(|record| record.path != path);
        self.files.retain(|(file, _)| file.name != name);
        self.payload_changed = true;
        Ok(self)
    }
    pub fn set_file_mode(&mut self, destination: &str, permissions: u32) -> io::Result<&mut Self> {
        if permissions & !0o7777 != 0 {
            return Err(invalid("Permissions must contain only mode bits"));
        }
        let path = package_path(destination)?;
        let changed = {
            let payload = self.payload()?;
            let target = payload
                .records
                .iter()
                .find(|record| record.path == path)
                .ok_or_else(|| invalid("Payload file does not exist"))?;
            if target.entry.mode & 0o170000 == 0o120000 {
                return Err(invalid("Symbolic-link permissions cannot be changed"));
            }
            let key = (
                target.entry.dev_major,
                target.entry.dev_minor,
                target.entry.ino,
            );
            let linked = regular(&target.entry) && target.entry.nlink > 1;
            let mut changed = Vec::new();
            for record in &mut payload.records {
                if record.path == path
                    || (linked
                        && regular(&record.entry)
                        && (
                            record.entry.dev_major,
                            record.entry.dev_minor,
                            record.entry.ino,
                        ) == key)
                {
                    record.entry.mode = (record.entry.mode & 0o170000) | permissions;
                    changed.push((installed_path(&record.path), record.entry.mode as u16));
                }
            }
            changed
        };
        for (name, mode) in changed {
            let info = self
                .files
                .iter_mut()
                .find(|(file, _)| file.name == name)
                .ok_or_else(|| invalid("Payload file is absent from metadata"))?;
            info.0.mode = mode;
        }
        self.payload_changed = true;
        Ok(self)
    }
    /// Build and validate an edit in scratch storage, then compare it with the
    /// original package. No input or caller-specified output path is modified.
    /// This consumes the editor and applies the same checks as `save()`.
    pub fn preview(mut self) -> io::Result<EditPreview> {
        let directory = tempfile::tempdir()?;
        let original = directory.path().join("original.rpm");
        let edited = directory.path().join("edited.rpm");
        self.package.file.rewind()?;
        io::copy(&mut self.package.file, &mut File::create(&original)?)?;
        let report = self.save(&edited)?;
        let changes = super::diff_rpm(&original, &edited)?;
        Ok(EditPreview {
            payload_changed: report.payload_changed,
            removed_signatures: report.removed_signatures,
            removed_file_signatures: report.removed_file_signatures,
            changes,
        })
    }
    /// Regenerate integrity information and publish atomically. Existing OpenPGP
    /// signatures are dropped; payload bytes are preserved for metadata-only edits.
    pub fn save(mut self, output: impl AsRef<Path>) -> io::Result<EditReport> {
        let output = output.as_ref();
        let mut removed_file_signatures: Vec<u32> = self
            .package
            .signature
            .entries
            .iter()
            .filter(|tag| matches!(tag.id, 274..=277))
            .map(|tag| tag.id)
            .collect();
        if self.payload_changed {
            removed_file_signatures.extend(
                self.package
                    .header
                    .entries
                    .iter()
                    .filter(|tag| matches!(tag.id, 274..=277 | 5090 | 5091))
                    .map(|tag| tag.id),
            );
        }
        removed_file_signatures.sort_unstable();
        removed_file_signatures.dedup();
        let permissions = match output.metadata() {
            Ok(metadata) => metadata.permissions(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.package.file.metadata()?.permissions()
            }
            Err(error) => return Err(error),
        };
        let mut payload = match self.payload.take() {
            Some(payload) => payload,
            None => Payload::read(&mut self.package)?,
        };
        let by_path: HashMap<_, _> = payload
            .records
            .iter()
            .enumerate()
            .map(|(i, record)| (record.path.clone(), i))
            .collect();
        let mut covered = std::collections::HashSet::new();
        for (info, _) in &mut self.files {
            let path = package_path(&info.name)?;
            if let Some(&index) = by_path.get(&path) {
                covered.insert(index);
                let record = &payload.records[index];
                if info.mode != record.entry.mode as u16 {
                    return Err(invalid(
                        "File mode differs between payload and metadata; choose a mode explicitly",
                    ));
                }
                if linkable(&record.entry) {
                    info.size = record.data.size.into();
                }
                info.inode = record.entry.ino;
                if self.payload_changed {
                    info.mtime = record.entry.mtime;
                }
                let data = record.data;
                info.digest = if regular(&record.entry) {
                    payload.digest(data, 8)?
                } else {
                    String::new()
                };
                if info.mode & 0o170000 == 0o120000 {
                    info.linkname = String::from_utf8(payload.bytes(data)?)
                        .map_err(|_| invalid("Non-UTF8 symlink target"))?;
                }
            } else if info.flags & 64 == 0 {
                return Err(invalid("Non-ghost metadata file is missing from payload"));
            }
        }
        if covered.len() != payload.records.len() {
            return Err(invalid("Payload contains files absent from metadata"));
        }
        let old_count = self
            .package
            .header
            .get(1117)
            .or_else(|| self.package.header.get(1027))
            .map_or(0, |tag| tag.count as usize);
        let mapping_changed = self.files.len() != old_count
            || self
                .files
                .iter()
                .enumerate()
                .any(|(index, (_, old))| *old != Some(index));
        if mapping_changed {
            for tag in &self.package.header.entries {
                let array = tag.kind == Type::StringArray
                    || (tag.count > 1
                        && matches!(
                            tag.kind,
                            Type::Char | Type::Int8 | Type::Int16 | Type::Int32 | Type::Int64
                        ));
                if Tag::from_u32(tag.id).is_none() && array && tag.count as usize == old_count {
                    return Err(unsupported(&format!(
                        "Cannot safely realign opaque array tag {}; update or remove it explicitly",
                        tag.id
                    )));
                }
            }
        }
        realign_extra_file_tags(&mut self.package.header, old_count, &self.files)?;
        self.package.header.remove(1027);
        self.package.header.remove(5008);
        self.package.header.remove(1028);
        for id in [1003, 1006] {
            if let Some(value) = self.package.header.number(id) {
                let value = u32::try_from(value)
                    .map_err(|_| invalid("Numeric metadata does not fit INT32"))?;
                self.package.header.set_value(id, RType::Int32(value))?;
            }
        }
        let infos: Vec<_> = self.files.iter().map(|(file, _)| file.clone()).collect();
        let mut tags = Tags::<Tag>::new();
        super::builder::insert_file_tags(&mut tags, &infos);
        for tag in RawHeader::from_tags(&tags)?.entries {
            // Preserve original verify flags and rdev data realigned above.
            if matches!(tag.id, 1033 | 1045) && self.package.header.get(tag.id).is_some() {
                continue;
            }
            self.package.header.set(tag)?;
        }
        if infos.is_empty() {
            for id in FILE_TAGS {
                self.package.header.remove(*id);
            }
            for id in [1116, 1117, 1118, 5008] {
                self.package.header.remove(id);
            }
        }
        self.package.header.set_value(5011, RType::Int32(8))?;
        set_number(
            &mut self.package.header,
            1009,
            crate::payload::installed_size(&infos),
            5009,
        )?;
        let compressor = self.package.compressor()?;
        let compressed = tempfile::NamedTempFile::new()?;
        let raw = if self.payload_changed {
            let mut raw = tempfile::NamedTempFile::new()?;
            let mut groups = HashMap::new();
            for record in &payload.records {
                if regular(&record.entry) {
                    *groups
                        .entry((
                            record.entry.dev_major,
                            record.entry.dev_minor,
                            record.entry.ino,
                        ))
                        .or_insert(0_u32) += 1;
                }
            }
            for record in &mut payload.records {
                record.entry.file_size = record.data.size;
                record.entry.checksum = None;
                if regular(&record.entry) {
                    record.entry.nlink = groups[&(
                        record.entry.dev_major,
                        record.entry.dev_minor,
                        record.entry.ino,
                    )];
                }
                payload.file.seek(SeekFrom::Start(record.data.offset))?;
                let mut reader = payload.file.as_file_mut().take(record.data.size.into());
                // FileEntry has a public, owned representation; regenerate its header.
                let entry = record.entry.clone();
                raw.write_cpio_record(entry, &mut reader)?;
            }
            raw.cpio_close()?;
            let mut compression = Tags::new();
            compression.insert_payload_compressor(compressor);
            RPMFile {
                lead: self.package.lead.clone(),
                signature_tags: Tags::new(),
                header_tags: compression,
                payload_offset: 0,
                file: compressed.reopen()?,
            }
            .write_payload(raw.path())?;
            // Any file-content signature is invalid after a payload edit.
            for id in [5090, 5091, 274, 275, 276, 277] {
                self.package.header.remove(id);
            }
            raw
        } else {
            self.package
                .file
                .seek(SeekFrom::Start(self.package.payload_offset))?;
            io::copy(&mut self.package.file, &mut compressed.reopen()?)?;
            payload.file
        };
        self.package.header.set_value(
            5092,
            RType::StringArray(vec![hex::encode(hash_reader::<sha2::Sha256>(
                &mut File::open(compressed.path())?,
            )?)]),
        )?;
        self.package.header.set_value(5093, RType::Int32(8))?;
        self.package.header.set_value(
            5097,
            RType::StringArray(vec![hex::encode(hash_reader::<sha2::Sha256>(
                &mut File::open(raw.path())?,
            )?)]),
        )?;
        // Algorithms we cannot recompute must never survive a changed payload.
        if self.payload_changed {
            for id in 5121..=5124 {
                self.package.header.remove(id);
            }
        }
        let mut removed_signatures: Vec<u32> = self
            .package
            .signature
            .entries
            .iter()
            .filter(|tag| matches!(tag.id, 267 | 268 | 278 | 1002 | 1005 | 1006))
            .map(|tag| tag.id)
            .collect();
        // Signature aliases may also have been merged into the metadata header.
        for id in [
            257, 258, 259, 260, 261, 262, 263, 264, 265, 267, 268, 269, 270, 271, 272, 273, 278,
            279,
        ] {
            if self.package.header.get(id).is_some() {
                if matches!(id, 259 | 262 | 263 | 267 | 268 | 278) {
                    removed_signatures.push(id);
                }
                self.package.header.remove(id);
            }
        }
        removed_signatures.sort_unstable();
        removed_signatures.dedup();
        let header = self.package.header.encode_region(63)?;
        let mut signature = RawHeader::default();
        let mut md5 = md5::Md5::new();
        md5.update(&header);
        let mut source = File::open(compressed.path())?;
        let mut buffer = [0; 8192];
        loop {
            let count = source.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            md5.update(&buffer[..count]);
        }
        signature.set_value(1004, RType::Bin(md5.finalize().to_vec()))?;
        signature.set_value(269, RType::String(hex::encode(sha1::Sha1::digest(&header))))?;
        signature.set_value(
            272,
            RType::String(hex::encode(sha2::Sha256::digest(&header))),
        )?;
        set_number(
            &mut signature,
            1000,
            header.len() as u64 + compressed.as_file().metadata()?.len(),
            270,
        )?;
        set_number(&mut signature, 1007, raw.as_file().metadata()?.len(), 271)?;
        let signature = signature.encode_region(62)?;
        let name = self
            .package
            .header
            .strings(1000)?
            .first()
            .cloned()
            .ok_or_else(|| invalid("Missing package name"))?;
        self.package.lead.name = name.parse::<Lead>()?.name;
        let parent = output
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut result = tempfile::NamedTempFile::new_in(parent)?;
        self.package.lead.write(&mut result)?;
        result.write_all(&signature)?;
        result.write_all(&[0; 7][..align_n_bytes(signature.len() as u32, 8) as usize])?;
        result.write_all(&header)?;
        io::copy(&mut File::open(compressed.path())?, &mut result)?;
        result.as_file_mut().sync_all()?;
        let validation = super::lint::lint_rpm(result.path());
        if let Some(issue) = validation
            .issues
            .iter()
            .find(|issue| issue.severity == super::lint::Severity::Error)
        {
            return Err(invalid(&format!(
                "Edited package failed validation: {}",
                issue.message
            )));
        }
        result.as_file().set_permissions(permissions)?;
        // Close the input before replacement, including for Windows in-place edits.
        drop(self.package);
        result.persist(output).map_err(|error| error.error)?;
        Ok(EditReport {
            output: output.to_owned(),
            payload_changed: self.payload_changed,
            removed_signatures,
            removed_file_signatures,
        })
    }
}
// Managed metadata describes the payload and is regenerated together with it.
fn managed(id: u32) -> bool {
    matches!(id, 0..=64 | 1009 | 1027..=1037 | 1039..=1040 | 1045 | 1095..=1097 | 1116..=1118 | 1124..=1126 | 5008..=5011 | 5090..=5093 | 5097 | 5112..=5115 | 5121..=5124)
}
const FILE_TAGS: &[u32] = &[
    1028, 1029, 1030, 1031, 1032, 1033, 1034, 1035, 1036, 1037, 1039, 1040, 1045, 1095, 1096, 1097,
    1140, 1141, 1143, 1144, 5010, 5090, 5115,
];
fn realign_extra_file_tags(
    header: &mut RawHeader,
    old_count: usize,
    files: &[(FileInfo, Option<usize>)],
) -> io::Result<()> {
    for id in [
        1029, 1031, 1032, 1033, 1045, 1097, 1140, 1141, 1143, 1144, 5010, 5090, 5115,
    ] {
        let Some(old) = header.get(id).cloned() else {
            continue;
        };
        if old.count as usize != old_count {
            return Err(invalid("Extra file array has wrong length"));
        }
        let values = if old.kind == Type::StringArray {
            old.data
                .split_inclusive(|byte| *byte == 0)
                .map(|bytes| bytes.to_vec())
                .collect::<Vec<_>>()
        } else {
            let width = match old.kind {
                Type::Char | Type::Int8 => 1,
                Type::Int16 => 2,
                Type::Int32 => 4,
                Type::Int64 => 8,
                _ => return Err(unsupported("Cannot realign this file array type")),
            };
            old.data
                .chunks_exact(width)
                .map(|bytes| bytes.to_vec())
                .collect()
        };
        let default = if id == 1045 {
            u32::MAX.to_be_bytes().to_vec()
        } else if old.kind == Type::StringArray {
            vec![0]
        } else {
            vec![0; old.alignment()]
        };
        let data = files
            .iter()
            .flat_map(|(_, index)| {
                index
                    .and_then(|index| values.get(index))
                    .unwrap_or(&default)
                    .clone()
            })
            .collect();
        header.set(RawTag::new(id, old.kind, files.len() as u32, data)?)?;
    }
    Ok(())
}
