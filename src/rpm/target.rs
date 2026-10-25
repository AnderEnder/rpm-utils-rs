//! Explicit Linux package targets, independent of the packaging host.
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
    str::FromStr,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageTarget {
    arch: String,
    elf: Option<(u16, u8, u8)>,
}
impl PackageTarget {
    pub fn arch(&self) -> &str {
        &self.arch
    }
    pub fn os(&self) -> &str {
        "linux"
    }
    /// Check recognized native formats without executing the input. Non-native
    /// files (including JARs, Java class files, scripts and data) are accepted.
    /// This checks ELF machine, class and byte order, not libc or CPU extensions.
    pub fn validate_file(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let mut file = File::open(path.as_ref())?;
        let mut header = [0_u8; 64];
        let mut count = 0;
        while count < header.len() {
            let n = file.read(&mut header[count..])?;
            if n == 0 {
                break;
            }
            count += n;
        }
        let bytes = &header[..count];
        let fail = |message: &str| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{}: {message} (package target {})",
                    path.as_ref().display(),
                    self.arch
                ),
            )
        };
        if bytes.starts_with(b"\x7fELF") {
            let Some((machine, class, endian)) = self.elf else {
                return Err(fail("Native ELF content is not allowed in noarch packages"));
            };
            if count < 20 {
                return Err(fail("Truncated ELF header"));
            }
            let actual = match header[5] {
                1 => u16::from_le_bytes([header[18], header[19]]),
                2 => u16::from_be_bytes([header[18], header[19]]),
                _ => return Err(fail("Invalid ELF byte order")),
            };
            if actual != machine || header[4] != class || header[5] != endian {
                return Err(fail(
                    "ELF machine, class or byte order does not match target",
                ));
            }
        } else if bytes.starts_with(b"MZ")
            || [
                [0xfe, 0xed, 0xfa, 0xce],
                [0xce, 0xfa, 0xed, 0xfe],
                [0xfe, 0xed, 0xfa, 0xcf],
                [0xcf, 0xfa, 0xed, 0xfe],
                [0xca, 0xfe, 0xba, 0xbf],
                [0xbf, 0xba, 0xfe, 0xca],
                [0xbe, 0xba, 0xfe, 0xca],
            ]
            .iter()
            .any(|magic| bytes.starts_with(magic))
        {
            return Err(fail(
                "macOS/Windows native content cannot target Linux/noarch",
            ));
        } else if bytes.starts_with(&[0xca, 0xfe, 0xba, 0xbe]) {
            // Java class magic overlaps big-endian Mach-O universal magic.
            // Universal files have a small architecture count; Java has a
            // major class version >=45 in the low half of the same word.
            if count < 8 || u32::from_be_bytes(header[4..8].try_into().unwrap()) < 45 {
                return Err(fail("Mach-O universal content cannot target Linux/noarch"));
            }
        }
        Ok(())
    }
}
impl FromStr for PackageTarget {
    type Err = io::Error;
    fn from_str(value: &str) -> io::Result<Self> {
        let arch = match value {
            "noarch" => "noarch",
            "x86_64" | "amd64" => "x86_64",
            "aarch64" | "arm64" => "aarch64",
            "i686" => "i686",
            "riscv64" | "ppc64le" | "s390x" => value,
            _ => {
                // Accept only explicitly supported Linux ABI triples.
                let (arch, rest) = value.split_once('-').ok_or_else(|| invalid(value))?;
                if !matches!(
                    rest,
                    "unknown-linux-gnu" | "unknown-linux-musl" | "linux-gnu" | "linux-musl"
                ) {
                    return Err(invalid(value));
                }
                match arch {
                    "x86_64" | "aarch64" | "i686" | "riscv64" | "s390x" => arch,
                    "powerpc64le" => "ppc64le",
                    "riscv64gc" => "riscv64",
                    _ => return Err(invalid(value)),
                }
            }
        };
        let elf = match arch {
            "noarch" => None,
            "x86_64" => Some((62, 2, 1)),
            "aarch64" => Some((183, 2, 1)),
            "i686" => Some((3, 1, 1)),
            "riscv64" => Some((243, 2, 1)),
            "ppc64le" => Some((21, 2, 1)),
            "s390x" => Some((22, 2, 2)),
            _ => unreachable!(),
        };
        Ok(Self {
            arch: arch.into(),
            elf,
        })
    }
}
fn invalid(value: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "Unsupported package target {value:?}; use noarch, x86_64, aarch64, i686, riscv64, ppc64le, s390x or a supported Linux target triple"
        ),
    )
}
