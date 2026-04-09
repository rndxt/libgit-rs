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
    #[error("email should not be empty")]
    EmptyEmail,

    #[error("name should not be empty")]
    EmptyName,

    #[error("email should not contain '<', '>' chars")]
    EmailContainsAngle,

    #[error("name should not contain '<', '>' chars")]
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

fn contain_angle_brackets(s: &str) -> bool {
    s.contains('<') || s.contains('>')
}
