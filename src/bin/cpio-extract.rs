use clap::Parser;
use rpm_utils::payload;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::process::exit;

#[derive(Debug, Parser)]
#[command(name = "cpio-extract")]
struct Args {
    /// Path to data file
    #[arg(name = "path")]
    path: PathBuf,

    /// Print debug information
    #[arg(long = "debug", short = 'd')]
    debug: bool,

    /// Target directory to extract
    #[arg(short = 'e')]
    target_dir: PathBuf,
}

fn run(args: Args) -> io::Result<()> {
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
    let args = Args::parse();
    if let Err(err) = run(args) {
        eprintln!("{}", err);
        exit(1);
    }
}
