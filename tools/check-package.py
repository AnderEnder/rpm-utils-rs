#!/usr/bin/env python3
"""Test the distributable crate and install its default-enabled CLI in isolation."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile


def run(*args, cwd, env=None):
    print("+ " + " ".join(map(str, args)), flush=True)
    subprocess.run(args, cwd=cwd, env=env, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow-dirty", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        cwd=root,
    ))
    package = next(p for p in metadata["packages"] if p["name"] == "rpm-utils")
    target = Path(metadata["target_directory"])
    package_args = ["--allow-dirty"] if args.allow_dirty else []
    run("cargo", "package", "--locked", *package_args, cwd=root)
    stem = f'{package["name"]}-{package["version"]}'
    archive = target / "package" / f"{stem}.crate"
    env = dict(os.environ, CARGO_TARGET_DIR=str(target))
    with tempfile.TemporaryDirectory(prefix="rpm-utils-package-") as temporary:
        scratch = Path(temporary)
        with tarfile.open(archive, "r:gz") as crate:
            crate.extractall(scratch, filter="data")
        source = scratch / stem
        for flags in [[], ["--no-default-features"]]:
            run("cargo", "test", "--locked", "--all-targets", *flags, cwd=source, env=env)
            run("cargo", "test", "--locked", "--doc", *flags, cwd=source, env=env)
        tree = subprocess.check_output(
            ["cargo", "tree", "--locked", "--no-default-features", "-e", "normal", "--prefix", "none"],
            cwd=source, env=env, text=True,
        )
        assert not any(line.split()[0] in {"clap", "serde_json"} for line in tree.splitlines()), tree
        destination = scratch / "install"
        run("cargo", "install", "--path", str(source), "--locked", "--offline",
            "--root", str(destination), "--target-dir", str(target), cwd=source, env=env)
        suffix = ".exe" if os.name == "nt" else ""
        for binary in package["targets"]:
            if "bin" in binary["kind"]:
                run(str(destination / "bin" / (binary["name"] + suffix)), "--help", cwd=scratch)
    print(f"Verified {archive.name}: packaged tests, doctests, dependencies, and CLI installation.")


if __name__ == "__main__":
    main()
