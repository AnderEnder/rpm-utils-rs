use chrono::Utc;
use sha2::Digest;
use std::collections::{HashMap, HashSet};
use std::convert::AsRef;
use std::fs::read_to_string;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use super::file::RPMFile;
use crate::payload::{CpioWriter, FileEntry, FileInfo, entry_reader};
use crate::utils::align_n_bytes;

use crate::header::{RType, SignatureTag, Tag, Tags, TagsWrite, encode_region};
use crate::lead::Lead;

#[derive(Debug, Clone)]
pub struct RPMBuilder {
    filename: Option<PathBuf>,
    package_name: Option<String>,
    version: Option<String>,
    release: String,
    epoch: u32,
    arch: String,
    package_group: Option<String>,
    license: Option<String>,
    source_rpm: Option<String>,
    build_time: i64,
    build_host: String,
    summary: Option<String>,
    description: Option<String>,
    pre_install: Option<String>,
    post_install: Option<String>,
    pre_uninstall: Option<String>,
    post_uninstall: Option<String>,
    default_user: String,
    default_group: String,
    directories: Vec<String>,
    files: Vec<String>,
    links: Vec<String>,
    compression: String,
    mapped_files: Vec<(PathBuf, String)>,
}

impl RPMBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn filename<P: AsRef<Path>>(mut self, file: P) -> Self {
        self.filename = Some(file.as_ref().to_owned());
        self
    }

    pub fn package_name(mut self, name: &str) -> Self {
        self.package_name = Some(name.to_owned());
        self
    }

    pub fn version(mut self, version: &str) -> Self {
        self.version = Some(version.to_owned());
        self
    }

    pub fn release(mut self, release: &str) -> Self {
        self.release = release.to_owned();
        self
    }

    pub fn epoch(mut self, epoch: u32) -> Self {
        self.epoch = epoch;
        self
    }

    pub fn arch(mut self, arch: &str) -> Self {
        self.arch = arch.to_owned();
        self
    }

    pub fn package_group(mut self, group: String) -> Self {
        self.package_group = Some(group);
        self
    }

    pub fn license(mut self, license: String) -> Self {
        self.license = Some(license);
        self
    }

    pub fn source_rpm(mut self, source_rpm: String) -> Self {
        self.source_rpm = Some(source_rpm);
        self
    }

    pub fn build_time(mut self, build_time: i64) -> Self {
        self.build_time = build_time;
        self
    }

    pub fn summary(mut self, summary: &str) -> Self {
        self.summary = Some(summary.to_owned());
        self
    }

    pub fn description(mut self, description: &str) -> Self {
        self.description = Some(description.to_owned());
        self
    }

    pub fn build_host(mut self, build_host: &str) -> Self {
        self.build_host = build_host.to_owned();
        self
    }

    pub fn default_user(mut self, user: &str) -> Self {
        self.default_user = user.to_owned();
        self
    }

    pub fn default_group(mut self, group: &str) -> Self {
        self.default_group = group.to_owned();
        self
    }

    pub fn compression(mut self, format: &str) -> Self {
        self.compression = format.to_owned();
        self
    }

    // trigger scripts
    // TriggerIn
    // TriggerUn
    // TriggerPostUn

    pub fn pre_install(mut self, file: &str) -> Self {
        self.pre_install = Some(file.to_owned());
        self
    }

    pub fn post_install(mut self, file: &str) -> Self {
        self.post_install = Some(file.to_owned());
        self
    }

    pub fn pre_uninstall(mut self, file: &str) -> Self {
        self.pre_uninstall = Some(file.to_owned());
        self
    }

    pub fn post_uninstall(mut self, file: &str) -> Self {
        self.post_uninstall = Some(file.to_owned());
        self
    }

    // preTrans
    // postTrans

    pub fn add_file(mut self, file: &str) -> Self {
        self.files.push(file.to_owned());
        self
    }

    pub fn add_files(mut self, files: Vec<&str>) -> Self {
        for file in &files {
            self.files.push((*file).to_owned());
        }
        self
    }

    pub fn add_directory(mut self, dir: &str) -> Self {
        self.directories.push(dir.to_owned());
        self
    }

    pub fn add_directories(mut self, dirs: Vec<&str>) -> Self {
        for dir in dirs.into_iter() {
            self.directories.push(dir.to_owned());
        }
        self
    }

    pub fn add_link(mut self, link: &str) -> Self {
        self.links.push(link.to_owned());
        self
    }

    pub fn add_links(mut self, links: Vec<&str>) -> Self {
        for link in links.into_iter() {
            self.links.push(link.to_owned());
        }
        self
    }

    /// Include a source at an explicit package path, e.g. /usr/bin/tool.
    pub fn add_file_as<P: AsRef<Path>>(mut self, source: P, destination: &str) -> Self {
        self.mapped_files
            .push((source.as_ref().to_owned(), destination.to_owned()));
        self
    }

    /// Prepare metadata and open a low-level output writer. Use build_package()
    /// to produce a complete RPM containing the configured files.
    pub fn build(self) -> io::Result<RPMFile<File>> {
        let filename = self.output_path()?;
        let prepared = self.prepare()?;
        let writer = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(filename)?;
        Ok(RPMFile {
            lead: prepared.lead,
            header_tags: prepared.header_tags,
            signature_tags: prepared.signature_tags,
            payload_offset: 0,
            file: writer,
        })
    }

    /// Build an unsigned RPM v4 package atomically, including files, metadata,
    /// immutable regions, and integrity digests. A failed build preserves output.
    pub fn build_package(self) -> io::Result<RPMFile<File>> {
        let filename = self.output_path()?;
        if self.package_name.as_deref().is_none_or(str::is_empty)
            || self.version.as_deref().is_none_or(str::is_empty)
        {
            return Err(invalid_input("Package name and version are required"));
        }
        let compressor = self.compression.clone();
        let mut prepared = self.prepare()?;
        let compressed = tempfile::NamedTempFile::new()?;
        let mut compression_tags = Tags::new();
        compression_tags.insert_payload_compressor(compressor);
        RPMFile {
            lead: prepared.lead.clone(),
            header_tags: compression_tags,
            signature_tags: Tags::new(),
            payload_offset: 0,
            file: compressed.reopen()?,
        }
        .write_payload(prepared.payload.path())?;
        let compressed_size = compressed.as_file().metadata()?.len();
        let compressed_digest = digest_file::<sha2::Sha256>(compressed.path())?;
        let raw_digest = digest_file::<sha2::Sha256>(prepared.payload.path())?;
        prepared.header_tags.insert(
            Tag::PayloadDigest,
            RType::StringArray(vec![hex::encode(compressed_digest)]),
        );
        prepared
            .header_tags
            .insert(Tag::PayloadDigestAlgo, RType::Int32(8));
        prepared.header_tags.insert(
            Tag::PayloadDigestAlt,
            RType::StringArray(vec![hex::encode(raw_digest)]),
        );
        let header = encode_region(&prepared.header_tags, Tag::Immutable)?;
        let mut md5 = md5::Md5::new();
        md5.update(&header);
        update_digest(&mut md5, &mut File::open(compressed.path())?)?;
        let size = header.len() as u64 + compressed_size;
        if let Ok(size) = u32::try_from(size) {
            prepared
                .signature_tags
                .insert(SignatureTag::Size, RType::Int32(size));
        } else {
            prepared
                .signature_tags
                .insert(SignatureTag::LongSigSize, RType::Int64(size));
        }
        prepared
            .signature_tags
            .insert(SignatureTag::MD5, RType::Bin(md5.finalize().to_vec()));
        prepared.signature_tags.insert(
            SignatureTag::SHA1Header,
            RType::String(hex::encode(sha1::Sha1::digest(&header))),
        );
        prepared.signature_tags.insert(
            SignatureTag::SHA256Header,
            RType::String(hex::encode(sha2::Sha256::digest(&header))),
        );
        let signature = encode_region(&prepared.signature_tags, SignatureTag::HeaderSignatures)?;
        let parent = filename
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut output = tempfile::NamedTempFile::new_in(parent)?;
        prepared.lead.write(&mut output)?;
        output.write_all(&signature)?;
        output.write_all(&[0; 7][..align_n_bytes(signature.len() as u32, 8) as usize])?;
        output.write_all(&header)?;
        io::copy(&mut File::open(compressed.path())?, &mut output)?;
        output.as_file_mut().sync_all()?;
        output.persist(&filename).map_err(|error| error.error)?;
        RPMFile::open(filename)
    }

    fn output_path(&self) -> io::Result<PathBuf> {
        self.filename
            .clone()
            .ok_or_else(|| invalid_input("No rpm file is defined"))
    }

    fn prepare(self) -> io::Result<Prepared> {
        if u32::try_from(self.build_time).is_err() {
            return Err(invalid_input(
                "Build time must fit an unsigned 32-bit timestamp",
            ));
        }
        if !matches!(
            self.compression.as_str(),
            "gzip" | "bzip2" | "xz" | "lzma" | "zstd" | "none"
        ) {
            return Err(invalid_input("Unsupported payload compressor"));
        }
        let mut paths = Vec::new();
        for source in &self.files {
            let path = PathBuf::from(source);
            if !path.symlink_metadata()?.is_file() {
                return Err(invalid_input("add_file requires a regular file"));
            }
            paths.push((path.clone(), default_destination(&path)?));
        }
        for source in &self.links {
            let path = PathBuf::from(source);
            if !path.symlink_metadata()?.file_type().is_symlink() {
                return Err(invalid_input("add_link requires a symbolic link"));
            }
            paths.push((path.clone(), default_destination(&path)?));
        }
        for source in &self.directories {
            let path = PathBuf::from(source);
            if !path.symlink_metadata()?.is_dir() {
                return Err(invalid_input("add_directory requires a directory"));
            }
            collect_tree(&path, &default_destination(&path)?, &mut paths)?;
        }
        for (source, destination) in &self.mapped_files {
            let destination = package_path(destination)?;
            if source.symlink_metadata()?.is_dir() {
                collect_tree(source, &destination, &mut paths)?;
            } else {
                paths.push((source.clone(), destination));
            }
        }
        paths.sort_by(|a, b| a.1.cmp(&b.1));
        let mut names = HashSet::new();
        let mut payload = tempfile::NamedTempFile::new()?;
        let mut infos = Vec::new();
        let mut identities = HashMap::new();
        for (source, destination) in paths {
            let destination = package_path(&destination)?;
            if !names.insert(destination.clone()) {
                return Err(invalid_input("Duplicate package path"));
            }
            let mut record = FileEntry::try_from(&source)?;
            record.name = format!("./{destination}");
            let identity = file_identity(&source)?;
            let next = identities.len() as u32 + 1;
            record.ino = *identities.entry(identity).or_insert(next);
            record.dev_major = 0;
            record.dev_minor = 0;
            let mut reader = HashingReader {
                inner: entry_reader(&source, &record)?,
                digest: sha2::Sha256::new(),
            };
            let mut linkname = String::new();
            if record.mode & 0o170000 == 0o120000 {
                linkname = std::fs::read_link(&source)?
                    .to_str()
                    .ok_or_else(|| invalid_input("Non-UTF8 symlink target"))?
                    .to_owned();
            }
            let info = FileInfo {
                name: format!("/{destination}"),
                size: record.file_size.into(),
                user: self.default_user.clone(),
                group: self.default_group.clone(),
                mtime: record.mtime,
                mode: record.mode as u16,
                linkname,
                inode: record.ino,
                ..Default::default()
            };
            payload.write_cpio_record(record, &mut reader)?;
            let digest = if info.mode & 0o170000 == 0o100000 {
                hex::encode(reader.digest.finalize())
            } else {
                String::new()
            };
            infos.push(FileInfo { digest, ..info });
        }
        payload.cpio_close()?;
        let package_name = self.package_name.unwrap_or_default();
        let mut header_tags = Tags::new();
        header_tags
            .insert_name(package_name.clone())
            .insert_epoch(self.epoch)
            .insert_version(self.version.unwrap_or_default())
            .insert(Tag::Release, RType::String(self.release))
            .insert_arch(self.arch)
            .insert(Tag::Os, RType::String("linux".into()))
            .insert_group(self.package_group.unwrap_or_default())
            .insert_size(crate::payload::installed_size(&infos))
            .insert_license(self.license.unwrap_or_default())
            .insert_source_rpm(self.source_rpm.unwrap_or_default())
            .insert_build_time(self.build_time)
            .insert_build_host(self.build_host)
            .insert_summary(self.summary.unwrap_or_default())
            .insert_description(self.description.unwrap_or_default())
            .insert_payload_format("cpio".into())
            .insert_payload_compressor(self.compression)
            .insert_payload_flags("6".into())
            .insert(Tag::FileDigestAlgo, RType::Int32(8))
            .insert(Tag::I18nTable, RType::StringArray(vec!["C".into()]));
        insert_file_tags(&mut header_tags, &infos);
        for (script, tag, program) in [
            (self.pre_install, Tag::PreIn, Tag::PreInProg),
            (self.post_install, Tag::PostIn, Tag::PostInProg),
            (self.pre_uninstall, Tag::PreUn, Tag::PreUnProg),
            (self.post_uninstall, Tag::PostUn, Tag::PostUnProg),
        ] {
            if let Some(path) = script {
                header_tags.insert(tag, RType::String(read_to_string(path)?));
                header_tags.insert(program, RType::StringArray(vec!["/bin/sh".into()]));
            }
        }
        // Validate authored strings before opening the final destination.
        let mut check = Vec::new();
        check.write_header(&header_tags)?;
        let mut signature_tags = Tags::new();
        signature_tags.insert_payload_size(payload.as_file().metadata()?.len());
        let lead = Lead::from_str(&package_name)?;
        Ok(Prepared {
            lead,
            header_tags,
            signature_tags,
            payload,
        })
    }
}

