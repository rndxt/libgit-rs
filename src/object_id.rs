use std::fmt;

pub use crate::sha1::Error;
use crate::sha1::{SHA1_SIZE_IN_BYTES, Sha1};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectId {
    hash: Sha1,
}

impl ObjectId {
    pub fn zero() -> ObjectId {
        Self { hash: Sha1::null() }
    }

    pub fn from_str(s: &str) -> Result<ObjectId, Error> {
        Sha1::from_str(s).map(|hash| Self { hash })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<ObjectId, Error> {
        Sha1::from_bytes(bytes).map(|hash| Self { hash })
    }

    pub fn from_sha1(hash: Sha1) -> ObjectId {
        Self { hash }
    }

    pub fn from_array(bytes: &[u8; SHA1_SIZE_IN_BYTES]) -> ObjectId {
        Self {
            hash: Sha1::from_array(bytes),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.hash.as_bytes()
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn from_str() -> testing::Result<()> {
        let s = "85df50785d62d3b05ab03d9cbf7e4a0b49449730";
        assert!(ObjectId::from_str(s).is_ok());

        let s = "wrong format";
        assert!(ObjectId::from_str(s).is_err());
        Ok(())
    }

    #[test]
    fn from_bytes() -> testing::Result<()> {
        let bytes = &[0; 20];
        assert!(ObjectId::from_bytes(bytes).is_ok());

        let short = &[1, 2, 3, 4];
        assert!(ObjectId::from_bytes(short).is_err());

        let long = &[1; 21];
        assert!(ObjectId::from_bytes(long).is_err());
        Ok(())
    }

    #[test]
    fn from_bytes_and_from_str_equals() -> testing::Result<()> {
        let bytes =
            b"\x00\x68\x5f\x1c\x06\x8f\xc3\x32\x30\x76\x91\x78\xcf\xa3\xd3\xb7\x1f\x8d\x99\x4b";
        let str = "00685f1c068fc33230769178cfa3d3b71f8d994b";
        assert_eq!(
            ObjectId::from_bytes(bytes).unwrap(),
            ObjectId::from_str(str).unwrap()
        );
        Ok(())
    }
}
