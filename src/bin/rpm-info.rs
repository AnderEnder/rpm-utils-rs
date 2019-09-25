use rpm_utils::{RPMFile, RPMInfo};
use std::io;
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;

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
        println!("{:?}", file.indexes);
        println!("{:?}", file.sigtags);
        println!("{:?}", file.header);
        println!("{:?}", file.h_indexes);
        println!("{:?}", file.tags);
    } else {
        let info: RPMInfo = file.into();
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
