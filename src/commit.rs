use std::io::{self, Write};

use crate::index::OpenIndexError;
use crate::object_db;
use crate::object_id::ObjectId;
use crate::object_type::ObjectType;
use crate::repo::Repository;
use crate::signature::{AuthorInfo, CommitterInfo, Signature};
use crate::tree::create_trees_from_index;

pub struct Commit {
    pub tree_id: ObjectId,
    pub author: Signature,
    pub committer: Signature,
    pub message: String,
    pub parents: Vec<ObjectId>,
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
            parents: parents,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateCommitError {
    #[error(transparent)]
    ReadIndexFailed(#[from] OpenIndexError),

    #[error(transparent)]
    WriteTreeFailed(#[from] object_db::Error),
}

pub fn create_commit_from_index(
    repo: &Repository,
    author: AuthorInfo,
    committer: CommitterInfo,
    message: &str,
    parents: &[ObjectId],
) -> Result<ObjectId, CreateCommitError> {
    let index = repo.read_index()?;
    let odb = repo.object_db();
    let tree_id = create_trees_from_index(&odb, &index)?;
    let mut writer = CommitWriter::new(Vec::new());
    writer
        .write_commit_ext(&tree_id, parents, &author.0, &committer.0, message)
        .unwrap();
    let buffer = writer.done();
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

    fn write_commit(&mut self, commit: &Commit) -> io::Result<()> {
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
        writeln!(self.dest, "tree {}", tree_id.to_string())?;
        for parent in parents {
            writeln!(self.dest, "parent {}", parent.to_string())?;
        }

        self.write_signature("author", author)?;
        self.write_signature("committer", committer)?;
        writeln!(&mut self.dest)?;
        writeln!(&mut self.dest, "{}", message)?;
        Ok(())
    }

    fn write_signature(&mut self, role: &str, signature: &Signature) -> io::Result<()> {
        let Signature { name, email, time } = signature;

        let (sign, offset) = if time.offset < 0 {
            ('-', -time.offset)
        } else {
            ('+', time.offset)
        };

        let hours = offset / 3600;
        let minutes = offset % 3600;
        writeln!(
            self.dest,
            "{} {} <{}> {} {}{:02}{:02}",
            role, name, email, time.unix_time, sign, hours, minutes
        )
    }

    fn done(self) -> W {
        self.dest
    }
}
