This fixture verifies cross-compilation and RPM packaging on macOS and Linux.
It is a freestanding ELF binary that loops; it is never installed or executed.
It deliberately needs no Linux C compiler, libc or sysroot at link time:

```sh
rustup target add aarch64-unknown-linux-musl x86_64-unknown-linux-musl
RUSTFLAGS='-C linker=rust-lld -C linker-flavor=ld.lld -C link-self-contained=no -C link-arg=--entry=_start' \
  cargo rpm-build --manifest-path examples/cross-smoke/Cargo.toml \
    --target aarch64-unknown-linux-musl --locked --output /tmp/smoke.aarch64.rpm
```

Normal Rust applications using `std`, C dependencies, or dynamic libraries need
the appropriate linker/sysroot configuration. These freestanding linker flags
are only for this fixture. CI sends packages created on both hosts to Linux for
independent native RPM digest and metadata checks.
