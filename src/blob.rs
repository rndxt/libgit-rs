#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Blob {
    pub data: Vec<u8>,
}

impl Blob {
    pub fn new(data: Vec<u8>) -> Blob {
        Blob { data }
    }
}