impl Default for RPMBuilder {
    fn default() -> Self {
        Self {
            filename: None,
            package_name: None,
            version: None,
            release: "1".into(),
            epoch: 0,
            arch: "noarch".into(),
            package_group: None,
            license: None,
            source_rpm: None,
            build_time: Utc::now().timestamp(),
            build_host: hostname::get()
                .map(|host| host.to_string_lossy().into_owned())
                .unwrap_or_default(),
            summary: None,
            description: None,
            pre_install: None,
            post_install: None,
            pre_uninstall: None,
            post_uninstall: None,
            default_user: "root".into(),
            default_group: "root".into(),
            directories: Vec::new(),
            files: Vec::new(),
            links: Vec::new(),
            compression: "gzip".into(),
            mapped_files: Vec::new(),
        }
    }
}

struct Prepared {
    lead: Lead,
    header_tags: Tags<Tag>,
    signature_tags: Tags<SignatureTag>,
    payload: tempfile::NamedTempFile,
}

fn invalid_input(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn package_path(value: &str) -> io::Result<String> {
    let path = Path::new(value);
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => components.push(
                name.to_str()
                    .ok_or_else(|| invalid_input("Non-UTF8 package path"))?,
            ),
            Component::RootDir | Component::CurDir => {}
            _ => return Err(invalid_input("Unsafe package path")),
        }
    }
    if components.is_empty() {
        return Err(invalid_input("Empty package path"));
    }
    Ok(components.join("/"))
}

