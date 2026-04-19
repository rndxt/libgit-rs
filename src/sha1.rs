use std::{fmt, io::Write};

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

    pub fn from_str(s: &str) -> Result<Self, Error> {
        base16ct::lower::decode_vec(s.as_bytes())
            .map_err(|_| Error::InvalidData)
            .and_then(|v| Sha1::from_bytes(&v))
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
        let str = base16ct::lower::encode_string(&self.0);
        f.write_str(&str)
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
