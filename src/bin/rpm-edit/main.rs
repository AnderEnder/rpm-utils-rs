use clap::Parser;
use std::{io, path::PathBuf, process::ExitCode};

mod workflow;

#[derive(Debug, Parser)]
#[command(about = "Edit RPM v3/v4 metadata or files and write an unsigned package atomically")]
struct Args {
    /// Input RPM
    input: PathBuf,
    /// Output RPM
    #[arg(
        short,
        long,
        required_unless_present_any = ["in_place", "dry_run"],
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
    /// Build and validate a preview without writing input or output paths
    #[arg(long)]
    dry_run: bool,
    /// Print a machine-readable edit report
    #[arg(long)]
    json: bool,
}
fn run(args: Args) -> io::Result<()> {
    let editor = workflow::prepare(&args)?;
    if args.dry_run {
        let preview = editor.preview()?;
        if args.json {
            println!("{}", serde_json::to_string_pretty(&preview)?);
        } else {
            println!("Validated preview (no package written)");
            for tag in &preview.changes.metadata {
                println!("Metadata tag {} changed", tag.id);
            }
            for tag in &preview.changes.signatures {
                println!("Signature tag {} changed", tag.id);
            }
            for file in &preview.changes.files {
                let action = match (&file.before, &file.after) {
                    (None, _) => "Added",
                    (_, None) => "Removed",
                    _ => "Changed",
                };
                println!("{action} {}", file.path);
            }
            if !preview.removed_signatures.is_empty() {
                println!(
                    "Would remove OpenPGP signature tags: {:?}",
                    preview.removed_signatures
                );
            }
            if !preview.removed_file_signatures.is_empty() {
                println!(
                    "Would remove file-content signature tags: {:?}",
                    preview.removed_file_signatures
                );
            }
        }
        return Ok(());
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
