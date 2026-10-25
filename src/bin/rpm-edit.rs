use clap::Parser;
use rpm_utils::{
    RPMEditor,
    header::{RType, RawTag},
};
use std::{io, path::PathBuf, process::ExitCode};

#[derive(Debug, Parser)]
#[command(about = "Edit RPM v3/v4 metadata or files and write an unsigned package atomically")]
struct Args {
    /// Input RPM
    input: PathBuf,
    /// Output RPM
    #[arg(
        short,
        long,
        required_unless_present = "in_place",
        conflicts_with = "in_place"
    )]
    output: Option<PathBuf>,
    /// Replace the input only after validation succeeds
    #[arg(long)]
    in_place: bool,
    /// Set textual metadata by name or numeric tag ID: summary=New text
    #[arg(long = "set", value_name = "TAG=VALUE")]
    set: Vec<String>,
    #[arg(long)]
    epoch: Option<u32>,
    /// Remove an optional metadata tag by name or ID
    #[arg(long = "remove-tag")]
    remove_tags: Vec<String>,
    /// Replace a regular file, preserving its installed permissions
    #[arg(long, value_name = "PACKAGE_PATH=SOURCE")]
    replace_file: Vec<String>,
    /// Add a regular file (default owner/group metadata: root)
    #[arg(long, value_name = "PACKAGE_PATH=SOURCE")]
    add_file: Vec<String>,
    #[arg(long, value_name = "PACKAGE_PATH")]
    remove_file: Vec<String>,
    /// Change file or directory permissions, e.g. /usr/bin/tool=0755
    #[arg(long, value_name = "PACKAGE_PATH=OCTAL")]
    chmod: Vec<String>,
    /// Print a machine-readable edit report
    #[arg(long)]
    json: bool,
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
fn run(args: Args) -> io::Result<()> {
    let mut editor = RPMEditor::open(&args.input)?;
    for value in args.set {
        let (tag, text) = pair(&value)?;
        editor.set_text(tag_id(tag)?, text)?;
    }
    if let Some(epoch) = args.epoch {
        editor.set_tag(RawTag::from_value(1003, RType::Int32(epoch))?)?;
    }
    for tag in args.remove_tags {
        editor.remove_tag(tag_id(&tag)?)?;
    }
    for value in args.replace_file {
        let (path, source) = pair(&value)?;
        editor.replace_file(path, source)?;
    }
    for value in args.add_file {
        let (path, source) = pair(&value)?;
        editor.add_file(source, path)?;
    }
    for path in args.remove_file {
        editor.remove_file(&path)?;
    }
    for value in args.chmod {
        let (path, mode) = pair(&value)?;
        let mode = u32::from_str_radix(mode.strip_prefix("0o").unwrap_or(mode), 8)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid octal mode"))?;
        editor.set_file_mode(path, mode)?;
    }
    let output = args.output.as_ref().unwrap_or(&args.input);
    let report = editor.save(output)?;
    if !report.removed_signatures.is_empty() {
        eprintln!("Removed invalidated OpenPGP signatures; output is unsigned");
    }
    if !report.removed_file_signatures.is_empty() {
        eprintln!("Removed file-content signatures; re-signing is required to restore them");
    }
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Wrote {} (unsigned)", report.output.display());
    }
    Ok(())
}
fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
