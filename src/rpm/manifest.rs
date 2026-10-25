//! Declarative packaging shared by rpm-build and our cargo rpm subcommand.
use super::{PackageTarget, RPMBuilder};
use serde::Deserialize;
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct PackageManifest {
    pub name: Option<String>,
    pub version: Option<String>,
    pub target: Option<String>,
    #[serde(default = "release")]
    pub release: String,
    #[serde(default)]
    pub epoch: u32,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    #[serde(default = "compression")]
    pub compression: String,
    pub output: Option<PathBuf>,
    pub source_date_epoch: Option<u32>,
    pub build_host: Option<String>,
    #[serde(default = "root")]
    pub user: String,
    #[serde(default = "root")]
    pub group: String,
    #[serde(default)]
    pub uid: u32,
    #[serde(default)]
    pub gid: u32,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub files: Vec<ManifestFile>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestFile {
    pub source: PathBuf,
    pub dest: String,
    /// Octal permission bits, e.g. "0755"; applies to this path only.
    pub mode: Option<String>,
}
fn release() -> String {
    "1".into()
}
fn compression() -> String {
    "gzip".into()
}
fn root() -> String {
    "root".into()
}
pub(crate) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
impl PackageManifest {
    pub fn from_toml(text: &str) -> io::Result<Self> {
        toml::from_str(text).map_err(|error| invalid(format!("Invalid RPM manifest: {error}")))
    }
    /// Explicit manifest timestamp takes precedence over SOURCE_DATE_EPOCH.
    pub fn reproducible_epoch(&self) -> io::Result<Option<u32>> {
        if let Some(epoch) = self.source_date_epoch {
            return Ok(Some(epoch));
        }
        match std::env::var("SOURCE_DATE_EPOCH") {
            Ok(value) => value
                .parse()
                .map(Some)
                .map_err(|_| invalid("SOURCE_DATE_EPOCH must be an unsigned 32-bit integer")),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(_) => Err(invalid("SOURCE_DATE_EPOCH is not valid Unicode")),
        }
    }
    pub fn package_target(&self) -> io::Result<PackageTarget> {
        self.target
            .as_deref()
            .ok_or_else(|| {
                invalid("An explicit RPM target is required (use noarch for JVM/data packages)")
            })?
            .parse()
    }
    pub fn build(&self, base: &Path, output: Option<&Path>) -> io::Result<PathBuf> {
        let target = self.package_target()?;
        let name = self
            .name
            .as_deref()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| invalid("Package name is required"))?;
        let version = self
            .version
            .as_deref()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| invalid("Package version is required"))?;
        let output = if let Some(path) = output {
            path.to_owned()
        } else if let Some(path) = &self.output {
            base.join(path)
        } else {
            for value in [name, version, self.release.as_str()] {
                if value.is_empty()
                    || !value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
                {
                    return Err(invalid(
                        "Default RPM filename requires simple name/version/release components; specify output explicitly",
                    ));
                }
            }
            base.join(format!(
                "{name}-{version}-{}.{}.rpm",
                self.release,
                target.arch()
            ))
        };
        let mut builder = RPMBuilder::new()
            .filename(&output)
            .package_name(name)
            .version(version)
            .release(&self.release)
            .epoch(self.epoch)
            .target(self.target.as_deref().unwrap())?
            .compression(&self.compression)
            .default_user(&self.user)
            .default_group(&self.group)
            .payload_owner(self.uid, self.gid);
        if let Some(epoch) = self.reproducible_epoch()? {
            builder = builder.reproducible(epoch);
        }
        if let Some(host) = &self.build_host {
            builder = builder.build_host(host);
        }
        if let Some(summary) = &self.summary {
            builder = builder.summary(summary);
        }
        if let Some(description) = &self.description {
            builder = builder.description(description);
        }
        if let Some(license) = &self.license {
            builder = builder.license(license.clone());
        }
        for name in &self.requires {
            builder = builder.requires(name);
        }
        for file in &self.files {
            builder = builder.add_file_as(base.join(&file.source), &file.dest);
            if let Some(mode) = &file.mode {
                let mode = u32::from_str_radix(mode.strip_prefix("0o").unwrap_or(mode), 8)
                    .map_err(|_| invalid("File modes must be octal strings, e.g. '0755'"))?;
                builder = builder.file_mode(&file.dest, mode)?;
            }
        }
        builder.build_package()?;
        Ok(output)
    }
}
