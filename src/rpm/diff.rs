//! Deterministic value comparisons of RPM v3/v4 headers and installed files.
use super::package::*;
use crate::header::{RawHeader, RawTag};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::{io, path::Path};

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct TagValue {
    pub kind: String,
    pub count: u32,
    /// Exact network-order value, including all translations and terminators.
    pub data_hex: String,
}
impl From<&RawTag> for TagValue {
    fn from(tag: &RawTag) -> Self {
        Self {
            kind: format!("{:?}", tag.kind),
            count: tag.count,
            data_hex: hex::encode(&tag.data),
        }
    }
}
#[derive(Debug, serde::Serialize)]
pub struct TagChange {
    pub id: u32,
    pub before: Option<TagValue>,
    pub after: Option<TagValue>,
}
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct FileMetadata {
    pub size: u64,
    pub mode: u16,
    pub user: String,
    pub group: String,
    pub flags: u32,
    pub mtime: u32,
    pub link_target: String,
}
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct FileContent {
    pub size: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub mtime: u32,
    pub rdev_major: u32,
    pub rdev_minor: u32,
    /// SHA-256 of actual file bytes or symlink target; absent for non-data entries.
    pub sha256: Option<String>,
    /// SHA-256 of sorted, length-prefixed installed paths sharing this inode.
    /// Independent of archive inode numbering; absent for unlinked files.
    pub hardlink_group: Option<String>,
}
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct FileSnapshot {
    /// Absent for a payload entry missing from the header inventory.
    pub metadata: Option<FileMetadata>,
    /// Absent for ghost files and missing payload entries.
    pub content: Option<FileContent>,
}
#[derive(Debug, serde::Serialize)]
pub struct FileChange {
    pub path: String,
    pub before: Option<FileSnapshot>,
    pub after: Option<FileSnapshot>,
}
#[derive(Debug, serde::Serialize)]
pub struct DiffReport {
    pub metadata: Vec<TagChange>,
    pub signatures: Vec<TagChange>,
    pub files: Vec<FileChange>,
}
impl DiffReport {
    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty() && self.signatures.is_empty() && self.files.is_empty()
    }
}
fn tags(header: &RawHeader) -> io::Result<BTreeMap<u32, TagValue>> {
    let mut values = BTreeMap::new();
    for tag in &header.entries {
        if values.insert(tag.id, tag.into()).is_some() {
            return Err(invalid("Cannot compare duplicate header tag IDs"));
        }
    }
    Ok(values)
}
fn tag_changes(before: &RawHeader, after: &RawHeader) -> io::Result<Vec<TagChange>> {
    let mut before = tags(before)?;
    let mut after = tags(after)?;
    let ids: BTreeSet<_> = before.keys().chain(after.keys()).copied().collect();
    Ok(ids
        .into_iter()
        .filter_map(|id| {
            let before = before.remove(&id);
            let after = after.remove(&id);
            (before != after).then_some(TagChange { id, before, after })
        })
        .collect())
}
fn files(package: &mut Package) -> io::Result<BTreeMap<String, FileSnapshot>> {
    let mut files = BTreeMap::<String, FileSnapshot>::new();
    for info in file_infos(&package.header)? {
        let name = installed_path(&package_path(&info.name)?);
        let snapshot = FileSnapshot {
            metadata: Some(FileMetadata {
                size: info.size,
                mode: info.mode,
                user: info.user,
                group: info.group,
                flags: info.flags,
                mtime: info.mtime,
                link_target: info.linkname,
            }),
            content: None,
        };
        if files.insert(name, snapshot).is_some() {
            return Err(invalid("Cannot compare duplicate file inventory paths"));
        }
    }
    let mut payload = Payload::read(package)?;
    let mut groups = BTreeMap::<_, Vec<String>>::new();
    for record in &payload.records {
        if linkable(&record.entry) && record.entry.nlink > 1 {
            groups
                .entry((
                    record.entry.dev_major,
                    record.entry.dev_minor,
                    record.entry.ino,
                ))
                .or_default()
                .push(installed_path(&record.path));
        }
    }
    let groups: BTreeMap<_, _> = groups
        .into_iter()
        .map(|(key, mut paths)| {
            paths.sort();
            let mut hash = Sha256::new();
            for path in paths {
                hash.update((path.len() as u64).to_be_bytes());
                hash.update(path.as_bytes());
            }
            (key, hex::encode(hash.finalize()))
        })
        .collect();
    let mut digests = BTreeMap::<(u64, u32), String>::new();
    for index in 0..payload.records.len() {
        let record = &payload.records[index];
        let entry = &record.entry;
        let path = installed_path(&record.path);
        let data = record.data;
        let is_data = linkable(entry);
        let mut content = FileContent {
            size: data.size,
            mode: entry.mode,
            uid: entry.uid,
            gid: entry.gid,
            mtime: entry.mtime,
            rdev_major: entry.rdev_major,
            rdev_minor: entry.rdev_minor,
            sha256: None,
            hardlink_group: if is_data && entry.nlink > 1 {
                Some(groups[&(entry.dev_major, entry.dev_minor, entry.ino)].clone())
            } else {
                None
            },
        };
        if is_data {
            let digest = match digests.entry((data.offset, data.size)) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.get().clone(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(payload.digest(data, 8)?).clone()
                }
            };
            content.sha256 = Some(digest);
        }
        files.entry(path).or_default().content = Some(content);
    }
    Ok(files)
}
/// Compare all raw metadata/signature values and actual decompressed file content.
/// Header ordering, padding, lead fields and compression byte streams are not
/// compared. This does not authenticate signatures or verify declared digests.
pub fn diff_rpm(before: impl AsRef<Path>, after: impl AsRef<Path>) -> io::Result<DiffReport> {
    let mut before = Package::open(before.as_ref())?;
    let mut after = Package::open(after.as_ref())?;
    let metadata = tag_changes(&before.header, &after.header)?;
    let signatures = tag_changes(&before.signature, &after.signature)?;
    let mut left = files(&mut before)?;
    let mut right = files(&mut after)?;
    let paths: BTreeSet<_> = left.keys().chain(right.keys()).cloned().collect();
    let files = paths
        .into_iter()
        .filter_map(|path| {
            let before = left.remove(&path);
            let after = right.remove(&path);
            (before != after).then_some(FileChange {
                path,
                before,
                after,
            })
        })
        .collect();
    Ok(DiffReport {
        metadata,
        signatures,
        files,
    })
}
