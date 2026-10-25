# rpm-utils-rs

[![Rust](https://github.com/AnderEnder/rpm-utils-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/AnderEnder/rpm-utils-rs/actions/workflows/rust.yml)
[![codecov](https://codecov.io/gh/AnderEnder/rpm-utils-rs/branch/master/graph/badge.svg)](https://codecov.io/gh/AnderEnder/rpm-utils-rs)

**Experimental** Rust library for parsing and creating RPM packages based on the [RPM v4 specification](https://rpm.org/docs/6.0.x/manual/format_v4.html).

## Features

- Parse RPM package headers and metadata
- Create RPM packages programmatically
- CPIO archive support (RPM payload format)
- Multiple compression formats: gzip, bzip2, xz, lzma, zstd, uncompressed

## CLI Tools

- `rpm-info` - Display RPM package information
- `rpm2cpio` - Extract CPIO payload from RPM packages
- `cpio-create` - Create CPIO archives
- `cpio-extract` - Extract CPIO archives

## Usage

```bash
# Build the project
cargo build --release

# View RPM package info
cargo run --bin rpm-info -- /path/to/package.rpm

# Check integrity digests (does not authenticate an OpenPGP signature)
cargo run --bin rpm-info -- --verify /path/to/package.rpm

# Extract CPIO from RPM
cargo run --bin rpm2cpio -- /path/to/package.rpm > payload.cpio
```

Create a complete unsigned RPM v4 package:

```rust,no_run
use rpm_utils::RPMBuilder;

# fn main() -> std::io::Result<()> {
RPMBuilder::new()
    .filename("tool-1.0.rpm")
    .package_name("tool")
    .version("1.0")
    .license("MIT".into())
    .summary("Example tool")
    .add_file_as("target/release/tool", "/usr/bin/tool")
    .build_package()?;
# Ok(())
# }
```

`build_package()` includes configured files, recursive directories and Unix
symbolic links, file metadata, SHA-256 file and payload digests, and RPM v4
immutable regions with the standard header digests. It stages data in temporary
files and replaces the destination after the package is complete. It does not
create an OpenPGP signature. `build()` remains the low-level API for callers
writing their own header and payload. Parsed packages expose
`verify_integrity()` to check available supported digests over the complete
header and payload. Verification fails if supported digest coverage is incomplete;
it does not establish the identity of a package signer.

`add_file_as(source, destination)` maps an explicit installation path. `add_file`,
`add_link` and `add_directory` preserve relative source paths; absolute sources
use their final filename as the package path. Directory inputs are recursive.

CPIO supports regular files and directory entries, plus symbolic links on Unix.
Extraction reconstructs hard-link groups and restores directory metadata after
children are extracted. Readers work with streams, including decompressor output.
Special files return an unsupported-type error; extraction requires a destination
directory that other processes cannot modify concurrently.
RPM epochs use `u32` throughout the metadata API.

The current implementation targets legacy v3/v4 packages; RPM v6 is a distinct
package format and is not supported. An explicit `RPMTAG_RPMFORMAT` value other
than 3 or 4 is rejected. The historical lead version is not sufficient to identify
the package format. See the upstream [v4](https://rpm.org/docs/6.0.x/manual/format_v4.html)
and [v6](https://rpm.org/docs/6.0.x/manual/format_v6.html) format descriptions.

## Requirements

- Rust 2024 edition (1.88.0+)

## Status

This is an experimental project. APIs may change without notice.

## License

No license file has been provided in this repository.
