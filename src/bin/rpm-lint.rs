use clap::Parser;
use rpm_utils::{lint_rpm, repair_rpm};
use std::{io, path::PathBuf, process::ExitCode};

#[derive(Debug, Parser)]
#[command(about = "Inspect RPM metadata, payload and integrity; repair reconstructable values")]
struct Args {
    input: PathBuf,
    /// Print stable issue codes, severity, tag IDs and fixability as JSON
    #[arg(long)]
    json: bool,
    /// Rebuild layout, sizes and digests; edited output is unsigned
    #[arg(long)]
    fix: bool,
    #[arg(short, long, requires = "fix", conflicts_with = "in_place")]
    output: Option<PathBuf>,
    /// Atomically replace input when repair succeeds
    #[arg(long, requires = "fix")]
    in_place: bool,
}
fn run(args: Args) -> io::Result<bool> {
    let report = if args.fix {
        let destination = if args.in_place {
            &args.input
        } else {
            args.output.as_ref().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--fix requires --output or --in-place",
                )
            })?
        };
        let report = repair_rpm(&args.input, destination)?;
        eprintln!(
            "Repair completed: {}. Rewritten packages are unsigned.",
            destination.display()
        );
        report
    } else {
        lint_rpm(&args.input)
    };
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if report.is_clean() {
        println!("{}: OK", report.path.display());
    } else {
        for issue in &report.issues {
            println!(
                "{:?} {}: {}{}",
                issue.severity,
                issue.code,
                issue.message,
                if issue.fixable { " [fixable]" } else { "" }
            );
        }
    }
    Ok(!report.has_errors())
}
fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
