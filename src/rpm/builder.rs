use chrono::Utc;
use gethostname::gethostname;
use std::ffi::OsString;

#[derive(Debug, Default)]
pub struct RPMBuilder {
    filename: Option<String>,
    package_name: Option<String>,
    version: Option<String>,
    release: String,
    epoch: u8,
    arch: String,
    package_group: Option<String>,
    license: Option<String>,
    source_rpm: Option<String>,
    build_time: i64,
    build_host: OsString,
    summary: Option<String>,
    description: Option<String>,
    packager: Option<String>,
    os: Option<String>,
    distribution: Option<String>,
    vendor: Option<String>,
    url: Option<String>,
    //  BINARY, SOURCE
    package_type: Option<String>,
    default_user: Option<String>,
    default_group: Option<String>,
    directories: Vec<String>,
    files: Vec<String>,
    links: Vec<String>,
}

impl RPMBuilder {
    pub fn new() -> Self {
        let build_time = Utc::now().timestamp();
        Self {
            epoch: 0,
            release: "1".to_owned(),
            arch: "noarch".to_owned(),
            build_host: gethostname(),
            build_time,
            ..Default::default()
        }
    }

    pub fn filename(mut self, file: &str) -> Self {
        self.filename = Some(file.to_owned());
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
        self.build_host = OsString::from(build_host);
        self
    }

    // scripts
    // install_utils
    // pre_install
    // postInstall
    // preUninstall
    // postUninstall
    // preTrans
    // postTrans
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::u32;
    #[test]
    fn test_builder_smoke() {
        let rpm = RPMBuilder::new()
            .name("Test")
            .description("Test Package")
            .summary("Test Package")
            .version("0.1")
            .release("1234")
            .epoch(1)
            .arch("noarch");
    }
}
