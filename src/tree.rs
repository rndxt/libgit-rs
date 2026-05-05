use std::collections::LinkedList;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::FileMode;
use crate::binary::BinaryReader;
use crate::index::Index;
use crate::object_db::{self, Object, ObjectDB};
use crate::object_id::ObjectId;
use crate::object_type::ObjectType;
use crate::refs::{self, HeadState};
use crate::repo::Repository;
use crate::sha1::SHA1_SIZE_IN_BYTES;

#[derive(Debug, PartialEq)]
pub struct TreeEntry {
    pub mode: u32,
    pub filename: Vec<u8>,
    pub id: ObjectId,
}

#[derive(Debug, PartialEq)]
pub struct Tree {
    pub entries: Vec<TreeEntry>,
}

impl Tree {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

pub fn create_trees_from_index(
    odb: &ObjectDB,
    index: &Index,
) -> Result<ObjectId, object_db::Error> {
    // TODO: check conflicts
    let (_, id) = create_trees_from_index_impl(
        index,
        &mut |buffer| odb.write_raw(&buffer, ObjectType::Tree),
        &[],
        0,
    )?;
    Ok(id)
}

fn create_trees_from_index_impl<F, E>(
    index: &Index,
    callback: &mut F,
    dir: &[u8],
    start: usize,
) -> Result<(usize, ObjectId), E>
where
    F: FnMut(&[u8]) -> Result<ObjectId, E>,
{
    let mut i = start;
    let mut buffer = Vec::new();
    let mut writer = TreeWriter::new(&mut buffer);

    while i < index.count_entries() {
        let entry = index.get_unchecked(i);
        let path = &entry.path[..];

        if path.len() <= dir.len() {
            break;
        }

        let (left, rest) = path.split_at(dir.len());
        if left != dir {
            break;
        }

        if let Some(slash) = rest.iter().position(|b| *b == b'/') {
            let (mid, _) = rest.split_at(slash);
            let (path, _) = path.split_at(left.len() + mid.len() + 1);
            let (next, id) = create_trees_from_index_impl(index, callback, path, i)?;
            writer.write_entry(FileMode::Tree.into(), mid, &id).unwrap();
            i = next;
        } else {
            // File or gitlink
            writer.write_entry(entry.mode, rest, &entry.id).unwrap();
            i += 1;
        }
    }

    let id = callback(&buffer)?;
    Ok((i, id))
}

pub fn read_tree_from_buffer(buffer: &[u8]) -> Option<Tree> {
    let mut reader = BinaryReader::new(buffer);
    let mut tree = Tree::new();
    loop {
        if reader.is_empty() {
            break;
        }

        let mode = reader
            .split_until_inclusive(b' ')
            .and_then(|bytes| str::from_utf8(bytes).ok())
            .and_then(|str| u32::from_str_radix(str, 8).ok())?;

        let filename = reader.split_until_inclusive(b'\0')?.to_vec();

        let id = reader
            .split_n(SHA1_SIZE_IN_BYTES)
            .and_then(|bytes| ObjectId::from_bytes(bytes).ok())?;

        tree.entries.push(TreeEntry { mode, filename, id });
    }
    Some(tree)
}

struct TreeWriter<W: Write> {
    dest: W,
}

impl<W: Write> TreeWriter<W> {
    fn new(dest: W) -> Self {
        Self { dest }
    }

    fn write_tree(&mut self, tree: &Tree) -> io::Result<()> {
        for entry in &tree.entries {
            self.write_entry(entry.mode, &entry.filename, &entry.id)?;
        }
        Ok(())
    }

    fn write_entry(&mut self, mode: u32, filename: &[u8], id: &ObjectId) -> io::Result<()> {
        write!(self.dest, "{:o}", mode)?;
        self.dest.write_all(b" ")?;
        self.dest.write_all(filename)?;
        self.dest.write_all(b"\0")?;
        self.dest.write_all(id.as_bytes())?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read tree: {0}")]
    OdbReadFailed(object_db::Error),

    #[error("lookup ref failed: {0}")]
    LookupRefFailed(refs::Error),

    #[error("specified treeish is not commit, tag, tree, branch or HEAD")]
    InvalidTreeish,

    #[error("Empty directories hierarchy")]
    EmptyDirs,
}

struct StackNode {
    entries: Vec<TreeEntry>,
    idx: usize,
}

pub struct TreeWalker<'a> {
    repo: &'a Repository,
    path: PathBuf,
    stack: LinkedList<StackNode>,
}

pub struct WalkEntry<'a> {
    tree_entry: &'a TreeEntry,
    dir_path: &'a [u8],
}

