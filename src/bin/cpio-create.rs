use clap::Parser;
use rpm_utils::payload::CpioBuilder;
use std::io;
use std::path::PathBuf;
use std::process::exit;

#[derive(Debug, Parser)]
#[command(name = "cpio-create")]
struct Args {
    /// Path to cpio file
    #[arg(name = "file", long = "file", short = 'f')]
    file: PathBuf,

    /// Target directory to extract
    #[arg(name = "path")]
    files: Vec<PathBuf>,
}

fn run(args: Args) -> io::Result<()> {
    let mut builder = CpioBuilder::open(&args.file)?;
    for path in args.files.into_iter() {
        builder = builder.add_raw_file(&path)?;
    }
    builder.build()
}

fn main() {
    let args = Args::parse();

    if let Err(err) = run(args) {
        eprintln!("{}", err);
        exit(1);
    }
}
