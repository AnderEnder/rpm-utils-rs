use std::io;
use std::io::{Read, Write};
use hex::{FromHex, ToHex};

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
    fn write_u32_as_hex(&mut self, from: u32) -> Result<(), io::Error>;
}

impl<W> HexWriter for W
where
    W: Write,
{
    fn write_u32_as_hex(&mut self, from: u32) -> Result<(), io::Error> {
        // self.write_all(format!("{:x}", from).as_bytes())?;
        self.write_all(&from.to_string().encode_hex::<String>().as_bytes())?;
        Ok(())
    }
}

pub trait HexReader {
    fn read_hex_as_u32(&mut self) -> Result<u32, io::Error>;
}

impl<R> HexReader for R
where
    R: Read,
{
    fn read_hex_as_u32(&mut self) -> Result<u32, io::Error> {
        let mut raw_bytes = [0_u8; 8];
        self.read_exact(&mut raw_bytes)?;

        let v = Vec::from_hex(raw_bytes).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Error: can not parse hex {}", e),
            )
        })?;

        let bytes = [v[0], v[1], v[2], v[3]];
        Ok(u32::from_be_bytes(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_allign_n() {
        assert_eq!(align_n_bytes(32, 8), 0);
        assert_eq!(align_n_bytes(33, 8), 7);
        assert_eq!(align_n_bytes(34, 8), 6);
        assert_eq!(align_n_bytes(35, 8), 5);
        assert_eq!(align_n_bytes(39, 8), 1);
    }
}