impl<'a> WalkEntry<'a> {
    pub fn new(tree_entry: &'a TreeEntry, dir_path: &'a [u8]) -> WalkEntry<'a> {
        WalkEntry {
            tree_entry,
            dir_path,
        }
    }

    pub fn make_fullpath(&self) -> Vec<u8> {
        let mut path = Vec::new();
        if !self.dir_path.is_empty() {
            path.extend_from_slice(self.dir_path);
            path.push(b'/');
        }
        path.extend_from_slice(&self.tree_entry.filename);
        path
    }

    pub fn dir_path(&self) -> &[u8] {
        self.dir_path
    }

    pub fn object_id(&self) -> ObjectId {
        self.tree_entry.id
    }
}

impl<'a> TreeWalker<'a> {
    fn advance_to_next_non_tree(&mut self) -> Result<(), Error> {
        loop {
            let top = self.stack.back().unwrap();
            let entry = &top.entries[top.idx];
            if entry.mode != FileMode::Tree.into() {
                return Ok(());
            }

            // TODO: check cycles: A -> B, B -> A
            let tree = self
                .repo
                .object_db()
                .read_tree(entry.id)
                .map_err(Error::OdbReadFailed)?;

            if tree.entries.is_empty() {
                return Err(Error::EmptyDirs);
            }

            self.path.push(str::from_utf8(&entry.filename).unwrap());
            self.stack.push_back(StackNode {
                entries: tree.entries,
                idx: 0,
            });
        }
    }
}

impl<'a> TreeWalker<'a> {
    pub fn from_id(id: ObjectId, repo: &'a Repository) -> Result<Self, Error> {
        let tree = repo
            .object_db()
            .read_tree(id)
            .map_err(Error::OdbReadFailed)?;
        Self::from_tree(repo, tree)
    }

    pub fn from_tree(repo: &'a Repository, tree: Tree) -> Result<Self, Error> {
        let mut walker = Self {
            repo,
            path: PathBuf::new(),
            stack: LinkedList::new(),
        };

        if !tree.entries.is_empty() {
            walker.stack.push_back(StackNode {
                entries: tree.entries,
                idx: 0,
            });
        }
        walker.advance_to_next_non_tree()?;
        Ok(walker)
    }

    pub fn current(&self) -> Option<WalkEntry<'_>> {
        let top = self.stack.back()?;
        debug_assert!(
            top.idx < top.entries.len(),
            "{} {}",
            top.idx,
            top.entries.len()
        );
        let entry = &top.entries[top.idx];
        debug_assert_ne!(entry.mode, FileMode::Tree.into());
        let path = &self.path.as_os_str().as_encoded_bytes();
        Some(WalkEntry::new(entry, path))
    }

    pub fn advance(&mut self) -> Result<(), Error> {
        while let Some(top) = self.stack.back_mut() {
            top.idx += 1;
            if top.idx != top.entries.len() {
                break;
            }

            self.stack.pop_back();
            self.path.pop();
        }

        if self.stack.is_empty() {
            return Ok(());
        }

        self.advance_to_next_non_tree()
    }
}

