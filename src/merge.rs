use crate::blob::Blob;
use crate::commit::Commit;
use crate::diff3::{Chunk, Diff3};
use crate::merge_base::{self, find_merge_bases};
use crate::myers::Line;
use crate::object_db::IOdb;
use crate::object_id::ObjectId;
use crate::repo::Repository;
use crate::tree::TreeWalker;
use crate::{FileMode, diff, object_db, tree};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read commit object: {0}")]
    OdbReadFailed(object_db::Error),

    #[error("cannot store object to odb: {0}")]
    OdbWriteFailed(object_db::Error),

    #[error("cannot find merge base: {0}")]
    CannotComputeMergeBase(merge_base::Error),

    #[error("cannot create tree iterator: {0}")]
    CannotCreateIterator(tree::Error),

    #[error("cannot advance tree iterator: {0}")]
    IteratorAdvanceFailed(tree::Error),

    #[error("merge conflict")]
    MergeConflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexEntry {
    Normal(TreeEntry),
    Conflict {
        o: Option<TreeEntry>,
        a: Option<TreeEntry>,
        b: Option<TreeEntry>,
    },
}

impl IndexEntry {
    fn filename(&self) -> &[u8] {
        match self {
            IndexEntry::Normal(e) => &e.filename,
            IndexEntry::Conflict { o, a, b } => match (o, a, b) {
                (Some(o), _, _) => &o.filename,
                (_, Some(a), _) => &a.filename,
                (_, _, Some(b)) => &b.filename,
                (_, _, _) => unreachable!(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InMemoryIndex {
    pub entries: Vec<IndexEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub mode: FileMode,
    pub filename: Vec<u8>,
    pub id: ObjectId,
    pub flags: u16,
}

// const STAGE_BIT_1: u16 = 0x1 << 12;
// const STAGE_BIT_2: u16 = 0x2 << 12;

pub struct InMemoryCommit {
    pub parents: Vec<ObjectId>,
    pub index: InMemoryIndex,
}

struct InMemoryCommitWalker<'a> {
    entries: &'a Vec<IndexEntry>,
    idx: usize,
}

impl<'a> InMemoryCommitWalker<'a> {
    fn new(commit: &'a InMemoryCommit) -> Self {
        let mut walker = Self {
            entries: &commit.index.entries,
            idx: 0,
        };
        walker.advance_to_next_non_conflict();
        walker
    }

    fn current(&self) -> Option<TreeEntry> {
        match self.entries.get(self.idx).cloned()? {
            IndexEntry::Normal(e) => Some(e),
            _ => unreachable!(),
        }
    }

    fn advance(&mut self) {
        if self.idx != self.entries.len() {
            self.advance_to_next_non_conflict();
        }
    }

    fn advance_to_next_non_conflict(&mut self) {
        while self.idx < self.entries.len()
            && let IndexEntry::Conflict { .. } = self.entries[self.idx]
        {
            self.idx += 1;
        }
    }
}

enum Elem {
    CommitFromOdb { commit: Commit, id: ObjectId },
    InMemoryCommit(InMemoryCommit),
}

impl From<(Commit, ObjectId)> for Elem {
    fn from(value: (Commit, ObjectId)) -> Self {
        Elem::CommitFromOdb {
            commit: value.0,
            id: value.1,
        }
    }
}

impl From<&(Commit, ObjectId)> for Elem {
    fn from(value: &(Commit, ObjectId)) -> Self {
        Elem::CommitFromOdb {
            commit: value.0.clone(),
            id: value.1,
        }
    }
}

impl Elem {
    fn get_parents(&self) -> Vec<ObjectId> {
        match self {
            Elem::CommitFromOdb { id, .. } => vec![*id],
            Elem::InMemoryCommit(commit) => commit.parents.clone(),
        }
    }
}

enum ElemIterator<'a, T>
where
    T: IOdb,
{
    TreeIterator(TreeWalker<'a, T>),
    InMemoryIterator(InMemoryCommitWalker<'a>),
}

impl<'a, T> ElemIterator<'a, T>
where
    T: IOdb,
{
    fn try_from_elem(odb: &'a T, elem: &'a Elem) -> Result<Self, Error> {
        match elem {
            Elem::CommitFromOdb { commit, .. } => TreeWalker::from_id(odb, commit.tree_id)
                .map_err(Error::CannotCreateIterator)
                .map(Self::TreeIterator),
            Elem::InMemoryCommit(commit) => Ok(ElemIterator::InMemoryIterator(
                InMemoryCommitWalker::new(commit),
            )),
        }
    }

    fn current(&self) -> Option<TreeEntry> {
        match self {
            ElemIterator::TreeIterator(it) => {
                let entry = it.current()?;
                let entry = TreeEntry {
                    mode: FileMode::from(entry.mode()),
                    filename: entry.make_fullpath(),
                    id: entry.object_id(),
                    flags: 0,
                };
                Some(entry)
            },
            ElemIterator::InMemoryIterator(it) => it.current(),
        }
    }

    fn advance(&mut self) -> Result<(), Error> {
        match self {
            ElemIterator::TreeIterator(it) => it.advance().map_err(Error::IteratorAdvanceFailed),
            ElemIterator::InMemoryIterator(it) => {
                it.advance();
                Ok(())
            },
        }
    }
}

fn walk_iterators<T, F>(iterators: &mut [ElemIterator<T>], mut callback: F) -> Result<(), Error>
where
    T: IOdb,
    F: FnMut(&[Option<TreeEntry>]) -> Result<(), Error>,
{
    let mut current = vec![None; iterators.len()];
    loop {
        current.fill(None);
        if let Some(path) = iterators
            .iter()
            .filter_map(ElemIterator::current)
            .map(|e| e.filename)
            .min()
        {
            for (i, it) in iterators.iter().enumerate() {
                if let Some(entry) = it.current()
                    && entry.filename == path
                {
                    current[i] = Some(entry.clone());
                } else {
                    current[i] = None;
                }
            }

            callback(&current)?;

            for (i, e) in current.iter().enumerate() {
                if e.is_some() {
                    iterators[i].advance()?;
                }
            }
        } else {
            return Ok(());
        }
    }
}

impl FileStatus {
    fn from_base_and_other(base: &Option<TreeEntry>, other: &Option<TreeEntry>) -> Self {
        match (base, other) {
            (Some(o), Some(a)) => {
                let mode_o = FileMode::from(o.mode);
                let mode_a = FileMode::from(a.mode);
                debug_assert_eq!(
                    mode_o.is_tree() != mode_a.is_tree(),
                    mode_o.is_tree() & mode_a.is_tree()
                );
                if mode_o.is_tree() != mode_a.is_tree() {
                    Self::TypeChanged
                } else if o.id != a.id {
                    Self::Modified
                } else {
                    Self::Unmodified
                }
            },
            (Some(_), None) => Self::Deleted,
            (None, Some(_)) => Self::Added,
            (None, None) => Self::Unmodified,
        }
    }
}

type FileStatus = diff::Status;

#[derive(Debug, Clone)]
struct MergeDiff {
    o: Option<TreeEntry>,
    a: Option<TreeEntry>,
    status_o_to_a: FileStatus,

    b: Option<TreeEntry>,
    status_o_to_b: FileStatus,
}

fn get_status_between(a: &TreeEntry, b: &TreeEntry) -> FileStatus {
    let mode_a = FileMode::from(a.mode);
    let mode_b = FileMode::from(b.mode);

    debug_assert_eq!(
        mode_a.is_tree() != mode_b.is_tree(),
        mode_a.is_tree() ^ mode_b.is_tree()
    );

    if mode_a.is_tree() != mode_b.is_tree() {
        FileStatus::TypeChanged
    } else if a.id != b.id {
        FileStatus::Modified
    } else {
        FileStatus::Unmodified
    }
}

struct DiffList {
    staged: Vec<TreeEntry>,
    conflicts: Vec<MergeDiff>,
}

impl DiffList {
    fn new() -> Self {
        DiffList {
            staged: Vec::new(),
            conflicts: Vec::new(),
        }
    }

    fn add(&mut self, o: Option<TreeEntry>, a: Option<TreeEntry>, b: Option<TreeEntry>) {
        match (o, a, b) {
            (Some(o), Some(a), Some(b)) => {
                debug_assert!(o.filename == a.filename);
                debug_assert!(o.filename == b.filename);
                let a_to_o = get_status_between(&o, &a);
                let b_to_o = get_status_between(&o, &b);
                if a_to_o == FileStatus::Unmodified && b_to_o == FileStatus::Unmodified {
                    self.staged.push(o);
                } else {
                    self.conflicts.push(MergeDiff {
                        o: Some(o),
                        a: Some(a),
                        status_o_to_a: a_to_o,
                        b: Some(b),
                        status_o_to_b: b_to_o,
                    });
                }
            },
            (o, a, b) => {
                let status_a = FileStatus::from_base_and_other(&o, &a);
                let status_b = FileStatus::from_base_and_other(&o, &b);
                self.conflicts.push(MergeDiff {
                    o,
                    a,
                    status_o_to_a: status_a,
                    b,
                    status_o_to_b: status_b,
                });
            },
        };
    }
}

fn try_fold_no_conflicts(chunks: Vec<Chunk>) -> Option<Vec<u8>> {
    let mut buffer = Vec::new();
    for chunk in chunks {
        if let Chunk::Normal(lines) = chunk {
            for Line(_, mut content) in lines {
                buffer.append(&mut content);
            }
        } else {
            return None;
        }
    }
    Some(buffer)
}

struct OrtMerge<T: IOdb> {
    odb: T,
}

impl<T: IOdb> OrtMerge<T> {
    fn new(odb: T) -> Self {
        Self { odb }
    }

    fn create_index_from_diff_list(&mut self, list: DiffList) -> Result<InMemoryIndex, Error> {
        let mut entries = Vec::new();
        for conflict in list.conflicts {
            match (&conflict.status_o_to_a, &conflict.status_o_to_b) {
                (FileStatus::Unmodified, FileStatus::Deleted)
                | (FileStatus::Deleted, FileStatus::Unmodified)
                | (FileStatus::Deleted, FileStatus::Deleted) => {
                    // File is deleted in result also
                },
                (FileStatus::Unmodified, FileStatus::Modified) => {
                    entries.push(IndexEntry::Normal(conflict.b.unwrap()));
                },
                (FileStatus::Modified, FileStatus::Unmodified) => {
                    entries.push(IndexEntry::Normal(conflict.a.unwrap()));
                },
                (FileStatus::Added, FileStatus::Unmodified) => {
                    entries.push(IndexEntry::Normal(conflict.a.unwrap()));
                },
                (FileStatus::Unmodified, FileStatus::Added) => {
                    entries.push(IndexEntry::Normal(conflict.b.unwrap()));
                },
                (FileStatus::Added, FileStatus::Added) // todo
                | (FileStatus::Modified, FileStatus::Modified) => {
                    let original = self.odb.read_blob(conflict.o.as_ref().unwrap().id).map_err(Error::OdbReadFailed)?;
                    let left = self.odb.read_blob(conflict.a.as_ref().unwrap().id).map_err(Error::OdbReadFailed)?;
                    let right = self.odb.read_blob(conflict.b.as_ref().unwrap().id).map_err(Error::OdbReadFailed)?;
                    let diff = Diff3::new(&original.data, &left.data, &right.data).compare();
                    if let Some(data) = try_fold_no_conflicts(diff) {
                        let id = self.odb.write_blob(&Blob { data }).map_err(Error::OdbWriteFailed)?;
                        let TreeEntry { mode, filename, .. } = conflict.o.unwrap();
                        entries.push(IndexEntry::Normal( TreeEntry { mode, filename, id, flags: 0 }));
                    } else {
                        entries.push(IndexEntry::Conflict { o: conflict.o, a: conflict.a, b: conflict.b })
                    }
                },
                (FileStatus::Deleted, FileStatus::Modified)|(FileStatus::Modified, FileStatus::Deleted) => {
                    entries.push(IndexEntry::Conflict { o: conflict.o, a: conflict.a, b: conflict.b })
                },
                _ => unreachable!(),
            }
        }
        for e in list.staged {
            entries.push(IndexEntry::Normal(e));
        }
        entries.sort_by(|left, right| left.filename().cmp(right.filename()));
        Ok(InMemoryIndex { entries })
    }

    fn merge_iterators(
        &self,
        base: ElemIterator<T>,
        side1: ElemIterator<T>,
        side2: ElemIterator<T>,
    ) -> Result<DiffList, Error> {
        let mut diff_list = DiffList::new();
        walk_iterators([base, side1, side2].as_mut_slice(), |entries| {
            debug_assert_eq!(entries.len(), 3);
            diff_list.add(entries[0].clone(), entries[1].clone(), entries[2].clone());
            Ok(())
        })?;
        Ok(diff_list)
    }

    fn create_in_memory_base(
        &mut self,
        base: &Elem,
        other: &Elem,
    ) -> Result<InMemoryCommit, Error> {
        let (_, index) = self.merge_impl(base, other)?;
        let mut parents = Vec::new();
        parents.append(&mut base.get_parents());
        parents.append(&mut other.get_parents());
        let commit = InMemoryCommit { parents, index };
        Ok(commit)
    }

    fn compute_merge_base(&mut self, side1: &Elem, side2: &Elem) -> Result<Elem, Error> {
        let first_id = match side2 {
            Elem::CommitFromOdb { id, .. } => id,
            _ => unreachable!(),
        };

        let rest = side1.get_parents();
        let bases = find_merge_bases(&mut self.odb, *first_id, &rest)
            .map_err(Error::CannotComputeMergeBase)?;
        let base = Elem::from(&bases[0]);
        let result = bases[1..]
            .into_iter()
            .map(Elem::from)
            .try_fold(base, |acc, x| {
                self.create_in_memory_base(&acc, &x)
                    .map(Elem::InMemoryCommit)
            })?;
        Ok(result)
    }

    fn merge_impl(&mut self, side1: &Elem, side2: &Elem) -> Result<(Elem, InMemoryIndex), Error> {
        let base = self.compute_merge_base(side1, side2)?;
        let diff_list = self.merge_iterators(
            ElemIterator::try_from_elem(&self.odb, &base)?,
            ElemIterator::try_from_elem(&self.odb, side1)?,
            ElemIterator::try_from_elem(&self.odb, side2)?,
        )?;

        let index = self.create_index_from_diff_list(diff_list)?;
        Ok((base, index))
    }

    fn merge(
        &mut self,
        side1: (Commit, ObjectId),
        side2: (Commit, ObjectId),
    ) -> Result<(Elem, InMemoryIndex), Error> {
        self.merge_impl(&Elem::from(side1), &Elem::from(side2))
    }
}

pub fn merge_commits(
    repo: &Repository,
    side1: (Commit, ObjectId),
    side2: (Commit, ObjectId),
) -> Result<InMemoryIndex, Error> {
    let odb = repo.object_db();
    let (_, index) = OrtMerge::new(odb).merge(side1, side2)?;
    Ok(index)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::object_db::Object;
    use crate::signature::{AuthorInfo, CommitterInfo, Signature};
    use crate::testing;
    use crate::time::Time;
    use crate::tree::Tree;

    fn get_author_info() -> AuthorInfo {
        let sign =
            Signature::build("user", "example@domain.com", Time::new(1771253662, 10800)).unwrap();
        AuthorInfo(sign)
    }

    fn get_commiter_info() -> CommitterInfo {
        let sign =
            Signature::build("committer", "commiter@email", Time::new(1771253662, 10800)).unwrap();
        CommitterInfo(sign)
    }

    fn get_commit(tree_id: ObjectId, parents: Vec<ObjectId>) -> Commit {
        Commit::new(
            tree_id,
            parents,
            get_author_info(),
            get_commiter_info(),
            String::new(),
        )
    }

    #[test]
    fn left_add_and_right_remove() -> testing::Result<()> {
        let id = [
            // blob
            "0000000000000000000000000000000000000000",
            "1000000000000000000000000000000000000000",
            "2000000000000000000000000000000000000000",
            // tree
            "3000000000000000000000000000000000000000",
            "4000000000000000000000000000000000000000",
            "5000000000000000000000000000000000000000",
            // commit
            "6000000000000000000000000000000000000000",
            "7000000000000000000000000000000000000000",
            "8000000000000000000000000000000000000000",
        ]
        .map(ObjectId::from_str)
        .map(Result::unwrap);

        let mut odb = HashMap::new();
        let a = Blob::new(Vec::new());
        let b = Blob::new(Vec::new());
        let c = Blob::new(Vec::new());

        odb.insert(id[0], Object::Blob(a));
        odb.insert(id[1], Object::Blob(b));
        odb.insert(id[2], Object::Blob(c));

        let tree_o = Tree {
            entries: vec![
                tree::TreeEntry {
                    mode: FileMode::Blob.into(),
                    filename: b"a.txt".to_vec(),
                    id: id[0],
                },
                tree::TreeEntry {
                    mode: FileMode::Blob.into(),
                    filename: b"b.txt".to_vec(),
                    id: id[1],
                },
            ],
        };

        // delete b.txt
        let tree_left = Tree {
            entries: vec![tree::TreeEntry {
                mode: FileMode::Blob.into(),
                filename: b"a.txt".to_vec(),
                id: id[0],
            }],
        };

        // Add file c.txt
        let tree_right = Tree {
            entries: vec![
                tree::TreeEntry {
                    mode: FileMode::Blob.into(),
                    filename: b"a.txt".to_vec(),
                    id: id[0],
                },
                tree::TreeEntry {
                    mode: FileMode::Blob.into(),
                    filename: b"b.txt".to_vec(),
                    id: id[1],
                },
                tree::TreeEntry {
                    mode: FileMode::Blob.into(),
                    filename: b"c.txt".to_vec(),
                    id: id[2],
                },
            ],
        };

        odb.insert(id[3], Object::Tree(tree_o));
        odb.insert(id[4], Object::Tree(tree_left));
        odb.insert(id[5], Object::Tree(tree_right));

        let commits = [
            get_commit(id[3], vec![]),
            get_commit(id[4], vec![id[6]]),
            get_commit(id[5], vec![id[6]]),
        ];

        odb.insert(id[6], Object::Commit(commits[0].clone()));
        odb.insert(id[7], Object::Commit(commits[1].clone()));
        odb.insert(id[8], Object::Commit(commits[2].clone()));

        let side1 = (commits[1].clone(), id[7]);
        let side2 = (commits[2].clone(), id[8]);
        let (_, actual) = OrtMerge::new(odb).merge(side1, side2)?;

        let expected = InMemoryIndex {
            entries: vec![
                IndexEntry::Normal(TreeEntry {
                    mode: FileMode::Blob,
                    filename: b"a.txt".to_vec(),
                    id: id[0],
                    flags: 0,
                }),
                IndexEntry::Normal(TreeEntry {
                    mode: FileMode::Blob,
                    filename: b"c.txt".to_vec(),
                    id: id[2],
                    flags: 0,
                }),
            ],
        };
        assert_eq!(actual, expected);
        Ok(())
    }
}
