# Implementation status

Release preparation reviewed and tested on 2026-10-26; format and interoperability
review originally completed on 2026-10-25.

## First-release preparation

- MIT license, complete registry metadata, and explicit crate archive contents.
- Latest stable Rust policy (currently 1.99.1) in the manifest, toolchain file,
  README, and CI.
- Default-enabled `cli` feature gates all nine binaries and their Clap/JSON
  dependencies. Library-only consumers retain all library APIs.
- Explicit RPM API exports, documented lossy metadata views, and deprecation of
  the panic-based `RPMInfo::into_rpm()` in favor of `try_into_rpm()`.
- README-backed crate documentation with an executable create/read/verify/edit
  example, pre-1.0 compatibility policy, changelog, and release checklist.
- CI documentation/doctest checks and isolated tests and CLI installation from
  the actual packaged source, in addition to existing platform/native RPM jobs.

## Implemented in this review

- **Complete unsigned RPM v4 creation:** `RPMBuilder::build_package()` stages a
  payload, includes configured files and recursive directories, maps installation
  paths, records file metadata, and publishes the finished package atomically.
  Existing `build()` remains the low-level metadata/writer API.
- **RPM v4 structure and integrity:** immutable signature/header regions, correctly
  typed sizes and timestamps, per-file SHA-256, compressed and uncompressed payload
  SHA-256, header SHA-1/SHA-256, and the legacy header-plus-payload MD5 checksum.
  Legacy hashes are emitted for format compatibility, not signer authentication.
- **Package integrity checking:** `verify_integrity()` and `rpm-info --verify`
  check supported stored digests and sizes, require coverage of both header and
  payload, and preserve the reader position even on errors.
- **CPIO streaming and links:** newc and crc records, checked payload boundaries,
  crc byte sums, symbolic links on Unix, hard-link reconstruction with data in any
  member, and deferred directory/hard-link metadata restoration.
- **Safe failure behavior:** validation precedes opening builder output, full
  builds and payload copies preserve existing output on failure, dangling symlink
  escapes are rejected, and replacing a pre-existing hard link preserves aliases.
- **Metadata conversion:** file lists and digest algorithms survive supported
  `RPMInfo` conversions. `try_into_rpm()` reports invalid timestamp errors.
- **Format limits:** streaming supports the full unsigned 32-bit newc size range.
  Name allocation remains bounded; source timestamps and oversized files are
  rejected rather than truncated. Device fields use native platform decoding.

- **Editing:** `RPMEditor` and `rpm-edit` modify textual/typed metadata, replace,
  add or remove regular files, and change installed permissions. Unknown metadata
  and other translations are preserved. Metadata-only edits preserve compressed
  payload bytes; changed packages get fresh digests and are unsigned.
  `RPMEditor::preview()` and `rpm-edit --dry-run` build and validate proposed edits
  in scratch storage and report changes without publishing a package.
- **Cross-packaging:** `rpm-build` and our own `cargo rpm-build` support explicit Linux
  architectures and JVM/data `noarch` packages from macOS or other hosts. TOML
  manifests, optional build commands, Cargo workspace/profile resolution, target
  sanity checks, installed mode/ownership control and reproducible timestamps are
  implemented. See [cross-packaging](cross-packaging.md). Linux dependency/ABI
  generation and toolchain provisioning remain external.
- **Comparison:** `diff_rpm` and `rpm-diff` compare raw metadata/signature values
  and actual installed content, including modes, ownership and hardlink groups.
  Text and JSON reports use deterministic tag/path order.
- **Lint and repair:** `lint_rpm`, `repair_rpm` and `rpm-lint` expose structured
  diagnostics, JSON output and atomic repair of reconstructable values. Ambiguous
  content, ownership and mode errors are refused; clean signed inputs are copied
  byte-for-byte. See [editing and linting](editing-and-linting.md).

## Validation

The packaged source passes 118 tests with default features and 107 library-only
tests on macOS, plus the README doctest in both configurations. Documentation
with warnings denied, strict Clippy in both configurations, formatting, and
`git diff --check` pass. All nine tools install into an isolated root and answer
`--help`; library-only normal dependencies omit Clap and serde_json.

Regression tests independently inspect region
trailers and digest values, exercise all six compressor choices, cover damaged
packages and failure preservation, and check extraction contents and inode/link
behavior. The system libarchive reader can read generated RPM packages. The
2026-10-25 review also verified an independently constructed crc fixture with
native cpio and the Rust extractor.

These checks do not establish installation compatibility with every RPM release.
Linux and Windows execution and native RPM compatibility are covered by hosted
CI. The new release-check jobs must pass on the exact release commit before
publication; a local macOS run does not replace that gate.

## Explicit remaining boundaries

- RPM v6 and RPM's `07070X` payload format for files exceeding the newc limit are
  not implemented. Header lead version and package format are distinct concepts.
- OpenPGP signing, signer authentication, dependency resolution, and package
  installation are not provided. Integrity verification detects corruption only.
- Device nodes, FIFOs and sockets are rejected. Windows symbolic-link extraction
  is not implemented.
- Legacy enum-based `Tags` and `RPMInfo` expose a subset of metadata: unknown IDs
  are coalesced and only the first I18N translation is exposed. Use `RawHeader`
  and `RPMEditor` for lossless opaque values and translations. Unknown arrays that
  could follow changed file entries require an explicit application decision;
  editing does not infer binary dependencies or authenticate signatures.
- Extraction requires an exclusive destination directory. Path checks do not
  defend against another process concurrently replacing its ancestors.
