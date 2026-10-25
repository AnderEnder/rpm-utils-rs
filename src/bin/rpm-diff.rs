use clap::Parser;
use rpm_utils::diff_rpm;
use std::{io, path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(about = "Compare RPM v3/v4 metadata, signatures and actual installed file content")]
struct Args {
    before: PathBuf,
    after: PathBuf,
    /// Include complete before/after values as JSON
    #[arg(long)]
    json: bool,
}
fn run(args: Args) -> io::Result<bool> {
    let report = diff_rpm(args.before, args.after)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for tag in &report.metadata {
            println!("Metadata tag {} changed", tag.id);
        }
        for tag in &report.signatures {
            println!("Signature tag {} changed", tag.id);
        }
        for file in &report.files {
            let action = match (&file.before, &file.after) {
                (None, _) => "Added",
                (_, None) => "Removed",
                _ => "Changed",
            };
            println!("{action} {}", file.path);
        }
        if report.is_empty() {
            println!("No value differences");
        }
    }
    Ok(report.is_empty())
}
fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
