//! Our Cargo subcommand: cargo rpm-build, configured in package.metadata.rpm.
use clap::Parser;
use rpm_utils::{ManifestFile, PackageManifest, PackageTarget};
use serde_json::Value;
use std::{
    ffi::OsString,
    io,
    path::PathBuf,
    process::{Command, ExitCode, Stdio},
};

#[derive(Parser)]
#[command(
    name = "cargo rpm-build",
    about = "Build and package Cargo projects as unsigned Linux RPMs"
)]
struct Args {
    #[arg(long, default_value = "Cargo.toml")]
    manifest_path: PathBuf,

    #[arg(short, long)]
    package: Option<String>,

    /// Rust Linux target triple; inferred from metadata.rpm.target when possible
    #[arg(long)]
    target: Option<String>,

    /// Cargo profile (release by default; dev artifacts are in debug)
    #[arg(long, default_value = "release")]
    profile: String,

    #[arg(long)]
    bin: Option<String>,

    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Package existing files without invoking cargo build
    #[arg(long)]
    no_build: bool,

    #[arg(long)]
    features: Vec<String>,

    #[arg(long, conflicts_with = "no_default_features")]
    all_features: bool,

    #[arg(long)]
    no_default_features: bool,

    #[arg(long)]
    locked: bool,

    #[arg(long)]
    offline: bool,

    #[arg(long)]
    source_date_epoch: Option<u32>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn run(args: Args) -> io::Result<()> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let manifest_path = args.manifest_path.canonicalize()?;

    let mut metadata_command = Command::new(&cargo);
    metadata_command
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(&manifest_path);

    if args.locked {
        metadata_command.arg("--locked");
    }

    if args.offline {
        metadata_command.arg("--offline");
    }

    let result = metadata_command.output()?;

    if !result.status.success() {
        return Err(io::Error::other(format!(
            "Cargo metadata failed: {}",
            String::from_utf8_lossy(&result.stderr)
        )));
    }

    let metadata: Value = serde_json::from_slice(&result.stdout)?;

    let packages = metadata["packages"]
        .as_array()
        .ok_or_else(|| invalid("Missing Cargo packages"))?;

    let exact = packages.iter().find(|package| {
        package["manifest_path"]
            .as_str()
            .is_some_and(|path| std::path::Path::new(path) == manifest_path)
    });

    let candidates: Vec<_> = if let Some(name) = &args.package {
        packages
            .iter()
            .filter(|p| p["name"].as_str() == Some(name))
            .collect()
    } else if let Some(package) = exact {
        vec![package]
    } else {
        packages
            .iter()
            .filter(|p| {
                metadata["workspace_default_members"]
                    .as_array()
                    .is_some_and(|ids| ids.contains(&p["id"]))
            })
            .collect()
    };

    if candidates.len() != 1 {
        return Err(invalid("Select one Cargo workspace package with --package"));
    }

    let package = candidates[0];
    let name = package["name"]
        .as_str()
        .ok_or_else(|| invalid("Missing Cargo package name"))?;

    let base = PathBuf::from(
        package["manifest_path"]
            .as_str()
            .ok_or_else(|| invalid("Missing Cargo manifest path"))?,
    );
    let base = base.parent().unwrap();

    let config = &package["metadata"]["rpm"];

    let mut spec: PackageManifest = serde_json::from_value(if config.is_null() {
        serde_json::json!({})
    } else {
        config.clone()
    })
    .map_err(|error| invalid(format!("Invalid package.metadata.rpm: {error}")))?;
    spec.name.get_or_insert_with(|| name.into());

    if spec.version.is_none() {
        spec.version = package["version"].as_str().map(Into::into);
    }

    if spec.description.is_none() {
        spec.description = package["description"].as_str().map(Into::into);
    }

    if spec.license.is_none() {
        spec.license = package["license"].as_str().map(Into::into);
    }

    if let Some(epoch) = args.source_date_epoch {
        spec.source_date_epoch = Some(epoch);
    }

    let compile_target = args
        .target
        .or_else(|| spec.target.as_ref().filter(|t| t.contains('-')).cloned());

    if let Some(target) = &compile_target {
        let parsed: PackageTarget = target.parse()?;
        if !target.contains('-') {
            return Err(invalid("--target requires a Linux compiler target triple"));
        }
        if let Some(declared) = &spec.target {
            if declared.parse::<PackageTarget>()?.arch() != parsed.arch() {
                return Err(invalid(
                    "Cargo target and RPM package architecture disagree",
                ));
            }
        } else {
            spec.target = Some(target.clone());
        }
    }

