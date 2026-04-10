use std::io::{self, Write};

use crate::FileMode;
use crate::index::Index;
use crate::object_db::hash_buffer;
use crate::object_db::{self, ObjectDB};
use crate::object_id::ObjectId;
use crate::object_type::ObjectType;

pub struct TreeEntry {
    pub mode: u32,
    pub filename: Vec<u8>,
    pub id: ObjectId,
}

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

pub fn create_trees_from_index(odb: &ObjectDB, index: &Index) -> Result<ObjectId, object_db::Error> {
    // TODO: check conflicts
    let writer = FromIndex::new(index, WriteCallback::WriteToDb(odb));
    writer.write_tree()
}

enum WriteCallback<'a> {
    WriteToDb(&'a ObjectDB),
    Dryrun,
}

struct FromIndex<'a> {
    index: &'a Index,
    callback: WriteCallback<'a>,
}

impl<'a> FromIndex<'a> {
    fn new(index: &'a Index, callback: WriteCallback<'a>) -> FromIndex<'a> {
        Self { index, callback }
    }

    fn write_tree(&self) -> Result<ObjectId, object_db::Error> {
        let (_, id) = self.write_tree_impl(&[], 0)?;
        Ok(id)
    }

    fn write_tree_impl(
        &self,
        dir: &[u8],
        start: usize,
    ) -> Result<(usize, ObjectId), object_db::Error> {
        let mut i = start;
        let mut writer = TreeWriter::new(Vec::new());

        while i < self.index.count_entries() {
            let entry = self.index.get_unchecked(i);
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
                let (next, id) = self.write_tree_impl(path, i)?;
                writer.write_entry(FileMode::Tree.into(), mid, &id).unwrap();
                i = next;
            } else {
                // File or gitlink
                writer.write_entry(entry.mode, rest, &entry.id).unwrap();
                i += 1;
            }
        }

        let buffer = writer.done();
        let id = match self.callback {
            WriteCallback::Dryrun => hash_buffer(&buffer, ObjectType::Tree),
            WriteCallback::WriteToDb(odb) => odb.write_raw(&buffer, ObjectType::Tree)?,
        };
        Ok((i, id))
    }
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

    fn done(self) -> W {
        self.dest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{Index, IndexEntry, IndexTime};
    use crate::testing;

    #[test]
    fn index_to_tree() -> testing::Result<()> {
        let index = get_test_index();
        let expected_tree_id = "43a32e4561668fff56c5f453776061ae20b90fcc";

        let writer = FromIndex::new(&index, WriteCallback::Dryrun);
        let id = writer.write_tree()?;
        assert_eq!(expected_tree_id, id.to_string());
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
