pub mod commit;
pub mod index;
pub mod object_db;
pub mod object_id;
pub mod object_type;
pub mod refs;
pub mod repo;
pub mod signature;
pub mod time;
pub mod tree;

mod sha1;

pub const GIT_MODE_BLOB: u32 = 0o100644;
pub const GIT_MODE_TREE: u32 = 0o040000;
pub const GIT_MODE_BLOB_EXECUTABLE: u32 = 0o100755;
pub const GIT_MODE_LINK: u32 = 0o120000;
pub const GIT_MODE_COMMIT: u32 = 0o160000;

#[cfg(test)]
mod testing {
    pub type Error = Box<dyn std::error::Error>;
    pub type Result<T> = std::result::Result<T, Error>;
}
