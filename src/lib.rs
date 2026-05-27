pub mod blob;
pub mod checkout;
pub mod commit;
pub mod diff;
pub mod diff3;
pub mod index;
pub mod merge;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMode {
    Blob,
    Executable,
    Tree,
    Link,
    Commit,
}

impl From<u32> for FileMode {
    fn from(mode: u32) -> FileMode {
        match mode {
            0o100644 => FileMode::Blob,
            0o040000 => FileMode::Tree,
            _ => unreachable!(),
        }
    }
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

impl FileMode {
    fn is_tree(&self) -> bool {
        match self {
            FileMode::Tree => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod testing {
    use std::collections::HashMap;

    use crate::blob::Blob;
    use crate::commit::Commit;
    use crate::object_db::{self, IOdb, Object};
    use crate::object_id::ObjectId;
    use crate::tag::Tag;
    use crate::tree::Tree;

    pub type Error = Box<dyn std::error::Error>;
    pub type Result<T> = std::result::Result<T, Error>;

    impl IOdb for HashMap<ObjectId, Object> {
        fn read_blob(&self, id: ObjectId) -> std::result::Result<Blob, object_db::Error> {
            self.get(&id)
                .ok_or(object_db::Error::NotFound)?
                .clone()
                .get_blob()
        }

        fn read_tree(&self, id: ObjectId) -> std::result::Result<Tree, object_db::Error> {
            self.get(&id)
                .ok_or(object_db::Error::NotFound)?
                .clone()
                .get_tree()
        }

        fn read_commit(&self, id: ObjectId) -> std::result::Result<Commit, object_db::Error> {
            self.get(&id)
                .ok_or(object_db::Error::NotFound)?
                .clone()
                .get_commit()
        }

        fn read_tag(&self, id: ObjectId) -> std::result::Result<Tag, object_db::Error> {
            self.get(&id)
                .ok_or(object_db::Error::NotFound)?
                .clone()
                .get_tag()
        }

        fn write_blob(&mut self, _blob: &Blob) -> std::result::Result<ObjectId, object_db::Error> {
            unimplemented!()
        }

        fn write_tree(&mut self, _tree: &Tree) -> std::result::Result<ObjectId, object_db::Error> {
            unimplemented!()
        }

        fn write_commit(
            &mut self,
            _commit: &Commit,
        ) -> std::result::Result<ObjectId, object_db::Error> {
            unimplemented!()
        }

        fn write_tag(&mut self, _tag: &Tag) -> std::result::Result<ObjectId, object_db::Error> {
            unimplemented!()
        }
    }
}
