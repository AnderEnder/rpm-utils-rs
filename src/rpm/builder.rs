use chrono::Utc;
use std::convert::AsRef;
use std::fs::read_to_string;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use super::file::RPMFile;
use crate::payload::FileInfo;

use crate::header::{SignatureTag, Tag, Tags};
use crate::lead::Lead;

struct InnerPath {
    path: String,
    user: String,
    group: String,
    mode: u8,
}

#[derive(Debug, Default, Clone)]
pub struct RPMBuilder {
    filename: Option<PathBuf>,
    package_name: Option<String>,
    version: Option<String>,
    release: String,
    epoch: u8,
    arch: String,
    package_group: Option<String>,
    license: Option<String>,
    source_rpm: Option<String>,
    build_time: i64,
    build_host: String,
    summary: Option<String>,
    description: Option<String>,
    packager: Option<String>,
    os: Option<String>,
    distribution: Option<String>,
    vendor: Option<String>,
    url: Option<String>,
    pre_install: Option<String>,
    post_install: Option<String>,
    pre_uninstall: Option<String>,
    post_uninstall: Option<String>,
    //  BINARY, SOURCE
    package_type: Option<String>,
    default_user: String,
    default_group: String,
    directories: Vec<String>,
    files: Vec<String>,
    links: Vec<String>,
    compression: String,
}

impl RPMBuilder {
    pub fn new() -> Self {
        let build_time = Utc::now().timestamp();
        Self {
            epoch: 0,
            release: "1".to_owned(),
            arch: "noarch".to_owned(),
            build_host: hostname::get().unwrap().into_string().unwrap(),
            build_time,
            default_user: "root".to_owned(),
            default_group: "root".to_owned(),
            compression: "gzip".to_owned(),
            ..Default::default()
        }
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

    pub fn epoch(mut self, epoch: u8) -> Self {
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

    pub fn build(self) -> io::Result<RPMFile<File>> {
        let filename = self
            .filename
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "No rpm file is defined"))?;

        let writer = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(filename)?;

        let mut file_infos: Vec<FileInfo> = Vec::new();

        for file in self.files {
            file_infos.push(FileInfo {
                name: file,
                ..Default::default()
            });
        }

        let package_name = self.package_name.clone().unwrap_or_default();

        let mut header_tags = Tags::<Tag>::new();
        header_tags
            .insert_name(package_name.clone())
            .insert_epoch(self.epoch)
            .insert_version(self.version.unwrap_or_default())
            .insert_arch(self.arch)
            .insert_group(self.package_group.unwrap_or_default())
            .insert_size(0)
            .insert_license(self.license.unwrap_or_default())
            .insert_source_rpm(self.source_rpm.unwrap_or_default())
            .insert_build_time(self.build_time)
            .insert_build_host(self.build_host)
            .insert_summary(self.summary.unwrap_or_default())
            .insert_description(self.description.unwrap_or_default())
            .insert_payload_format("cpio".to_owned())
            .insert_payload_compressor(self.compression)
            .insert_payload_flags("6".to_owned());

        if let Some(file) = self.pre_install {
            let contents = read_to_string(file)?;
            header_tags.insert_pre_install(contents);
        }

        if let Some(file) = self.post_install {
            let contents = read_to_string(file)?;
            header_tags.insert_post_install(contents);
        }

        if let Some(file) = self.pre_uninstall {
            let contents = read_to_string(file)?;
            header_tags.insert_pre_uninstall(contents);
        }

        if let Some(file) = self.post_uninstall {
            let contents = read_to_string(file)?;
            header_tags.insert_post_uninstall(contents);
        }

        let mut signature_tags = Tags::<SignatureTag>::new();
        signature_tags.insert_payload_size(0);

        let lead = Lead::from_str(&package_name)?;

        Ok(RPMFile {
            lead,
            header_tags,
            signature_tags,
            payload_offset: 0,
            file: writer,
        })
    }
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
