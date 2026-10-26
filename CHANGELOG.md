# Changelog

## 0.2.0 — Initial crates.io release

- Read legacy RPM v3/v4 packages and build complete unsigned RPM v4 packages.
- Stream newc/crc CPIO payloads with supported compression, checksum validation,
  hard-link reconstruction, and Unix symbolic-link support.
- Verify package integrity without authenticating OpenPGP signatures.
- Compare package metadata and installed content; preview, edit, lint, and repair
  packages while preserving opaque metadata where supported.
- Build Linux architecture and JVM/data noarch packages through TOML manifests
  and `cargo rpm-build`, including cross-packaging and reproducible timestamps.
- Provide nine command-line tools through the default-enabled `cli` feature;
  library users can disable default features to omit CLI dependencies.
- License the project under MIT and track the latest stable Rust toolchain
  (Rust 1.99.1 at release preparation).
- Deprecate `RPMInfo::into_rpm()` in favor of fallible `try_into_rpm()`.

RPM v6, files requiring RPM's `07070X` payload, signing/authentication, dependency
resolution, and package installation are outside this release. See the README
for API compatibility, platform support, and extraction constraints.
