mod cpio;

pub use cpio::*;

#[derive(Debug)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    pub user: String,
    pub group: String,
    pub flags: u32,
    pub mtime: u32,
    pub digest: String,
    pub mode: u16,
    pub linkname: String,
    pub device: u32,
    pub inode: u32,
}

#[derive(Debug, Default)]
pub struct RPMPayload {
    pub size: u64,
    pub format: String,
    pub compressor: String,
    pub flags: String,
    pub files: Vec<FileInfo>,
}
