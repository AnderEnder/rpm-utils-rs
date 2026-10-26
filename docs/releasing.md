# Releasing rpm-utils

The first registry release is `0.2.0`. Releases use MIT licensing and the latest
stable Rust toolchain. Rust API, CLI, JSON, and manifest compatibility follow the
pre-1.0 policy in the README. Raising the compiler minimum follows the separate
latest-stable policy and must be reflected in the manifest and documentation.

## Prepare and validate

1. Review `Cargo.toml` metadata, public exports, README support boundaries, and
   `CHANGELOG.md`. Update the package version and its lockfile entry together.
2. Check the latest stable compiler with `rustup update stable`; align
   `rust-version` and the README's stated compiler version with the release.
3. Run the same checks as CI:

   ```sh
   cargo fmt --check
   cargo clippy --locked --all-targets -- -D warnings
   cargo clippy --locked --no-default-features --all-targets -- -D warnings
   cargo test --locked --all-targets
   cargo test --locked --no-default-features --all-targets
   cargo test --locked --doc
   cargo test --locked --no-default-features --doc
   RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --lib
   RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-default-features --no-deps --lib
   python3 tools/check-package.py
   cargo publish --dry-run --locked
   ```

   Package checks require Python 3.12+ and test the extracted archive in both
   feature configurations, run its doctests, check that normal library dependencies
   omit Clap/serde_json, and install all default CLI tools into a temporary root.
   During preparation, `tools/check-package.py --allow-dirty` checks uncommitted
   changes; the final release must be checked from a clean commit.
4. Review `cargo package --list` and the generated `target/package/*.crate`.
   The explicit include list carries source, tests, documentation, license,
   changelog, lockfile, and toolchain policy. CI configuration, release tooling,
   and the nested cross-compilation smoke project are repository-only files.
5. Require green CI on the exact release commit: Linux/macOS/Windows tests,
   docs/doctests, package checks, formatting, strict Clippy, cross-packaging,
   and native RPM validation. Native checks verify format/digests; they do not
   establish installation compatibility with every RPM release.

## Publish

Publishing is a separate maintainer action. A successful dry run builds the
archive but does not establish registry name availability, ownership, or account
readiness. Confirm the `rpm-utils` name is available (or owned by the publishing
account), verify the account email, and authenticate with `cargo login` or the
registry's supported CI authentication. Keep credentials out of the repository.

1. Ensure the release commit is clean and its version has not been published.
2. Create an annotated tag on that commit, e.g.
   `git tag -a v0.2.0 -m 'rpm-utils 0.2.0'`. If the tag already exists, verify it
   points to the checked commit; do not move a published release tag.
3. Run `cargo publish --locked` once the maintainer has authorized publication.
4. Verify the crates.io listing, docs.rs build, and
   `cargo install rpm-utils --version 0.2.0 --locked` using a temporary root.
5. Push the release commit and tag, and publish release notes from the changelog.

Release versions cannot be overwritten. Correct a defective release with a new
version; yank it if needed to prevent new dependency resolution. Publication
credentials and registry ownership are managed outside these source changes.
