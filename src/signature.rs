use std::fmt;

use crate::binary::BinaryReader;
use crate::time::Time;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub name: String,
    pub email: String,
    pub time: Time,
}

pub struct AuthorInfo(pub Signature);
pub struct CommitterInfo(pub Signature);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("email is empty")]
    EmptyEmail,

    #[error("name is empty")]
    EmptyName,

    #[error("email contains '<', '>' chars")]
    EmailContainsAngle,

    #[error("name contains '<', '>' chars")]
    NameContainsAngle,
}

impl Signature {
    pub fn build(name: &str, email: &str, time: Time) -> Result<Signature, Error> {
        if name.is_empty() {
            return Err(Error::EmptyName);
        }

        if contain_angle_brackets(name) {
            return Err(Error::NameContainsAngle);
        }

        if email.is_empty() {
            return Err(Error::EmptyEmail);
        }

        if contain_angle_brackets(email) {
            return Err(Error::EmailContainsAngle);
        }

        let sign = Signature {
            name: name.to_string(),
            email: email.to_string(),
            time,
        };
        Ok(sign)
    }

    pub fn try_from_bytes(bytes: &[u8]) -> Option<Signature> {
        let mut reader = BinaryReader::new(bytes);
        let name = reader
            .split_until_inclusive(b'<')
            .and_then(|bytes| str::from_utf8(bytes).ok())
            .map(str::trim_ascii_end)
            .map(str::to_string)?;

        let email = reader
            .split_until_inclusive(b'>')
            .and_then(|bytes| str::from_utf8(bytes).ok())
            .map(str::to_string)?;

        reader.skip_byte(b' ')?;
        let time = Time::try_from_bytes(reader.bytes())?;
        let sign = Signature { name, email, time };
        Some(sign)
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Signature { name, email, time } = self;
        write!(f, "{} <{}> {}", name, email, time)
    }
}

fn contain_angle_brackets(s: &str) -> bool {
    s.contains('<') || s.contains('>')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn try_from_bytes_on_valid() -> testing::Result<()> {
        let bytes = b"author <author@email> 1771253662 +0300";
        let signature = Signature::try_from_bytes(bytes).unwrap();
        assert_eq!(signature.name, "author");
        assert_eq!(signature.email, "author@email");
        assert_eq!(
            signature.time,
            Time {
                unix_time: 1771253662,
                offset: 10800
            }
        );
        Ok(())
    }
}
