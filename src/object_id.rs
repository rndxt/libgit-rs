use crate::sha1::{SHA1_SIZE_IN_BYTES, Sha1};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectId {
    hash: Sha1,
}

impl ObjectId {
    pub fn zero() -> ObjectId {
        Self { hash: Sha1::null() }
    }

    pub fn from_str(s: &str) -> Option<ObjectId> {
        Sha1::from_str(s).map(|hash| Self { hash })
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<ObjectId> {
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

    pub fn to_string(&self) -> String {
        self.hash.to_string()
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.hash.as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn from_str() -> testing::Result<()> {
        let s = "85df50785d62d3b05ab03d9cbf7e4a0b49449730";
        assert!(ObjectId::from_str(s).is_some());

        let s = "wrong format";
        assert!(ObjectId::from_str(s).is_none());
        Ok(())
    }

    #[test]
    fn from_bytes() -> testing::Result<()> {
        let bytes = &[0; 20];
        assert!(ObjectId::from_bytes(bytes).is_some());

        let short = &[1, 2, 3, 4];
        assert!(ObjectId::from_bytes(short).is_none());

        let long = &[1; 21];
        assert!(ObjectId::from_bytes(long).is_none());
        Ok(())
    }
}
