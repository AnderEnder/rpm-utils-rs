# rpm-utils-rs

[![Rust](https://github.com/AnderEnder/rpm-utils-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/AnderEnder/rpm-utils-rs/actions/workflows/rust.yml)
[![codecov](https://codecov.io/gh/AnderEnder/rpm-utils-rs/branch/master/graph/badge.svg)](https://codecov.io/gh/AnderEnder/rpm-utils-rs)

**Experimental** Rust library for parsing and creating RPM packages based on the [RPM specification](https://rpm-software-management.github.io/rpm/manual/format.html).

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

# Extract CPIO from RPM
cargo run --bin rpm2cpio -- /path/to/package.rpm > payload.cpio
```

CPIO creation and extraction currently support regular files and directory entries.
Symbolic links and special files return an unsupported-type error. Directory inputs
are not traversed recursively, and hard-link relationships are not reconstructed.
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

See LICENSE file for details.
