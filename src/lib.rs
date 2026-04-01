pub use crate::index::add_path_to_index;
pub use crate::index::remove_path_from_index;
pub use crate::index::write_index;
pub use crate::index::{Index, IndexEntry, IndexTime};

pub use crate::object_db::hash_buffer;
pub use crate::object_db::hash_file;

pub use crate::object_id::ObjectId;
pub use crate::object_type::ObjectType;

pub use crate::repo::Repository;
pub use crate::repo::RepositoryInitOptions;

pub use crate::tree::write_index_to_tree;

pub mod commit;
mod index;
mod object_db;
pub mod object_id;
mod object_type;
mod repo;
mod sha1;
pub mod signature;
pub mod time;
pub mod tree;

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
