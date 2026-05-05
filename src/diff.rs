use crate::object_db;
use crate::object_id::ObjectId;
use crate::repo::Repository;
use crate::tree::{self, Tree, TreeWalker};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read tree: {0}")]
    OdbReadFailed(object_db::Error),

    #[error("tree walking failed: {0}")]
    TreeWalkingFailed(tree::Error),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    Unmodified,
    Added,
    Deleted,
    Modified,
}

pub struct Delta {
    pub old_file: Option<(Vec<u8>, ObjectId)>,
    pub new_file: Option<(Vec<u8>, ObjectId)>,
    pub status: Status,
}

pub struct TreeDiff<'a> {
    repo: &'a Repository,
    deltas: Vec<Delta>,
}

impl<'a> TreeDiff<'a> {
    pub fn new(repo: &'a Repository) -> TreeDiff<'a> {
        TreeDiff {
            repo,
            deltas: Vec::new(),
        }
    }

    pub fn compare_trees(mut self, left: Tree, right: Tree) -> Result<Vec<Delta>, Error> {
        self.compare_trees_impl(left, right)?;
        Ok(self.deltas)
    }
}

impl<'a> TreeDiff<'a> {
    fn compare_trees_impl(&mut self, left: Tree, right: Tree) -> Result<(), Error> {
        if left == right {
            return Ok(());
        }

        let mut left_walker =
            TreeWalker::from_tree(self.repo, left).map_err(Error::TreeWalkingFailed)?;
        let mut right_walker =
            TreeWalker::from_tree(self.repo, right).map_err(Error::TreeWalkingFailed)?;

        while let Some(left_entry) = left_walker.current()
            && let Some(right_entry) = right_walker.current()
        {
            let left_path = left_entry.make_fullpath();
            let right_path = right_entry.make_fullpath();

            if left_path < right_path {
                let delta = Delta {
                    old_file: Some((left_path, left_entry.object_id())),
                    new_file: None,
                    status: Status::Deleted,
                };

                self.deltas.push(delta);
                left_walker.advance().map_err(Error::TreeWalkingFailed)?;
            } else if left_path == right_path {
                if left_entry.object_id() != right_entry.object_id() {
                    let delta = Delta {
                        old_file: Some((left_path, left_entry.object_id())),
                        new_file: Some((right_path, right_entry.object_id())),
                        status: Status::Modified,
                    };

                    self.deltas.push(delta);
                }

                left_walker.advance().map_err(Error::TreeWalkingFailed)?;
                right_walker.advance().map_err(Error::TreeWalkingFailed)?;
            } else {
                let delta = Delta {
                    old_file: None,
                    new_file: Some((right_path, right_entry.object_id())),
                    status: Status::Added,
                };

                self.deltas.push(delta);
                right_walker.advance().map_err(Error::TreeWalkingFailed)?;
            }
        }

        while let Some(entry) = left_walker.current() {
            let delta = Delta {
                old_file: Some((entry.make_fullpath(), entry.object_id())),
                new_file: None,
                status: Status::Deleted,
            };

            self.deltas.push(delta);
            left_walker.advance().map_err(Error::TreeWalkingFailed)?;
        }

        while let Some(entry) = right_walker.current() {
            let delta = Delta {
                old_file: None,
                new_file: Some((entry.make_fullpath(), entry.object_id())),
                status: Status::Added,
            };

            self.deltas.push(delta);
            right_walker.advance().map_err(Error::TreeWalkingFailed)?;
        }
        Ok(())
    }
}
