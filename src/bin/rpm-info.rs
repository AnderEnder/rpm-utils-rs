use rpm_utils::RPMFile;
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;
use std::io;

#[derive(Debug, StructOpt)]
#[structopt(name = "rpm-info")]
struct Args {
    /// Outputs results in JSON form
    #[structopt(long = "json")]
    json: bool,

    /// Path to data file
    #[structopt(name = "path", parse(from_os_str))]
    path: PathBuf,

    /// Outputs results in JSON form
    #[structopt(long = "debug", short = "d")]
    debug: bool,
}

fn run(args: Args) -> Result<(), io::Error> {
    let file = RPMFile::open(args.path)?;
    if args.debug {
        println!("{:?}", file.lead);
        println!("{:?}", file.signature);
    } else {
        println!("{}", file.lead);
        println!("{:?}", file.signature);
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
