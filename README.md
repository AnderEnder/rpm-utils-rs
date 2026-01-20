# rpm-utils-rs

[![Rust](https://github.com/AnderEnder/rpm-utils-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/AnderEnder/rpm-utils-rs/actions/workflows/rust.yml)
[![codecov](https://codecov.io/gh/AnderEnder/rpm-utils-rs/branch/master/graph/badge.svg)](https://codecov.io/gh/AnderEnder/rpm-utils-rs)

**Experimental** Rust library for parsing and creating RPM packages based on the [RPM specification](https://rpm-software-management.github.io/rpm/manual/format.html).

## Features

- Parse RPM package headers and metadata
- Create RPM packages programmatically
- CPIO archive support (RPM payload format)
- Multiple compression formats: gzip, bzip2, xz, zstd

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

## Requirements

- Rust 2024 edition (1.88.0+)

## Status

This is an experimental project. APIs may change without notice.

## License

See LICENSE file for details.
