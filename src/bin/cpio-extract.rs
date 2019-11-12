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

    /// Print debug information
    #[structopt(long = "debug", short = "d")]
    debug: bool,

    /// Target directory to extract
    #[structopt(short = "e", parse(from_os_str))]
    target_dir: PathBuf,
}

fn run(args: Args) -> Result<(), io::Error> {
    let mut file = File::open(&args.path)?;
    if args.debug {
        let entries = payload::read_entries(&mut file)?;
        for entry in &entries {
            println!("{:#?}", entry);
        }
    } else {
        let entries = payload::extract_entries(&mut file, &args.target_dir, true, false)?;
        for entry in &entries {
            println!("Extracting {}", &entry.name);
        }
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