fn default_destination(path: &Path) -> io::Result<String> {
    let value = if path.is_absolute() {
        Path::new(
            path.file_name()
                .ok_or_else(|| invalid_input("Missing source filename"))?,
        )
    } else {
        path
    };
    package_path(
        value
            .to_str()
            .ok_or_else(|| invalid_input("Non-UTF8 source path"))?,
    )
}

fn collect_tree(
    source: &Path,
    destination: &str,
    paths: &mut Vec<(PathBuf, String)>,
) -> io::Result<()> {
    paths.push((source.to_owned(), destination.to_owned()));
    if source.symlink_metadata()?.is_dir() {
        let mut children = std::fs::read_dir(source)?.collect::<io::Result<Vec<_>>>()?;
        children.sort_by_key(|child| child.file_name());
        for child in children {
            let name = child
                .file_name()
                .into_string()
                .map_err(|_| invalid_input("Non-UTF8 source filename"))?;
            collect_tree(&child.path(), &format!("{destination}/{name}"), paths)?;
        }
    }
    Ok(())
}

fn file_identity(path: &Path) -> io::Result<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = path.symlink_metadata()?;
        Ok((metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    {
        // Windows' synthesized CPIO inode is not a stable identity. Keep entries independent.
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut hasher);
        Ok((0, hasher.finish()))
    }
}

