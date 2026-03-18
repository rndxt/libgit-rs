use sha1::{Digest, Sha1 as Sha1Internal};
use std::io::Write;

pub const SHA1_SIZE_IN_BYTES: usize = 20;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Sha1(pub [u8; SHA1_SIZE_IN_BYTES]);

impl Sha1 {
    pub fn null() -> Self {
        Self([0; SHA1_SIZE_IN_BYTES])
    }

    pub fn from_array(bytes: &[u8; SHA1_SIZE_IN_BYTES]) -> Self {
        Self(bytes.clone())
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != SHA1_SIZE_IN_BYTES {
            return None;
        }

        let mut hash = Self::null();
        hash.0.copy_from_slice(bytes);
        Some(hash)
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match base16ct::lower::decode_vec(s.as_bytes()) {
            Ok(vec) => Sha1::from_bytes(&vec),
            _ => None,
        }
    }

    pub fn to_string(&self) -> String {
        base16ct::lower::encode_string(&self.0)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
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
