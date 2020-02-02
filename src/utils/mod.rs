use hex::FromHex;
use omnom::prelude::*;
use std::io;
use std::io::{Read, Write};

pub fn align_n_bytes(from: u32, n: u32) -> u32 {
    (n - from % n) % n
}

pub fn parse_string(bytes: &[u8]) -> String {
    let position = bytes.iter().position(|&x| x == 0).unwrap_or(0);
    let bytes2 = &bytes[0..position];
    String::from_utf8_lossy(bytes2).to_string()
}

pub fn parse_strings(bytes: &[u8], count: usize) -> Vec<String> {
    bytes
        .split(|x| *x == 0)
        .take(count)
        .map(|b| String::from_utf8_lossy(b).to_string())
        .collect()
}

pub trait HexWriter {
    fn write_u32_as_hex(&mut self, from: u32) -> io::Result<()>;
}

impl<W> HexWriter for W
where
    W: Write,
{
    fn write_u32_as_hex(&mut self, from: u32) -> io::Result<()> {
        self.write_all(format!("{:08x}", from).as_bytes())?;
        Ok(())
    }
}

pub trait HexReader {
    fn read_hex_as_u32(&mut self) -> io::Result<u32>;
}

impl<R> HexReader for R
where
    R: Read,
{
    fn read_hex_as_u32(&mut self) -> io::Result<u32> {
        let mut raw_bytes = [0_u8; 8];
        self.read_exact(&mut raw_bytes)?;

        Vec::from_hex(raw_bytes)
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Error: can not parse hex {}", e),
                )
            })?
            .as_slice()
            .read_be()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::u32;
    #[test]
    fn test_allign_n() {
        assert_eq!(align_n_bytes(32, 8), 0);
        assert_eq!(align_n_bytes(33, 8), 7);
        assert_eq!(align_n_bytes(34, 8), 6);
        assert_eq!(align_n_bytes(35, 8), 5);
        assert_eq!(align_n_bytes(39, 8), 1);
    }

    #[test]
    #[allow(clippy::string_lit_as_bytes)]
    fn test_hex_reader() {
        assert_eq!("00000001".as_bytes().read_hex_as_u32().unwrap(), 1);
        assert_eq!("00000101".as_bytes().read_hex_as_u32().unwrap(), 257);
        assert_eq!("000001f1".as_bytes().read_hex_as_u32().unwrap(), 497);
        assert_eq!("ffffffff".as_bytes().read_hex_as_u32().unwrap(), u32::MAX);
    }

    #[test]
    fn test_hex_writer() {
        let mut buf = Vec::new();
        buf.write_u32_as_hex(1).unwrap();
        assert_eq!(buf.as_slice(), b"00000001");

        let mut buf = Vec::new();
        buf.write_u32_as_hex(257).unwrap();
        assert_eq!(buf.as_slice(), b"00000101");

        let mut buf = Vec::new();
        buf.write_u32_as_hex(497).unwrap();
        assert_eq!(buf.as_slice(), b"000001f1");

        let mut buf = Vec::new();
        buf.write_u32_as_hex(std::u32::MAX).unwrap();
        assert_eq!(buf.as_slice(), b"ffffffff");
    }
}
