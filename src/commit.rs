use crate::index::OpenIndexError;
use crate::object_db;
use crate::{ObjectType, write_index_to_tree};

use crate::ObjectId;
use crate::Repository;
use crate::signature::{AuthorInfo, CommiterInfo, Signature};

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
    commiter: CommiterInfo,
    message: &str,
    parents: &[ObjectId],
) -> Result<ObjectId, CreateCommitError> {
    let index = repo.read_index()?;
    let odb = repo.object_db();
    let tree_id = write_index_to_tree(&odb, &index)?;
    let raw_commit = get_raw_commit(tree_id, parents, author, commiter, message);
    let commit_id = odb.write_raw(&raw_commit, ObjectType::Commit)?;
    Ok(commit_id)
}

fn get_raw_commit(
    tree_id: ObjectId,
    parents: &[ObjectId],
    author: AuthorInfo,
    commiter: CommiterInfo,
    message: &str,
) -> Vec<u8> {
    let mut builder = RawCommitBuilder::new();
    builder.add_tree(tree_id);
    for parent in parents {
        builder.add_parent_commit(*parent);
    }
    builder
        .add_author(author)
        .add_commiter(commiter)
        .add_newline()
        .add_message(message);
    builder.build()
}

struct RawCommitBuilder {
    buffer: Vec<u8>,
}

impl RawCommitBuilder {
    fn new() -> Self {
        Self { buffer: vec![] }
    }

    fn add_tree(&mut self, id: ObjectId) -> &mut Self {
        self.add_object_info(b"tree", id)
    }

    fn add_parent_commit(&mut self, id: ObjectId) -> &mut Self {
        self.add_object_info(b"parent", id)
    }

    fn add_author(&mut self, author: AuthorInfo) -> &mut Self {
        self.add_signature(b"author", author.0)
    }

    fn add_commiter(&mut self, commiter: CommiterInfo) -> &mut Self {
        self.add_signature(b"committer", commiter.0)
    }

    fn add_newline(&mut self) -> &mut Self {
        self.buffer.push(b'\n');
        self
    }

    fn add_message(&mut self, message: &str) -> &mut Self {
        self.buffer.extend_from_slice(message.as_bytes());
        self.add_newline();
        self
    }

    fn add_signature(&mut self, role: &[u8], signature: Signature) -> &mut Self {
        let Signature {
            name,
            email,
            time: mut when,
        } = signature;
        self.buffer.extend_from_slice(role);
        self.buffer.push(b' ');
        self.buffer.extend_from_slice(name.as_bytes());
        self.buffer.push(b' ');
        self.buffer.push(b'<');
        self.buffer.extend_from_slice(email.as_bytes());
        self.buffer.push(b'>');
        self.buffer.push(b' ');
        self.buffer
            .extend_from_slice(when.unix_time.to_string().as_bytes());
        self.buffer.push(b' ');
        let sign = if when.offset < 0 {
            when.offset = -when.offset;
            b'-'
        } else {
            b'+'
        };
        self.buffer.push(sign);

        let hours = format!("{:02}", when.offset / 3600);
        self.buffer.extend_from_slice(hours.as_bytes());

        let minutes = format!("{:02}", when.offset % 3600);
        self.buffer.extend_from_slice(minutes.as_bytes());
        self.add_newline();
        self
    }

    fn add_object_info(&mut self, name: &[u8], id: ObjectId) -> &mut Self {
        self.buffer.extend_from_slice(name);
        self.buffer.push(b' ');
        self.buffer.extend_from_slice(id.to_string().as_bytes());
        self.add_newline();
        self
    }

    fn build(self) -> Vec<u8> {
        self.buffer
    }
}
