use clap::Parser;
use rpm_utils::{RPMFile, RPMInfo};
use std::io;
use std::path::PathBuf;
use std::process::exit;

#[derive(Debug, Parser)]
#[command(name = "rpm-info")]
struct Args {
    /// Path to data file
    #[arg(name = "path")]
    path: PathBuf,

    /// Show internal debug information
    #[arg(long = "debug", short = 'd')]
    debug: bool,
}

fn run(args: Args) -> io::Result<()> {
    let file = RPMFile::open(args.path)?;
    let info: RPMInfo = (&file).into();

    if args.debug {
        println!("{:#?}", file.signature_tags);
        println!("{:#?}", file.header_tags);
        println!("{:#?}", info);
    } else {
        println!("{}", info);
    }
    Ok(())
}

fn main() {
    let args = Args::parse();
    if let Err(err) = run(args) {
        eprintln!("{}", err);
        exit(1);
    }
}
