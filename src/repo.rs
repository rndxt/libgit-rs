use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::Index;
use crate::index::OpenIndexError;
use crate::object_db::ObjectDB;

pub struct RepositoryInitOptions {
    pub default_branch: String,
}

pub struct Repository {
    git_dir: PathBuf,
}

impl Repository {
    pub fn create_new(path: &Path, options: &RepositoryInitOptions) -> io::Result<Self> {
        let git_dir = RepositoryBuilder::new(path)
            .create_git_dir()?
            .create_info_dir()?
            .create_hooks_dir()?
            .create_description_file()?
            .create_config()?
            .create_refs_dir()?
            .create_objects_dir()?
            .create_head_file(&options.default_branch)?
            .done();

        let repository = Self { git_dir };
        Ok(repository)
    }

    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        // TODO: validate path
        let git_dir = path.as_ref().join(".git");
        let repo = Self { git_dir };
        Ok(repo)
    }

    pub fn read_index(&self) -> Result<Index, OpenIndexError> {
        let path = self.git_dir().join("index");
        Index::open(path)
    }

    pub fn object_db(&self) -> ObjectDB {
        let objects_dir = self.git_dir().join("objects");
        ObjectDB::new(objects_dir.as_path())
    }

    pub fn git_dir(&'_ self) -> &Path {
        self.git_dir.as_path()
    }
}

struct RepositoryBuilder {
    dir: PathBuf,
}

impl RepositoryBuilder {
    fn new<P: AsRef<Path>>(path: P) -> Self {
        let dir = path.as_ref().join(".git");
        Self { dir }
    }

    fn done(self) -> PathBuf {
        self.dir
    }

    fn create_info_dir(mut self) -> io::Result<Self> {
        self.dir.push("info");
        fs::create_dir(self.dir.as_path())?;

        self.dir.push("exclude");
        File::create_new(self.dir.as_path())?;
        self.dir.pop();

        self.dir.pop();
        Ok(self)
    }

    fn create_hooks_dir(mut self) -> io::Result<Self> {
        self.dir.push("hooks");
        fs::create_dir(self.dir.as_path())?;
        self.dir.pop();
        Ok(self)
    }

    fn create_head_file(mut self, branch: &str) -> io::Result<Self> {
        self.dir.push("HEAD");
        let mut head = File::create(self.dir.as_path())?;
        write!(head, "refs: refs/heads/{}", branch)?;
        self.dir.pop();
        Ok(self)
    }

    fn create_objects_dir(mut self) -> io::Result<Self> {
        self.dir.push("objects");
        fs::create_dir(self.dir.as_path())?;

        self.dir.push("info");
        fs::create_dir(self.dir.as_path())?;
        self.dir.pop();

        self.dir.push("pack");
        fs::create_dir(self.dir.as_path())?;
        self.dir.pop();

        self.dir.pop();
        Ok(self)
    }

    fn create_refs_dir(mut self) -> io::Result<Self> {
        self.dir.push("refs");
        fs::create_dir(self.dir.as_path())?;

        self.dir.push("heads");
        fs::create_dir(self.dir.as_path())?;
        self.dir.pop();

        self.dir.push("tags");
        fs::create_dir(self.dir.as_path())?;
        self.dir.pop();

        self.dir.pop();
        Ok(self)
    }

    // fn create_index_file(mut self) -> io::Result<Self> {
    //     self.dir.push("index");
    //     File::create_new(self.dir.as_path())?;
    //     self.dir.pop();
    //     Ok(self)
    // }

    fn create_git_dir(self) -> io::Result<Self> {
        fs::create_dir(self.dir.as_path())?;
        Ok(self)
    }

    fn create_description_file(mut self) -> io::Result<Self> {
        self.dir.push("description");
        File::create_new(self.dir.as_path())?;
        self.dir.pop();
        Ok(self)
    }

    fn create_config(mut self) -> io::Result<Self> {
        self.dir.push("config");
        let mut config = File::create(self.dir.as_path())?;
        writeln!(config, "[core]")?;
        writeln!(config, "\trepositoryformatversion = 0")?;
        writeln!(config, "\tbare = false")?;
        writeln!(config, "\tlogallrefupdates = true")?;
        self.dir.pop();
        Ok(self)
    }
}
