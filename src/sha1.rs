use std::fmt;
use std::io::Write;

use sha1::{Digest, Sha1 as Sha1Internal};

pub const SHA1_SIZE_IN_BYTES: usize = 20;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid SHA-1")]
    InvalidData,

    #[error("wrong buffer size: {0}")]
    WrongBufferSize(usize),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Sha1(pub [u8; SHA1_SIZE_IN_BYTES]);

impl Sha1 {
    pub fn null() -> Self {
        Self([0; SHA1_SIZE_IN_BYTES])
    }

    pub fn from_array(bytes: &[u8; SHA1_SIZE_IN_BYTES]) -> Self {
        Self(*bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() != SHA1_SIZE_IN_BYTES {
            return Err(Error::WrongBufferSize(bytes.len()));
        }

        let mut hash = Self::null();
        hash.0.copy_from_slice(bytes);
        Ok(hash)
    }

    pub fn from_ascii_hex(hex: &[u8]) -> Result<Self, Error> {
        if let (chunks, []) = hex.as_chunks::<2>() {
            let mut bytes = Vec::new();
            for chunk in chunks {
                let str = str::from_utf8(chunk).map_err(|_| Error::InvalidData)?;
                let byte = u8::from_str_radix(str, 16).map_err(|_| Error::InvalidData)?;
                bytes.push(byte);
            }
            Self::from_bytes(&bytes)
        } else {
            Err(Error::WrongBufferSize(hex.len()))
        }
    }

    pub fn from_str(s: &str) -> Result<Self, Error> {
        Self::from_ascii_hex(s.as_bytes())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl fmt::Display for Sha1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            let byte = byte as u32;
            write!(
                f,
                "{}{}",
                char::from_digit(byte / 16, 16).unwrap(),
                char::from_digit(byte % 16, 16).unwrap()
            )?;
        }
        Ok(())
    }
}

pub struct Sha1Hasher {
    hasher: Sha1Internal,
}

impl Sha1Hasher {
    pub fn new() -> Self {
        Self {
            hasher: Sha1Internal::new(),
        }
    }

    pub fn update(&mut self, data: impl AsRef<[u8]>) {
        self.hasher.update(data);
    }

    pub fn finalize(self) -> Sha1 {
        Sha1::from_bytes(&self.hasher.finalize()).unwrap()
    }

    pub fn finalize_reset(&mut self) -> Sha1 {
        Sha1::from_bytes(&self.hasher.finalize_reset()).unwrap()
    }
}

impl Write for Sha1Hasher {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.hasher.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.hasher.flush()
    }
}
