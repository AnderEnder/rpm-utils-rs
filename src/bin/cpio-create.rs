use rpm_utils::payload::CpioBuilder;
use std::io;
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "cpio-create")]
struct Args {
    /// Path to cpio file
    #[structopt(name = "file", long = "file", short = "f", parse(from_os_str))]
    file: PathBuf,

    /// Target directory to extract
    #[structopt(name = "path", parse(from_os_str))]
    files: Vec<PathBuf>,
}

fn run(args: Args) -> Result<(), io::Error> {
    let mut builder = CpioBuilder::open(&args.file)?;
    for path in args.files.into_iter() {
        builder = builder.add_raw_file(&path)?;
    }
    builder.build()
}

fn main() {
    let args = Args::from_args();

    if let Err(err) = run(args) {
        eprintln!("{}", err);
        exit(1);
    }
}
