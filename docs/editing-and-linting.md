# Editing, linting and repair

## Editor

`RPMEditor` edits v3/v4 packages and publishes a validated result atomically.
It accepts both compressed directory/base-name tables and legacy full file-name
tables. Rewritten output uses v4 immutable regions and compressed file-name tables.
RPM v6 and the `07070X` large-file payload format remain unsupported.

```rust,no_run
use rpm_utils::RPMEditor;

fn main() -> std::io::Result<()> {
    let mut editor = RPMEditor::open("input.rpm")?;
    editor.set_text(1004, "Updated default summary")?;
    editor.replace_file("/usr/bin/tool", "target/release/tool")?;
    editor.add_file("tool.conf", "/etc/tool.conf")?;
    editor.set_file_mode("/usr/bin/tool", 0o755)?;
    editor.remove_file("/usr/share/tool/obsolete")?;
    let report = editor.save("output.rpm")?;
    println!("Removed signature tags: {:?}", report.removed_signatures);
    Ok(())
}
```

`set_tag(RawTag)` accepts explicitly typed, counted metadata. `remove_tag(id)`
removes optional metadata. Structural and payload-derived tags are managed by the
editor. Required identity fields cannot be removed. `set_text()` changes only the
first/default I18N translation and preserves all other translation bytes.

Metadata-only edits preserve the compressed payload byte-for-byte. File changes
rebuild the payload using the existing compressor, regenerate known file tables,
and retain existing ownership names and file flags. Added regular files default
to root ownership metadata. Replacement preserves installed permissions and
numeric ownership; it breaks a replaced file out of its old hard-link group.
Changing permissions on a hard-linked file changes all members of that group.
Symlink permission changes are rejected. Nonempty directories must have their
children removed first.

Payload and replacement content is staged on disk rather than accumulated in
memory. The output is validated before atomic replacement; failures leave an
existing destination untouched. Output filesystem permissions follow the existing
destination, or the input when creating a new file. Source and destination files should not be
concurrently modified by another process.

## Preservation and signatures

`RawHeader` and `RawTag` preserve unknown IDs, types, counts and network-order
value bytes, including multiple locales and CHAR arrays. Known file arrays are
realigned when entries change. The editor refuses to guess the relationship of
an unknown array to a changed file list; explicitly replace or remove that tag
when the application knows what it means. Dependencies and other inferred binary
metadata are not recalculated after replacing program content.

Changed packages are unsigned. The signature header is regenerated, stale
OpenPGP signatures and merged signature aliases are dropped, and the edit report
lists removed OpenPGP tag IDs in `removed_signatures`. File-content signatures are
also dropped when the payload changes; `removed_file_signatures` reports their tag
IDs, including any file-signature entries from the regenerated signature header. Unknown signature-header entries are not carried into a new
unsigned signature header. There is no signing or signer-authentication feature.

Header SHA-1/SHA-256 and header-plus-payload MD5 are regenerated for RPM v4
compatibility. Payload and regular-file digests are normalized to SHA-256. These
legacy integrity fields do not establish signer authenticity. The original lead
name cache is refreshed with NUL termination. Reserved bytes and index layout are
canonicalized, so edited RPM files are not byte-for-byte copies of their inputs.

## Lint and repair

`lint_rpm(path)` returns a `LintReport` with stable issue codes, severity,
optional numeric tag ID, and a `fixable` flag. It never edits the input. Checks
include parsing/bounds, duplicate IDs, index ordering/alignment, immutable
regions, required identity and locale metadata, file array lengths/types, file
inventory and modes/timestamps/link targets, content digests, and sizes.

`repair_rpm(input, output)` fixes reconstructable values through the same editor.
It recalculates file and package digests and sizes, normalizes index alignment,
and rebuilds immutable regions. If there are no repairable findings, it copies
the original bytes, preserving signatures. Unsupported or ambiguous errors stop
repair before publication. Repair does not recover corrupted content; it hashes
the supplied payload and cannot establish that those bytes are authentic.

The CLI supports `rpm-lint --json`. Errors give exit status 1 during inspection;
operational or refused-repair errors give status 2. Warnings alone return 0.
`--fix` requires `--output` or explicit `--in-place`. After repair, JSON and text
reports describe the result. `SIGNATURE_NOT_VERIFIED` warns about signatures whose
authenticity was not checked and does not trigger a rewrite on its own.

The editor/linter bounds raw header data and expanded tag values to 32 MiB,
file counts to 262,144, total expanded file-path bytes to 32 MiB, and locale counts
to 1,024. Oversized declarations fail with a diagnostic before expanding arrays.
The payload is staged on disk, so inspection and editing require temporary disk
space proportional to package content.

The linter covers the implemented schemas; it is not a complete validator for
every optional RPM extension. Unknown values are retained without interpreting
their semantics. Native RPM installation compatibility is not established by
these checks alone.

## Manual distribution-package checks

On 2026-10-25, lint and metadata editing were also exercised against signed RPMs
from the official AlmaLinux 9 BaseOS repository. Both originals reported only
`SIGNATURE_NOT_VERIFIED`. Both edited, unsigned outputs passed `rpm-info --verify`
and were readable by the system libarchive tool:

| Input | SHA-256 | Payload entries |
| --- | --- | --- |
| [which-2.21-30.el9_6.x86_64.rpm](https://repo.almalinux.org/almalinux/9/BaseOS/x86_64/os/Packages/which-2.21-30.el9_6.x86_64.rpm) | `c721b0392c9ccfa3b572a06bf96f6221cf29eda80f192ef0035b3f4e7bb216de` | 15 |
| [filesystem-3.16-5.el9.x86_64.rpm](https://repo.almalinux.org/almalinux/9/BaseOS/x86_64/os/Packages/filesystem-3.16-5.el9.x86_64.rpm) | `344ec08ec60a9644cd260ecdc66b292420f3e2732ecfae65d08d808e1df67206` | 1,545 |

The packages were downloaded into temporary directories for these checks and are
not redistributed in this repository. No package was installed or executed, and
its OpenPGP signature was not authenticated.