pub fn decay_to_tree(repo: &Repository, treeish: &str) -> Result<(Tree, ObjectId), Error> {
    if treeish == "HEAD" {
        let refs = repo.refs();
        let head = refs.resolve_head().map_err(Error::LookupRefFailed)?;
        let commit_id = match head {
            HeadState::Detached(id) => id,
            HeadState::Normal(branch) => branch.target,
        };

        let odb = repo.object_db();
        let commit = odb.read_commit(commit_id).map_err(Error::OdbReadFailed)?;
        let tree = odb
            .read_tree(commit.tree_id)
            .map_err(Error::OdbReadFailed)?;
        return Ok((tree, commit.tree_id));
    }

    if let Ok(id) = ObjectId::from_str(treeish) {
        let odb = repo.object_db();
        let object = odb.read_object(id).map_err(Error::OdbReadFailed)?;
        match object {
            Object::Tree(tree) => {
                return Ok((tree, id));
            },
            Object::Commit(commit) => {
                let tree = odb
                    .read_tree(commit.tree_id)
                    .map_err(Error::OdbReadFailed)?;
                return Ok((tree, commit.tree_id));
            },
            Object::Tag(tag) => {
                let commit = odb
                    .read_commit(tag.object_id)
                    .map_err(Error::OdbReadFailed)?;
                let tree = odb
                    .read_tree(commit.tree_id)
                    .map_err(Error::OdbReadFailed)?;
                return Ok((tree, commit.tree_id));
            },
            Object::Blob(_) => {
                return Err(Error::InvalidTreeish);
            },
        }
    }

    let refs = repo.refs();
    let branch = refs.lookup_branch(treeish).map_err(Error::LookupRefFailed)?;
    let commit_id = branch.target;
    let odb = repo.object_db();
    let commit = odb.read_commit(commit_id).map_err(Error::OdbReadFailed)?;
    let tree = odb
        .read_tree(commit.tree_id)
        .map_err(Error::OdbReadFailed)?;
    return Ok((tree, commit.tree_id));
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::*;
    use crate::index::{Index, IndexEntry, IndexTime};
    use crate::object_db::hash_buffer;
    use crate::testing;

    #[test]
    fn write_read_identity() -> testing::Result<()> {
        let mut tree = Tree::new();
        tree.entries.push(TreeEntry {
            mode: 0o40000,
            filename: b"dir".to_vec(),
            id: ObjectId::from_bytes(
                b"\x00\x68\x5f\x1c\x06\x8f\xc3\x32\x30\x76\x91\x78\xcf\xa3\xd3\xb7\x1f\x8d\x99\x4b",
            )
            .unwrap(),
        });

        let mut buffer = Vec::new();
        let mut writer = TreeWriter::new(&mut buffer);
        let _ = writer.write_tree(&tree)?;

        let parsed_tree = read_tree_from_buffer(&buffer).ok_or("failed")?;
        assert_eq!(parsed_tree, tree);
        Ok(())
    }

    #[test]
    fn read_returns_error_on_invalid_data() -> testing::Result<()> {
        let data = b"40000 dir\0abcdef";
        assert!(read_tree_from_buffer(data).is_none());

        let data = b"40000 dir\0\x00\x68\x5f\x1c\x06\x8f\xc3\x32\x30\x76\x91\x78\xcf\xa3\xd3\xb7\x1f\x8d\x99\x4b100644";
        assert!(read_tree_from_buffer(data).is_none());
        Ok(())
    }

    #[test]
    fn index_to_tree() -> testing::Result<()> {
        let index = get_test_index();

        let (_, id) = create_trees_from_index_impl(
            &index,
            &mut |buffer| -> Result<ObjectId, Infallible> {
                Ok(hash_buffer(&buffer, ObjectType::Tree))
            },
            &[],
            0,
        )?;

        assert_eq!(id.to_string(), "43a32e4561668fff56c5f453776061ae20b90fcc");
        Ok(())
    }

    fn get_test_index() -> Index {
        let mut index = Index::new();

        index.add(IndexEntry {
            mtime: IndexTime {
                seconds: 1770663391,
                nanoseconds: 768895303,
            },
            ctime: IndexTime {
                seconds: 1770663391,
                nanoseconds: 768895303,
            },
            dev: 2096,
            ino: 125352,
            mode: 0o100644,
            uid: 1000,
            gid: 1000,
            size: 8,
            id: ObjectId::from_str("b800cbcdba1d5ad659881e78c1b3a2ba09b48f71").unwrap(),
            flags: 11,
            path: b"dir/abc.txt".to_vec(),
        });

        index.add(IndexEntry {
            mtime: IndexTime {
                seconds: 1770889632,
                nanoseconds: 110920527,
            },
            ctime: IndexTime {
                seconds: 1770897876,
                nanoseconds: 896935512,
            },
            dev: 2096,
            ino: 106009,
            mode: 0o100644,
            uid: 1000,
            gid: 1000,
            size: 6,
            id: ObjectId::from_str("e56e15bb7ddb6bd0b6d924b18fcee53d8713d7ea").unwrap(),
            flags: 9,
            path: b"dir/file3".to_vec(),
        });

        index.add(IndexEntry {
            mtime: IndexTime {
                seconds: 1770897939,
                nanoseconds: 468940304,
            },
            ctime: IndexTime {
                seconds: 1770897939,
                nanoseconds: 468940304,
            },
            dev: 2096,
            ino: 125575,
            mode: 0o100644,
            uid: 1000,
            gid: 1000,
            size: 8,
            id: ObjectId::from_str("33a9488b167e4391ad6297a1e43e56f7ec8a294e").unwrap(),
            flags: 21,
            path: b"dir/subdir/example.md".to_vec(),
        });

        index.add(IndexEntry {
            mtime: IndexTime {
                seconds: 1770570963,
                nanoseconds: 108954535,
            },
            ctime: IndexTime {
                seconds: 1770897829,
                nanoseconds: 656954774,
            },
            dev: 2096,
            ino: 107557,
            mode: 0o100644,
            uid: 1000,
            gid: 1000,
            size: 4,
            id: ObjectId::from_str("85df50785d62d3b05ab03d9cbf7e4a0b49449730").unwrap(),
            flags: 9,
            path: b"file1.txt".to_vec(),
        });

        index.add(IndexEntry {
            mtime: IndexTime {
                seconds: 1770657479,
                nanoseconds: 758114976,
            },
            ctime: IndexTime {
                seconds: 1770897842,
                nanoseconds: 820929787,
            },
            dev: 2096,
            ino: 125244,
            mode: 0o100644,
            uid: 1000,
            gid: 1000,
            size: 5,
            id: ObjectId::from_str("81c545efebe5f57d4cab2ba9ec294c4b0cadf672").unwrap(),
            flags: 5,
            path: b"file2".to_vec(),
        });

        index
    }
}
