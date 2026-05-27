use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::diff::{Delta, Status, TreeDiff};
use crate::index::{self, add_path_to_index, write_index};
use crate::object_db;
use crate::refs;
use crate::repo::Repository;
use crate::tree::{self, decay_to_tree};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot open file: {0}")]
    CannotOpenFile(io::Error),

    #[error("cannot create file: {0}")]
    CreateFileFailed(io::Error),

    #[error("cannot read from file: {0}")]
    CannotReadFromFile(io::Error),

    #[error("cannot write to file: {0}")]
    CannotWriteToFile(io::Error),

    #[error("cannot create dir: {0}")]
    CreateDirFailed(io::Error),

    #[error("cannot read tree: {0}")]
    OdbReadFailed(object_db::Error),

    #[error("cannot read Index: {0}")]
    CannotReadIndex(#[from] index::Error),

    #[error("cannot resolve ref: {0}")]
    CannotResolveRef(#[from] refs::Error),

    #[error("cannot get tree: {0}")]
    InvalidTree(tree::Error),

    #[error("cannot update HEAD: {0}")] // TODO: print
    UpdateHead(refs::Error),
}

pub fn checkout_tree(repo: &Repository, treeish: &str) -> Result<(), Error> {
    let (target_tree, target_id) = decay_to_tree(repo, treeish).map_err(Error::InvalidTree)?;
    let (current_tree, current_id) = decay_to_tree(repo, "HEAD").map_err(Error::InvalidTree)?;

    if target_id != current_id {
        let deltas = TreeDiff::new(repo)
            .compare_trees(current_tree, target_tree)
            .unwrap();

        apply_changes(repo, &deltas)?;
        update_index(repo, &deltas)?;
    }

    let refs = repo.refs();
    refs.set_head(treeish).map_err(Error::UpdateHead)?;
    Ok(())
}

fn update_index(repo: &Repository, deltas: &[Delta]) -> Result<(), Error> {
    let mut index = repo.read_index().map_err(Error::CannotReadIndex)?;

    for delta in deltas.iter().filter(|d| d.status == Status::Deleted) {
        if let Ok(i) = index.find_by_raw_path(&delta.old_file.as_ref().unwrap().0) {
            index.remove(i);
        }
    }

    for delta in deltas.iter().filter(|d| d.status == Status::Added) {
        let path = str::from_utf8(&delta.new_file.as_ref().unwrap().0).unwrap();
        let path = Path::new(path);
        add_path_to_index(path, repo, &mut index).unwrap();
    }

    for delta in deltas.iter().filter(|d| d.status == Status::Modified) {
        let path = str::from_utf8(&delta.new_file.as_ref().unwrap().0).unwrap();
        let path = Path::new(path);
        add_path_to_index(path, repo, &mut index).unwrap();
    }

    write_index(repo, &index).unwrap();
    Ok(())
}

fn apply_changes(repo: &Repository, deltas: &[Delta]) -> Result<(), Error> {
    for delta in deltas {
        match delta.status {
            Status::Added => {
                let (path, id) = delta.new_file.as_ref().unwrap();
                let odb = repo.object_db();
                let blob = odb.read_blob(*id).map_err(Error::OdbReadFailed)?;

                let path = str::from_utf8(path).unwrap(); // TODO
                let path = PathBuf::from(path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(repo.root_path().join(parent)).unwrap();
                }

                let path = repo.root_path().join(path);
                let mut file = File::create(path).unwrap(); // TODO
                file.write_all(&blob.data).unwrap();
            },
            Status::Modified => {
                let (path, id) = delta.new_file.as_ref().unwrap();
                println!("modify: {} {}", str::from_utf8(path).unwrap(), id);
                let odb = repo.object_db();
                let blob = odb.read_blob(*id).map_err(Error::OdbReadFailed)?;

                let path = str::from_utf8(path).unwrap(); // TODO
                let path = PathBuf::from(path);
                let mut file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(path)
                    .unwrap();

                file.write_all(&blob.data).unwrap();
            },
            Status::Deleted => {
                let (path, _) = delta.old_file.as_ref().unwrap();
                println!("remove: {}", str::from_utf8(path).unwrap());
                let path = str::from_utf8(path).unwrap(); // TODO
                let path = PathBuf::from(path);
                fs::remove_file(path).unwrap();
            },
            Status::Unmodified => {},
            _ => unreachable!(),
        };
    }
    Ok(())
}
