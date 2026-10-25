# rpm-utils-rs

[![Rust](https://github.com/AnderEnder/rpm-utils-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/AnderEnder/rpm-utils-rs/actions/workflows/rust.yml)
[![codecov](https://codecov.io/gh/AnderEnder/rpm-utils-rs/branch/master/graph/badge.svg)](https://codecov.io/gh/AnderEnder/rpm-utils-rs)

**Experimental** Rust library for parsing and creating RPM packages based on the [RPM v4 specification](https://rpm.org/docs/6.0.x/manual/format_v4.html).

## Features

- Parse RPM package headers and metadata
- Create RPM packages programmatically
- Edit metadata and files while preserving opaque header values
- Lint packages and repair reconstructable metadata and integrity values
- CPIO archive support (RPM payload format)
- Multiple compression formats: gzip, bzip2, xz, lzma, zstd, uncompressed

## CLI Tools

- `rpm-info` - Display RPM package information
- `rpm2cpio` - Extract CPIO payload from RPM packages
- `rpm-edit` - Modify metadata, files, or permissions and publish atomically
- `rpm-lint` - Inspect metadata, payload and integrity; optionally repair
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

Edit or inspect an existing RPM:

```bash
cargo run --bin rpm-edit -- package.rpm --output edited.rpm \
  --set 'summary=Updated summary' --epoch 2

cargo run --bin rpm-edit -- package.rpm --output edited.rpm \
  --replace-file /usr/bin/tool=target/release/tool \
  --add-file /etc/tool.conf=tool.conf --chmod /usr/bin/tool=0755

cargo run --bin rpm-lint -- package.rpm --json
cargo run --bin rpm-lint -- package.rpm --fix --output repaired.rpm
# Explicitly replace the input after successful repair:
cargo run --bin rpm-lint -- package.rpm --fix --in-place
```

Metadata-only edits preserve the compressed payload bytes. Edited packages have
fresh integrity digests and are **unsigned**: invalidated OpenPGP signatures are
removed and reported. Clean packages copied with `rpm-lint --fix` remain byte-for-byte
identical, including existing signatures. Lint checks integrity, not signer trust.

Repair reconstructs sizes, digests, alignment and immutable regions. It refuses
missing content, ambiguous file identity/ownership, conflicting modes, and other
problems it cannot repair reliably. Unknown metadata IDs and all translations are
preserved through the editor's raw header API. Opaque arrays that may follow file
entries must be explicitly updated or removed before changing the file list.
See [editing and linting](docs/editing-and-linting.md) for the API and boundaries.

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

CPIO reads and writes `newc` (`070701`) and SVR4 `crc` (`070702`) records,
validating the crc byte-sum on read, write, listing and extraction.
`FileEntry::checksum` selects crc when set; use `..Default::default()` when
constructing entries that do not require a checksum.

CPIO supports regular files and directory entries, plus symbolic links on Unix.
Extraction reconstructs hard-link groups and restores directory metadata after
children are extracted. Readers work with streams, including decompressor output.
Entry sizes use the full 32-bit newc range (up to 4 GiB minus one byte) without
buffering whole files. Larger files require RPM's separate `07070X` payload format,
which is not implemented. Special files return an unsupported-type error;
extraction requires a destination
directory that other processes cannot modify concurrently.
RPM epochs use `u32` throughout the metadata API. Metadata conversion preserves
file lists and their digest algorithm; use `RPMInfo::try_into_rpm()` to report
invalid timestamps as errors instead of using the legacy infallible conversion.

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

Compare package metadata and installed file content (exit 1 when different):

```sh
rpm-diff original.rpm modified.rpm
rpm-diff original.rpm modified.rpm --json
```

Preview and validate edits without writing a package:

```sh
rpm-edit original.rpm --set summary="Updated summary" --dry-run --json
```
