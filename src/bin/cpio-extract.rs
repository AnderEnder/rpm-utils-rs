use rpm_utils::payload;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "cpio-extract")]
struct Args {
    /// Path to data file
    #[structopt(name = "path", parse(from_os_str))]
    path: PathBuf,

    /// Outputs results in JSON form
    #[structopt(long = "debug", short = "d")]
    debug: bool,
}

fn run(args: Args) -> Result<(), io::Error> {
    let mut file = File::open(args.path)?;
    let entries = payload::read_entries(&mut file)?;
    for entry in &entries {
        println!("{:#?}", entry);
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
