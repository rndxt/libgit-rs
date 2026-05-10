use std::io::{self, Write};

use crate::binary::BinaryReader;
use crate::index;
use crate::object_db;
use crate::object_db::Object;
use crate::object_id::ObjectId;
use crate::object_type::ObjectType;
use crate::refs;
use crate::refs::HeadState;
use crate::repo::Repository;
use crate::signature::{AuthorInfo, CommitterInfo, Signature};
use crate::tree::create_trees_from_index;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub tree_id: ObjectId,
    pub parents: Vec<ObjectId>,
    pub author: Signature,
    pub committer: Signature,
    pub message: String,
}

impl Commit {
    pub fn new(
        tree_id: ObjectId,
        parents: Vec<ObjectId>,
        author: AuthorInfo,
        committer: CommitterInfo,
        message: String,
    ) -> Self {
        Self {
            tree_id,
            author: author.0,
            committer: committer.0,
            message,
            parents,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read Index: {0}")]
    CannotReadIndex(#[from] index::Error),

    #[error("cannot store object to odb: {0}")]
    OdbWriteFailed(#[from] object_db::Error),

    #[error("cannot read tree: {0}")]
    OdbReadFailed(object_db::Error),

    #[error("lookup ref failed: {0}")]
    LookupRefFailed(refs::Error),

    #[error("specified commitish is not commit, tag, branch or HEAD")]
    InvalidCommitish,
}

pub fn create_commit_from_index(
    repo: &Repository,
    author: AuthorInfo,
    committer: CommitterInfo,
    message: &str,
    parents: &[ObjectId],
) -> Result<ObjectId, Error> {
    let index = repo.read_index()?;
    let odb = repo.object_db();
    let tree_id = create_trees_from_index(&odb, &index)?;
    let mut buffer = Vec::new();
    let mut writer = CommitWriter::new(&mut buffer);
    writer
        .write_commit_ext(&tree_id, parents, &author.0, &committer.0, message)
        .unwrap();
    let commit_id = odb.write_raw(&buffer, ObjectType::Commit)?;
    Ok(commit_id)
}

struct CommitWriter<W: Write> {
    dest: W,
}

impl<W: Write> CommitWriter<W> {
    fn new(dest: W) -> Self {
        Self { dest }
    }

    #[allow(unused)]
    pub fn write_commit(&mut self, commit: &Commit) -> io::Result<()> {
        self.write_commit_ext(
            &commit.tree_id,
            &commit.parents,
            &commit.author,
            &commit.committer,
            &commit.message,
        )
    }

    fn write_commit_ext(
        &mut self,
        tree_id: &ObjectId,
        parents: &[ObjectId],
        author: &Signature,
        committer: &Signature,
        message: &str,
    ) -> io::Result<()> {
        writeln!(self.dest, "tree {}", tree_id)?;
        for parent in parents {
            writeln!(self.dest, "parent {}", parent)?;
        }

        writeln!(self.dest, "author {}", author)?;
        writeln!(self.dest, "committer {}", committer)?;
        writeln!(self.dest)?;
        writeln!(self.dest, "{}", message)?;
        Ok(())
    }
}

pub fn read_commit_from_buffer(buffer: &[u8]) -> Option<Commit> {
    let mut reader = BinaryReader::new(buffer);
    reader.skip_prefix(b"tree ")?;
    let tree_id = reader
        .split_until_inclusive(b'\n')
        .and_then(|bytes| str::from_utf8(bytes).ok())
        .and_then(|str| ObjectId::from_str(str).ok())?;

    let mut parents = Vec::new();
    while reader.skip_prefix(b"parent ").is_some() {
        let parent_id = reader
            .split_until_inclusive(b'\n')
            .and_then(|bytes| str::from_utf8(bytes).ok())
            .and_then(|str| ObjectId::from_str(str).ok())?;
        parents.push(parent_id);
    }

    reader.skip_prefix(b"author ")?;
    let author = reader
        .split_until_inclusive(b'\n')
        .and_then(Signature::try_from_bytes)?;
    reader.skip_prefix(b"committer ")?;
    let committer = reader
        .split_until_inclusive(b'\n')
        .and_then(Signature::try_from_bytes)?;

    reader.skip_byte(b'\n')?;
    let message = reader
        .split_until_inclusive(b'\n')
        .and_then(|bytes| str::from_utf8(bytes).ok())
        .map(str::to_string)?;

    let commit = Commit {
        tree_id,
        parents,
        author,
        committer,
        message,
    };
    Some(commit)
}

pub fn decay_to_commit(repo: &Repository, commitish: &str) -> Result<(Commit, ObjectId), Error> {
    if commitish == "HEAD" {
        let refs = repo.refs();
        let head = refs.resolve_head().map_err(Error::LookupRefFailed)?;
        let commit_id = match head {
            HeadState::Detached(id) => id,
            HeadState::Normal(branch) => branch.target,
        };

        let odb = repo.object_db();
        let commit = odb.read_commit(commit_id).map_err(Error::OdbReadFailed)?;
        return Ok((commit, commit_id));
    }

    if let Ok(id) = ObjectId::from_str(commitish) {
        let odb = repo.object_db();
        let object = odb.read_object(id).map_err(Error::OdbReadFailed)?;
        match object {
            Object::Commit(commit) => {
                return Ok((commit, id));
            },
            Object::Tag(tag) => {
                let commit = odb
                    .read_commit(tag.object_id)
                    .map_err(Error::OdbReadFailed)?;
                return Ok((commit, id));
            },
            _ => {
                return Err(Error::InvalidCommitish);
            },
        }
    }

    let refs = repo.refs();
    let branch = refs
        .lookup_branch(commitish)
        .map_err(Error::LookupRefFailed)?;
    let commit_id = branch.target;
    let odb = repo.object_db();
    let commit = odb.read_commit(commit_id).map_err(Error::OdbReadFailed)?;
    Ok((commit, commit_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    use crate::time::Time;

    #[test]
    fn write_commit() -> testing::Result<()> {
        let tree_id = ObjectId::from_str("85df50785d62d3b05ab03d9cbf7e4a0b49449730").unwrap();
        let parents = [
            ObjectId::from_str("15df50785d62d3b05ab03d9cbf7e4a0b49449730").unwrap(),
            ObjectId::from_str("25df50785d62d3b05ab03d9cbf7e4a0b49449730").unwrap(),
        ];
        let author =
            Signature::build("author", "author@email", Time::new(1771253662, 10800)).unwrap();
        let committer =
            Signature::build("committer", "commiter@email", Time::new(1771253662, 10800)).unwrap();
        let message = "Commit message";

        let mut buffer = Vec::new();
        let mut writer = CommitWriter::new(&mut buffer);
        writer.write_commit_ext(&tree_id, &parents, &author, &committer, &message)?;

        let expected_data = b"tree 85df50785d62d3b05ab03d9cbf7e4a0b49449730\nparent 15df50785d62d3b05ab03d9cbf7e4a0b49449730\nparent 25df50785d62d3b05ab03d9cbf7e4a0b49449730\nauthor author <author@email> 1771253662 +0300\ncommitter committer <commiter@email> 1771253662 +0300\n\nCommit message\n";
        assert_eq!(expected_data, &buffer[..]);
        Ok(())
    }

    #[test]
    fn read_commit() -> testing::Result<()> {
        let data = b"tree 85df50785d62d3b05ab03d9cbf7e4a0b49449730\nparent 15df50785d62d3b05ab03d9cbf7e4a0b49449730\nparent 25df50785d62d3b05ab03d9cbf7e4a0b49449730\nauthor author <author@email> 1771253662 +0300\ncommitter committer <commiter@email> 1771253662 +0300\n\nmessage\n";
        let commit = read_commit_from_buffer(data).unwrap();

        assert_eq!(
            commit.tree_id,
            ObjectId::from_str("85df50785d62d3b05ab03d9cbf7e4a0b49449730").unwrap()
        );

        assert_eq!(commit.parents.len(), 2);
        assert_eq!(
            commit.parents[0],
            ObjectId::from_str("15df50785d62d3b05ab03d9cbf7e4a0b49449730").unwrap()
        );
        assert_eq!(
            commit.parents[1],
            ObjectId::from_str("25df50785d62d3b05ab03d9cbf7e4a0b49449730").unwrap()
        );
        assert_eq!(
            commit.author,
            Signature {
                name: String::from("author"),
                email: String::from("author@email"),
                time: Time {
                    unix_time: 1771253662,
                    offset: 10800
                },
            }
        );
        assert_eq!(
            commit.committer,
            Signature {
                name: String::from("committer"),
                email: String::from("commiter@email"),
                time: Time {
                    unix_time: 1771253662,
                    offset: 10800
                },
            }
        );
        assert_eq!(commit.message, "message");
        Ok(())
    }
}
