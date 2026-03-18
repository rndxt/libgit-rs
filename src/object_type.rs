use std::fmt;

#[derive(Copy, Clone, Debug)]
pub enum ObjectType {
    Blob,
    Tree,
    Commit,
    Tag,
}

impl ObjectType {
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            ObjectType::Blob => b"blob",
            ObjectType::Tree => b"tree",
            ObjectType::Commit => b"commit",
            ObjectType::Tag => b"tag",
        }
    }
}

impl fmt::Display for ObjectType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ObjectType::Blob => "blob",
            ObjectType::Tree => "tree",
            ObjectType::Commit => "commit",
            ObjectType::Tag => "tag",
        };
        f.write_str(s)
    }
}
