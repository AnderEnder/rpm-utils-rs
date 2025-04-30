use clap::Parser;
use rpm_utils::RPMFile;
use std::io;
use std::path::PathBuf;
use std::process::exit;

#[derive(Debug, Parser)]
#[command(name = "rpm2cpio")]
struct Args {
    /// Path to rpm file
    #[arg(name = "rpm")]
    path: PathBuf,

    /// Path to save file
    #[arg(long = "output")]
    output: PathBuf,
}

fn run(args: Args) -> io::Result<()> {
    let rpm = RPMFile::open(args.path)?;
    rpm.copy_payload(&args.output)?;
    Ok(())
}

fn main() {
    let args = Args::parse();
    if let Err(err) = run(args) {
        eprintln!("{}", err);
        exit(1);
    }
}
