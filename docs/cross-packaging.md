# Cross-platform RPM packaging

RPM creation runs on the packaging host and does not execute the packaged files.
Use `RPMBuilder::target()` to select Linux package metadata and validate recognized
standalone native files independently of that host:

```rust,no_run
use rpm_utils::RPMBuilder;
fn main() -> std::io::Result<()> {
    RPMBuilder::new()
        .filename("tool.aarch64.rpm")
        .package_name("tool").version("1.0")
        .target("aarch64-unknown-linux-musl")?
        .add_file_as("target/aarch64-unknown-linux-musl/release/tool", "/usr/bin/tool")
        .build_package()?;
    Ok(())
}
```

Supported package architectures are `x86_64`, `aarch64`, `i686`, `riscv64`,
`ppc64le`, `s390x` and `noarch`. `amd64` and `arm64` are aliases. Supported
Linux GNU/musl target triples map to their RPM architecture; the header OS is
Linux. Cross-compilation still requires the compiler, linker and target libraries
for the chosen ABI. Selecting an architecture does not convert an existing binary.
The legacy `arch()` setter remains an unchecked metadata-only interface.

For JVM code, use `target("noarch")` and map JARs/classes and launch scripts into
the desired installed paths. Build-time Java tools are separate from package
creation. Native JNI libraries require an architecture-specific package.

Validation checks the ELF machine, word size and byte order and rejects recognized
Mach-O and Windows executables. It accepts JARs and Java class magic (which overlaps
Mach-O universal magic). ELF libc, ABI versions, CPU extensions and runtime library
requirements are not inferred. Nested archives/JAR contents are not inspected for
native code. This is a target sanity check, not a complete executable validator.

CPIO numeric ownership defaults to root (`0:0`), independent of the macOS or other
host account. Header ownership names default to `root:root`; callers choosing
custom owners must set both names and `payload_owner(uid, gid)` as appropriate.

## Standalone build command

Install this repository's tools with `cargo install --path . --locked`. This
installs our `cargo-rpm` executable along with `rpm-build` and the inspection tools.
If another crate already installed a binary called `cargo-rpm`, Cargo reports that
collision; choose which implementation to keep before installing.

Write `rpm.toml` for prebuilt JVM/data files:

```toml
name = "my-app"
version = "1.0.0"
target = "noarch"
summary = "My JVM application"
license = "Apache-2.0"
requires = ["java-headless"]
source-date-epoch = 1700000000

[[files]]
source = "build/libs/my-app.jar"
dest = "/usr/share/my-app/my-app.jar"
mode = "0644"

[[files]]
source = "packaging/my-app"
dest = "/usr/bin/my-app"
mode = "0755"
```

Run `rpm-build --manifest rpm.toml`, or build first with
`rpm-build --manifest rpm.toml -- ./gradlew build`. Commands after `--` run directly
without a shell, in the manifest's directory. Arguments remain separate, including
paths with spaces. Use `--build-env KEY=VALUE` to set compiler environment variables.
The command receives `RPM_TARGET_ARCH`, `RPM_TARGET_OS`, and the selected
`SOURCE_DATE_EPOCH`. A failed command stops packaging and preserves any existing
RPM output. The tool does not provision compilers or sandbox the build command.

For C/C++ cross-compilers, use an architecture-specific manifest and pass your
compiler or build system after `--`, for example:

```sh
rpm-build --manifest rpm.toml --target aarch64 -- \
  zig cc -target aarch64-linux-musl -O2 -o build/tool src/tool.c
```

The manifest must map `build/tool` to the installed path. Compiler output
directories must already exist or be created by your build system.

Relative source and manifest-output paths resolve beside the manifest;
`--output` resolves from the command's working directory. Directory sources are
included recursively; a mode override affects only its explicitly named path.
Runtime `requires` currently accepts unversioned names. Dependency generation,
versioned requirements and JVM dependency analysis are not implemented.

## Our Cargo integration

Our `cargo rpm` reads `[package.metadata.rpm]` from the selected Cargo package,
resolves workspace members and the target directory using `cargo metadata`, runs
`cargo build`, and packages the result. It does not call another RPM Cargo plugin,
`rpmbuild`, a container, or an installed Linux RPM library.

```toml
[package.metadata.rpm]
# This can also be provided by cargo rpm --target.
target = "aarch64-unknown-linux-musl"
summary = "My cross-compiled tool"
source-date-epoch = 1700000000
```

With a single binary target, no file list is necessary: the reported executable
is installed as `/usr/bin/<binary>` with mode `0755`. Multiple binary targets
require `--bin` or an explicit file list. Package name, version, license and
description default to Cargo's resolved package metadata, including workspace
inheritance. An explicit file list replaces the default binary selection:

```toml
[[package.metadata.rpm.files]]
source = "{target-dir}/{target}/{profile}/{bin}"
dest = "/usr/bin/my-tool"
mode = "0755"

[[package.metadata.rpm.files]]
source = "packaging/config.toml"
dest = "/etc/my-tool/config.toml"
mode = "0644"
```

`{target-dir}` is Cargo's resolved target directory; `{target}` is the compiler
triple; `{profile}` is the artifact profile directory (`dev` maps to `debug`);
`{bin}` requires a single binary or `--bin`. Other relative source paths resolve
beside the selected package's `Cargo.toml`, including for workspace members.

```sh
cargo rpm --target aarch64-unknown-linux-musl --locked
cargo rpm --target x86_64-unknown-linux-gnu --profile release --bin my-tool
cargo rpm --package workspace-member --target aarch64-unknown-linux-musl
cargo rpm --no-build --target aarch64-unknown-linux-musl
```

The default profile is `release`. `--features`, `--all-features`,
`--no-default-features`, `--offline` and `--locked` are passed to Cargo as
applicable. `--manifest-path` selects the workspace/package manifest. `--no-build`
packages existing artifacts and still validates their target; provide explicit
file sources for unusual artifact locations. `target = "noarch"` with an explicit
file list and `--no-build` supports JVM/assets in Cargo-managed repositories.
A declared package architecture that disagrees with `--target` is rejected.

Configure your Linux linker through Cargo's normal target configuration or
`CARGO_TARGET_<TRIPLE>_LINKER` before running the command. Installing Rust's target
standard library alone may not supply a linker or sysroot. Cargo build scripts
and native dependencies may require additional target-specific compiler settings.
GNU versus musl remains a compiler/sysroot decision; both use the same RPM CPU
architecture. The package tool does not infer a compatible glibc floor.

## Repeatable packaging

`source-date-epoch` in a manifest, the CLI `--source-date-epoch`, or the
`SOURCE_DATE_EPOCH` environment variable enables deterministic packaging.
CLI values override the manifest; the manifest overrides the environment.
The library equivalent is `RPMBuilder::reproducible(epoch)`.

This fixes build time, all packaged mtimes, and the default build-host label.
Package paths are sorted, inode IDs follow included files, numeric ownership is
explicit, and hardlink counts include only packaged paths. File `mode` overrides
remove reliance on host permissions. An explicit `build-host` overrides the fixed
label. Invalid environment timestamps fail rather than silently using the clock.
Identical package inputs and compressor versions produce identical bytes; the
compiler must also produce repeatable artifacts. Cross-platform compressor
version parity and arbitrary compiler reproducibility are not guaranteed.
