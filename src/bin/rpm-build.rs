use clap::Parser;
use rpm_utils::PackageManifest;
use std::{
    ffi::OsString,
    io,
    path::PathBuf,
    process::{Command, ExitCode},
};

#[derive(Parser)]
#[command(about = "Build an unsigned Linux/noarch RPM from a TOML manifest on any host")]
struct Args {
    #[arg(short, long, default_value = "rpm.toml")]
    manifest: PathBuf,

    #[arg(short, long)]
    output: Option<PathBuf>,

    #[arg(long, alias = "arch")]
    target: Option<String>,

    #[arg(long)]
    source_date_epoch: Option<u32>,

    /// Environment overrides for the optional build command (KEY=VALUE)
    #[arg(long, requires = "command")]
    build_env: Vec<String>,

    /// Optional compiler/build command after --; executed directly, without a shell
    #[arg(last = true)]
    command: Vec<OsString>,
}

fn run(args: Args) -> io::Result<()> {
    let manifest_path = args.manifest.canonicalize()?;
    let base = manifest_path.parent().unwrap();

    let mut manifest = PackageManifest::from_toml(&std::fs::read_to_string(&manifest_path)?)?;

    if let Some(target) = args.target {
        manifest.target = Some(target);
    }

    if let Some(epoch) = args.source_date_epoch {
        manifest.source_date_epoch = Some(epoch);
    }

    let target = manifest.package_target()?;
    let epoch = manifest.reproducible_epoch()?;

    if let Some(program) = args.command.first() {
        let mut command = Command::new(program);
        command
            .args(&args.command[1..])
            .current_dir(base)
            .env("RPM_TARGET_ARCH", target.arch())
            .env("RPM_TARGET_OS", target.os());
        if let Some(epoch) = epoch {
            command.env("SOURCE_DATE_EPOCH", epoch.to_string());
        }
        for value in args.build_env {
            let (key, value) = value
                .split_once('=')
                .filter(|(key, _)| !key.is_empty())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Expected build environment KEY=VALUE",
                    )
                })?;
            if key.contains('\0') || value.contains('\0') {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "NUL in build environment",
                ));
            }
            command.env(key, value);
        }
        let status = command.status()?;
        if !status.success() {
            return Err(io::Error::other(format!("Build command failed: {status}")));
        }
    }

    let output = manifest.build(base, args.output.as_deref())?;
    println!("Wrote {} (unsigned)", output.display());
    Ok(())
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
