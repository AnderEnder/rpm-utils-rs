//! Apply requested metadata and file edits to an RPM.
use super::RPMEditor;
use crate::header::{RType, RawTag};
use std::io;

/// Edits to prepare before previewing or saving a package.
#[derive(Debug, Clone)]
pub struct EditOptions {
    /// Input RPM path.
    pub input: std::path::PathBuf,
    /// Text metadata changes in `TAG=VALUE` form.
    pub set: Vec<String>,
    /// Optional numeric RPM epoch.
    pub epoch: Option<u32>,
    /// Metadata names or numeric tag IDs to remove.
    pub remove_tags: Vec<String>,
    /// File replacements in `PACKAGE_PATH=SOURCE` form.
    pub replace_file: Vec<String>,
    /// File additions in `PACKAGE_PATH=SOURCE` form.
    pub add_file: Vec<String>,
    /// Installed paths to remove.
    pub remove_file: Vec<String>,
    /// Permission changes in `PACKAGE_PATH=OCTAL` form.
    pub chmod: Vec<String>,
}

fn tag_id(value: &str) -> io::Result<u32> {
    let id = match value.to_ascii_lowercase().as_str() {
        "name" => 1000,
        "version" => 1001,
        "release" => 1002,
        "summary" => 1004,
        "description" => 1005,
        "distribution" => 1010,
        "vendor" => 1011,
        "license" => 1014,
        "packager" => 1015,
        "group" => 1016,
        "url" => 1020,
        "os" => 1021,
        "arch" => 1022,
        "prein" => 1023,
        "postin" => 1024,
        "preun" => 1025,
        "postun" => 1026,
        _ => value.parse().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unknown metadata name; use a numeric tag ID",
            )
        })?,
    };
    Ok(id)
}
fn pair(value: &str) -> io::Result<(&str, &str)> {
    value
        .split_once('=')
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Expected KEY=VALUE"))
}
/// Open an RPM and apply the specified edits without saving it.
pub fn prepare(args: &EditOptions) -> io::Result<RPMEditor> {
    let mut editor = RPMEditor::open(&args.input)?;
    for value in &args.set {
        let (tag, text) = pair(value)?;
        editor.set_text(tag_id(tag)?, text)?;
    }
    if let Some(epoch) = args.epoch {
        editor.set_tag(RawTag::from_value(1003, RType::Int32(epoch))?)?;
    }
    for tag in &args.remove_tags {
        editor.remove_tag(tag_id(tag)?)?;
    }
    for value in &args.replace_file {
        let (path, source) = pair(value)?;
        editor.replace_file(path, source)?;
    }
    for value in &args.add_file {
        let (path, source) = pair(value)?;
        editor.add_file(source, path)?;
    }
    for path in &args.remove_file {
        editor.remove_file(path)?;
    }
    for value in &args.chmod {
        let (path, mode) = pair(value)?;
        let mode = u32::from_str_radix(mode.strip_prefix("0o").unwrap_or(mode), 8)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid octal mode"))?;
        editor.set_file_mode(path, mode)?;
    }
    Ok(editor)
}