struct HashingReader<R> {
    inner: R,
    digest: sha2::Sha256,
}
impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(bytes)?;
        self.digest.update(&bytes[..count]);
        Ok(count)
    }
}
fn update_digest<D: Digest>(digest: &mut D, reader: &mut impl Read) -> io::Result<()> {
    let mut bytes = [0; 8192];
    loop {
        let count = reader.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        digest.update(&bytes[..count]);
    }
    Ok(())
}
fn digest_file<D: Digest + Default>(path: &Path) -> io::Result<Vec<u8>> {
    let mut digest = D::default();
    update_digest(&mut digest, &mut File::open(path)?)?;
    Ok(digest.finalize().to_vec())
}

pub(super) fn insert_file_tags(tags: &mut Tags<Tag>, files: &[FileInfo]) {
    if files.is_empty() {
        return;
    }
    let mut directories = Vec::new();
    let mut indexes = Vec::new();
    let mut basenames = Vec::new();
    for file in files {
        let (directory, basename) = file
            .name
            .rsplit_once('/')
            .expect("normalized absolute package path");
        let directory = format!("{directory}/");
        let index = directories
            .iter()
            .position(|value| value == &directory)
            .unwrap_or_else(|| {
                directories.push(directory);
                directories.len() - 1
            });
        indexes.push(index as u32);
        basenames.push(basename.to_owned());
    }
    tags.insert(Tag::DirNames, RType::StringArray(directories))
        .insert(Tag::DirIndexes, RType::Int32Array(indexes))
        .insert(Tag::BaseNames, RType::StringArray(basenames));
    if files.iter().any(|file| file.size > u32::MAX.into()) {
        tags.insert(
            Tag::LongFileSizes,
            RType::Int64Array(files.iter().map(|file| file.size).collect()),
        );
    } else {
        tags.insert(
            Tag::FileSizes,
            RType::Int32Array(files.iter().map(|file| file.size as u32).collect()),
        );
    }
    for (tag, values) in [
        (
            Tag::FileUserName,
            files.iter().map(|file| file.user.clone()).collect(),
        ),
        (
            Tag::FileGroupName,
            files.iter().map(|file| file.group.clone()).collect(),
        ),
        (
            Tag::FileLinktos,
            files.iter().map(|file| file.linkname.clone()).collect(),
        ),
        (
            Tag::FileMD5s,
            files.iter().map(|file| file.digest.clone()).collect(),
        ),
    ] {
        tags.insert(tag, RType::StringArray(values));
    }
    tags.insert(
        Tag::FileModes,
        RType::Int16Array(files.iter().map(|file| file.mode).collect()),
    )
    .insert(
        Tag::FileMTimes,
        RType::Int32Array(files.iter().map(|file| file.mtime).collect()),
    )
    .insert(
        Tag::FileFlags,
        RType::Int32Array(files.iter().map(|file| file.flags).collect()),
    )
    .insert(
        Tag::FileDevices,
        RType::Int32Array(files.iter().map(|file| file.device).collect()),
    )
    .insert(
        Tag::FileInodes,
        RType::Int32Array(files.iter().map(|file| file.inode).collect()),
    )
    .insert(Tag::FilerDevs, RType::Int16Array(vec![0; files.len()]))
    .insert(
        Tag::FileVerifyFlags,
        RType::Int32Array(vec![u32::MAX; files.len()]),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn test_builder_smoke() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.rpm");

        let mut rpm = RPMBuilder::new()
            .package_name("Test")
            .description("Test Package")
            .summary("Test Package")
            .version("0.1")
            .release("1234")
            .epoch(1)
            .arch("noarch")
            .compression("gzip")
            .filename(file)
            .build()
            .unwrap();

        rpm.write_head().unwrap();
    }
}
