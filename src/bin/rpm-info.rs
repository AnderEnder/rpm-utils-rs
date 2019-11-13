use rpm_utils::{RPMFile, RPMInfo};
use std::io;
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "rpm-info")]
struct Args {
    /// Path to data file
    #[structopt(name = "path", parse(from_os_str))]
    path: PathBuf,

    /// Show internal debug information
    #[structopt(long = "debug", short = "d")]
    debug: bool,
}

fn run(args: Args) -> Result<(), io::Error> {
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
    let args = Args::from_args();
    if let Err(err) = run(args) {
        eprintln!("{}", err);
        exit(1);
    }
}