    let target = spec.package_target()?;

    if !args.no_build && target.arch() != "noarch" && compile_target.is_none() {
        return Err(invalid(
            "Cross-building requires --target with a Linux compiler target triple",
        ));
    }

    let epoch = spec.reproducible_epoch()?;
    let mut executables = std::collections::HashMap::new();

    if !args.no_build {
        let mut command = Command::new(&cargo);
        command
            .args([
                "build",
                "--message-format=json-render-diagnostics",
                "--manifest-path",
            ])
            .arg(&manifest_path)
            .args(["--package", name, "--profile", &args.profile])
            .current_dir(base)
            .stderr(Stdio::inherit());
        if let Some(target) = &compile_target {
            command.args(["--target", target]);
        }
        if let Some(bin) = &args.bin {
            command.args(["--bin", bin]);
        }
        if args.locked {
            command.arg("--locked");
        }
        if args.offline {
            command.arg("--offline");
        }
        if args.all_features {
            command.arg("--all-features");
        }
        if args.no_default_features {
            command.arg("--no-default-features");
        }
        for features in &args.features {
            command.args(["--features", features]);
        }
        if let Some(epoch) = epoch {
            command.env("SOURCE_DATE_EPOCH", epoch.to_string());
        }
        let result = command.output()?;
        for line in result.stdout.split(|b| *b == b'\n') {
            if let Ok(value) = serde_json::from_slice::<Value>(line) {
                if let Some(rendered) = value["message"]["rendered"].as_str() {
                    eprint!("{rendered}");
                }
                if let (Some(name), Some(path)) = (
                    value["target"]["name"].as_str(),
                    value["executable"].as_str(),
                ) {
                    executables.insert(name.to_owned(), PathBuf::from(path));
                }
            }
        }
        if !result.status.success() {
            return Err(io::Error::other(format!(
                "Cargo build failed: {}",
                result.status
            )));
        }
    }

    let target_dir = metadata["target_directory"]
        .as_str()
        .ok_or_else(|| invalid("Missing Cargo target directory"))?;

    let profile_dir = match args.profile.as_str() {
        "dev" | "test" => "debug",
        "bench" => "release",
        value => value,
    };

    let binary = if let Some(bin) = args.bin {
        Some(bin)
    } else {
        let bins: Vec<_> = package["targets"]
            .as_array()
            .ok_or_else(|| invalid("Missing Cargo targets"))?
            .iter()
            .filter(|t| {
                t["kind"]
                    .as_array()
                    .is_some_and(|k| k.iter().any(|v| v == "bin"))
            })
            .filter_map(|t| t["name"].as_str())
            .collect();
        if bins.len() == 1 {
            Some(bins[0].into())
        } else {
            None
        }
    };

    if spec.files.is_empty() {
        let bin = binary
            .as_deref()
            .ok_or_else(|| invalid("Select --bin or declare metadata.rpm.files"))?;
        if !args.no_build && !executables.contains_key(bin) {
            return Err(invalid(format!(
                "Cargo did not report an executable for {bin}"
            )));
        }
        let source = executables.remove(bin).unwrap_or_else(|| {
            let mut path = PathBuf::from(target_dir);
            if let Some(target) = &compile_target {
                path.push(target);
            }
            path.join(profile_dir).join(bin)
        });
        spec.files.push(ManifestFile {
            source,
            dest: format!("/usr/bin/{bin}"),
            mode: Some("0755".into()),
        });
    }

    for file in &mut spec.files {
        let path = file
            .source
            .to_str()
            .ok_or_else(|| invalid("RPM source path is not UTF-8"))?;
        if path.contains("{bin}") && binary.is_none() {
            return Err(invalid("{bin} requires --bin or a single binary target"));
        }
        file.source = path
            .replace("{target-dir}", target_dir)
            .replace("{target}", compile_target.as_deref().unwrap_or("noarch"))
            .replace("{profile}", profile_dir)
            .replace("{bin}", binary.as_deref().unwrap_or(""))
            .into();
    }

    let output = spec.build(base, args.output.as_deref())?;
    println!("Wrote {} (unsigned)", output.display());
    Ok(())
}

fn main() -> ExitCode {
    // Cargo invokes cargo-rpm-build with the subcommand name as its first argument.
    let mut arguments: Vec<_> = std::env::args_os().collect();

    if arguments.get(1).is_some_and(|arg| arg == "rpm-build") {
        arguments.remove(1);
    }

    match run(Args::parse_from(arguments)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
