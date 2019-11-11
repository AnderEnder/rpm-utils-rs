use rpm_utils::RPMFile;
use std::io;
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "rpm-info")]
struct Args {
    /// Path to rpm file
    #[structopt(name = "rpm", parse(from_os_str))]
    path: PathBuf,

    /// Path to save file
    #[structopt(long = "output", parse(from_os_str))]
    output: PathBuf,
}

fn run(args: Args) -> Result<(), io::Error> {
    let rpm = RPMFile::open(args.path)?;
    rpm.copy_payload(&args.output)?;
    Ok(())
}

fn main() {
    let args = Args::from_args();
    if let Err(err) = run(args) {
        eprintln!("{}", err);
        exit(1);
    }
}
