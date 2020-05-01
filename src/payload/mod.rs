mod cpio;

pub use cpio::*;

use bitflags::bitflags;

#[derive(Debug, Clone)]
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

impl Default for FileInfo {
    fn default() -> Self {
        FileInfo {
            name: "".to_owned(),
            size: 0,
            user: "root".to_owned(),
            group: "root".to_owned(),
            flags: 0,
            mtime: 0,
            digest: "".to_owned(),
            mode: 33188,
            linkname: "root".to_owned(),
            device: 0,
            inode: 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct RPMPayload {
    pub size: u64,
    pub format: String,
    pub compressor: String,
    pub flags: String,
    pub files: Vec<FileInfo>,
}

// https://github.com/eclipse/packagedrone/blob/master/bundles/org.eclipse.packagedrone.utils.rpm/src/org/eclipse/packagedrone/utils/rpm/FileFlags.java
bitflags! {
    struct FileFlags: u32 {
        // from %%config
        const CONFIGURATION = 1 << 0;
        // from %%doc
        const DOC = 1 << 1;
        // from %%donotuse.
        const ICON = 1 << 2;
        // from %%config(missingok)
        const MISSINGOK = 1 << 3;
        // from %%config(noreplace)
        const NOREPLACE = 1 << 4;
        // from %%ghost
        const GHOST = 1 << 6;
        // from %%license
        const LICENSE = 1 << 7;
        // from %%readme
        const README = 1 << 8;
        // bits 9-10 unused
        // from %%pubkey
        const PUBKEY = 1 << 11;
        // from %%artifact
        const ARTIFACT = 1 << 12;
    }
}
