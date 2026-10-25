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

    /// Destination file (defaults to stdout)
    #[arg(long = "output")]
    output: Option<PathBuf>,
}

fn run(args: Args) -> io::Result<()> {
    let rpm = RPMFile::open(args.path)?;
    if let Some(output) = args.output {
        rpm.copy_payload(&output)?;
    } else {
        rpm.copy_payload_to(&mut io::stdout().lock())?;
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
