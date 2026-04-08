use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::object_id::{self, ObjectId};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("invalid symbolic ref")]
    InvalidSymbolicRef,

    #[error("branch does not exists")]
    BranchNotFound,

    #[error("branch contains invalid commit id: {0}")]
    InvalidCommitId(object_id::Error),

    #[error("invalid reference name: at index {0}")]
    InvalidRefName(usize),

    #[error("cannot create refs/heads/{0}: {1}")]
    FailedCreateBranchFile(String, io::Error),
}

pub struct Reference {
    pub name: String,
    pub target: ObjectId,
}

pub struct Refs {
    git_dir: PathBuf,
}

impl Refs {
    pub fn new(git_dir: &Path) -> Self {
        Self {
            git_dir: git_dir.to_path_buf(),
        }
    }

    pub fn get_refs_dir(&self) -> PathBuf {
        self.git_dir.join("refs")
    }

    pub fn get_branch_dir(&self) -> PathBuf {
        let mut path = self.get_refs_dir();
        path.push("heads");
        path
    }

    pub fn create_branch(
        &self,
        branch_name: String,
        target_commit: ObjectId,
    ) -> Result<Reference, Error> {
        if let Err(idx) = validate_ref_name(branch_name.as_bytes()) {
            return Err(Error::InvalidRefName(idx));
        }

        let branch_path = self.get_branch_dir().join(&branch_name);
        if let Err(e) = fs::create_dir_all(branch_path.parent().unwrap()) {
            return Err(Error::FailedCreateBranchFile(branch_name, e));
        }

        let mut file = match File::create_new(branch_path) {
            Ok(file) => file,
            Err(e) => return Err(Error::FailedCreateBranchFile(branch_name, e)),
        };

        io::copy(&mut target_commit.to_string().as_bytes(), &mut file)?;
        let reference = Reference {
            name: branch_name,
            target: target_commit,
        };
        Ok(reference)
    }

    pub fn update_branch(&self, branch_name: String, target_commit: ObjectId) -> Result<(), Error> {
        let branch_path = self.get_branch_dir().join(&branch_name);
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(branch_path)?;
        file.write_all(target_commit.to_string().as_bytes())?;
        Ok(())
    }

    pub fn lookup_branch(&self, name: &str) -> Result<Reference, Error> {
        let branch_path = self.get_branch_dir().join(&name);
        let file = File::open(branch_path)?;
        let s = io::read_to_string(file)?;
        println!("{}", s);
        let commit_id = ObjectId::from_str(s.trim_ascii_end()).map_err(Error::InvalidCommitId)?;
        let reference = Reference {
            name: name.to_string(),
            target: commit_id,
        };
        Ok(reference)
    }

    pub fn resolve_symbolic_ref(&self, ref_name: &str) -> Result<Reference, Error> {
        let mut ref_file = File::open(self.git_dir.join(&ref_name))?;
        let mut buffer = Vec::new();
        ref_file.read_to_end(&mut buffer)?;

        let prefix = b"ref: refs/heads/";
        if !buffer.starts_with(prefix) {
            return Err(Error::InvalidSymbolicRef);
        }

        let (_, rest) = buffer.split_at(prefix.len());
        let rest = rest.trim_ascii_end();
        if rest.is_empty() {
            return Err(Error::InvalidSymbolicRef);
        }

        if let Err(idx) = validate_ref_name(rest) {
            return Err(Error::InvalidRefName(idx));
        }

        match String::from_utf8(rest.to_vec()) {
            Ok(s) => self.lookup_branch(&s),
            Err(e) => Err(Error::InvalidRefName(e.utf8_error().valid_up_to())),
        }
    }
}

// According to git-check-ref-format
fn validate_ref_name(name: &[u8]) -> Result<(), usize> {
    if name == b"@" {
        return Err(0);
    }

    let mut i = 0usize;
    while i < name.len() {
        let c = name[i];

        if is_forbidden(c) {
            return Err(i);
        }

        if c == b'.' && i + 1 < name.len() && name[i + 1] == b'.' {
            return Err(i);
        }

        if c == b'@' && i + 1 < name.len() && name[i + 1] == b'{' {
            return Err(i);
        }

        i += 1;
    }

    Ok(())
}

fn is_forbidden(c: u8) -> bool {
    c.is_ascii_control() || matches!(c, b':' | b'?' | b'[' | b'\\' | b'^' | b'~' | b' ' | b'*')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn invalid_ref_names_are_rejected() -> testing::Result<()> {
        assert!(validate_ref_name(b"@").is_err());

        assert!(validate_ref_name(b":").is_err());
        assert!(validate_ref_name(b"?").is_err());
        assert!(validate_ref_name(b"[").is_err());
        assert!(validate_ref_name(b"\\").is_err());
        assert!(validate_ref_name(b"^").is_err());
        assert!(validate_ref_name(b"~").is_err());
        assert!(validate_ref_name(b" ").is_err());
        assert!(validate_ref_name(b"*").is_err());

        assert!(validate_ref_name(b"..").is_err());
        assert!(validate_ref_name(b"abc..").is_err());
        assert!(validate_ref_name(b"..abc").is_err());

        assert!(validate_ref_name(b"@{").is_err());
        assert!(validate_ref_name(b"abc@{").is_err());
        assert!(validate_ref_name(b"@{abc").is_err());
        Ok(())
    }
}
