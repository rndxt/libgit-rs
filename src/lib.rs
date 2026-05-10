pub mod blob;
pub mod checkout;
pub mod commit;
pub mod diff;
pub mod diff3;
pub mod index;
pub mod merge_base;
pub mod myers;
pub mod object_db;
pub mod object_id;
pub mod object_type;
pub mod refs;
pub mod repo;
pub mod signature;
pub mod tag;
pub mod time;
pub mod tree;

mod binary;
mod sha1;

#[derive(Debug, Clone, Copy)]
pub enum FileMode {
    Blob,
    Executable,
    Tree,
    Link,
    Commit,
}

impl From<FileMode> for u32 {
    fn from(mode: FileMode) -> u32 {
        match mode {
            FileMode::Blob => 0o100644,
            FileMode::Executable => 0o100755,
            FileMode::Tree => 0o040000,
            FileMode::Link => 0o120000,
            FileMode::Commit => 0o160000,
        }
    }
}

#[cfg(test)]
mod testing {
    pub type Error = Box<dyn std::error::Error>;
    pub type Result<T> = std::result::Result<T, Error>;
}
