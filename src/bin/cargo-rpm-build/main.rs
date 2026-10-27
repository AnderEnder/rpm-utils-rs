//! Our Cargo subcommand: cargo rpm-build, configured in package.metadata.rpm.
use clap::Parser;
use std::{path::PathBuf, process::ExitCode};

mod workflow;

#[derive(Parser)]
#[command(
    name = "cargo rpm-build",
    about = "Build and package Cargo projects as unsigned Linux RPMs"
)]
struct Args {
    /// Path to the Cargo package or workspace manifest
    #[arg(long, default_value = "Cargo.toml")]
    manifest_path: PathBuf,

    /// Select a package in the Cargo workspace
    #[arg(short, long)]
    package: Option<String>,

    /// Rust Linux target triple; inferred from metadata.rpm.target when possible
    #[arg(long)]
    target: Option<String>,

    /// Cargo profile (release by default; dev artifacts are in debug)
    #[arg(long, default_value = "release")]
    profile: String,

    /// Select the binary to build and package
    #[arg(long)]
    bin: Option<String>,

    /// Write the RPM to this path
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Package existing files without invoking cargo build
    #[arg(long)]
    no_build: bool,

    /// Enable Cargo features; may be specified more than once
    #[arg(long)]
    features: Vec<String>,

    /// Enable all Cargo features
    #[arg(long, conflicts_with = "no_default_features")]
    all_features: bool,

    /// Disable the package's default Cargo features
    #[arg(long)]
    no_default_features: bool,

    /// Require Cargo.lock to be up to date
    #[arg(long)]
    locked: bool,

    /// Run Cargo without network access
    #[arg(long)]
    offline: bool,

    /// Override the reproducible build timestamp, in seconds since the Unix epoch
    #[arg(long)]
    source_date_epoch: Option<u32>,
}

fn main() -> ExitCode {
    // Cargo invokes cargo-rpm-build with the subcommand name as its first argument.
    let mut arguments: Vec<_> = std::env::args_os().collect();

    if arguments.get(1).is_some_and(|arg| arg == "rpm-build") {
        arguments.remove(1);
    }

    match workflow::run(Args::parse_from(arguments)) {
        Ok(output) => {
            println!("Wrote {} (unsigned)", output.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
