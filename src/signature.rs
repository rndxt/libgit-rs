use std::fmt;

use crate::time::Time;

#[derive(Debug, Clone)]
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
