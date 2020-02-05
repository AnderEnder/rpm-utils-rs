#[derive(Debug, Default)]
pub struct RPMBuilder {
    filename: Option<String>,
    name: Option<String>,
    version: Option<String>,
    release: Option<String>,
    epoch: u8,
    arch: Option<String>,
    group: Option<String>,
    license: Option<String>,
    source_rpm: Option<String>,
    build_time: Option<String>,
    build_host: Option<String>,
    summary: Option<String>,
    description: Option<String>,
}

impl RPMBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn filename(mut self, file: &str) -> Self {
        self.filename = Some(file.to_owned());
        self
    }

    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_owned());
        self
    }

    pub fn version(mut self, version: &str) -> Self {
        self.version = Some(version.to_owned());
        self
    }

    pub fn release(mut self, release: &str) -> Self {
        self.release = Some(release.to_owned());
        self
    }

    pub fn epoch(mut self, epoch: u8) -> Self {
        self.epoch = epoch;
        self
    }

    pub fn arch(mut self, arch: String) -> Self {
        self.arch = Some(arch);
        self
    }

    pub fn group(mut self, group: String) -> Self {
        self.group = Some(group);
        self
    }

    pub fn license(mut self, license: String) -> Self {
        self.license = Some(license);
        self
    }

    //    source_rpm: Option<String>,
    //    build_time: Option<String>,
    //    build_host: Option<String>,

    pub fn summary(mut self, summary: &str) -> Self {
        self.summary = Some(summary.to_owned());
        self
    }

    pub fn description(mut self, description: &str) -> Self {
        self.description = Some(description.to_owned());
        self
    }
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
            .epoch(1);
    }
}
