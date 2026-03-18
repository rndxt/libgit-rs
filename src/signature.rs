use crate::time::Time;

pub struct Signature {
    pub name: String,
    pub email: String,
    pub time: Time,
}

impl Signature {
    pub fn new(name: &str, email: &str, time: Time) -> Result<Signature, ()> {
        let sign = Signature {
            name: name.to_string(),
            email: email.to_string(),
            time,
        };
        Ok(sign)
    }
}

pub struct AuthorInfo(pub Signature);

impl AuthorInfo {
    pub fn new(name: &str, email: &str, time: Time) -> Result<AuthorInfo, ()> {
        let sign = Signature::new(name, email, time)?;
        Ok(AuthorInfo(sign))
    }
}

pub struct CommiterInfo(pub Signature);

impl CommiterInfo {
    pub fn new(name: &str, email: &str, time: Time) -> Result<CommiterInfo, ()> {
        let sign = Signature::new(name, email, time)?;
        Ok(CommiterInfo(sign))
    }
}
