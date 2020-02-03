#[derive(Debug, Default)]
pub struct RPMBuilder {
    filename: Option<String>,
    name: Option<String>,
    version: Option<String>,
    release: Option<String>,
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

    pub fn filename(mut self, file: String) -> Self {
        self.filename = Some(file);
        self
    }

    pub fn name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    pub fn version(mut self, version: String) -> Self {
        self.version = Some(version);
        self
    }

    pub fn release(mut self, release: String) -> Self {
        self.release = Some(release);
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
}
