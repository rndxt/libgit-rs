use std::env;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use git_rs::checkout::checkout_tree;
use git_rs::commit::create_commit_from_index;
use git_rs::index::{add_path_to_index, remove_path_from_index, write_index};
use git_rs::myers::{Action, Myers};
use git_rs::object_db::{Object, hash_file};
use git_rs::object_id::ObjectId;
use git_rs::object_type::ObjectType;
use git_rs::refs::HeadState;
use git_rs::repo::{Repository, RepositoryInitOptions};
use git_rs::signature::{AuthorInfo, CommitterInfo, Signature};
use git_rs::tag::{Tag, create_annotated_tag, lookup_tag_by_id, lookup_tag_by_name};
use git_rs::time::Time;
use git_rs::tree::{TreeWalker, create_trees_from_index};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a new Git repository in the current folder
    Init,

    /// Compute object ID for given file
    GetId {
        file: PathBuf,
    },

    /// Create blob object from given file
    StoreToOdb {
        file: PathBuf,
    },

    /// Read Index file of repository in the current folder and print its content
    ParseIndex,

    /// Add or update an index entry from a file on disk
    AddToIndex {
        file: PathBuf,
    },

    /// Remove an index entry corresponding to a file on disk
    RemoveFromIndex {
        file: PathBuf,
    },

    /// Create a tree object from the current index
    IndexToTree,

    /// Create a commit object from current index
    IndexToCommit,

    /// Create a new branch that points to a specified commit
    CreateBranch {
        branch_name: String,
        commit_id: String,
    },

    /// Set branch to a specified commit
    UpdateBranch {
        branch_name: String,
        commit_id: String,
    },

    /// Print content of specified tree object
    WalkTree {
        id: String,
    },

    CreateTag {
        id: String,
        name: String,
        message: String,
    },

    LookupTag {
        str: String,
    },

    PrintObject {
        id: String,
    },

    Checkout {
        id: String,
    },

    Diff {
        left: String,
        right: String,
    },
}

fn init(path: &Path) -> Result<()> {
    let options = RepositoryInitOptions {
        default_branch: String::from("master"),
    };

    let _repo = Repository::create_new(path, &options)?;
    Ok(())
}

fn store_file_to_odb(repo: &Path, path: &Path) -> Result<()> {
    let repo = Repository::open(repo)?;

    let file = File::open(path)?;
    let stat = file.metadata()?;

    let db = repo.object_db();
    let id = db.write_file(&file, &stat, ObjectType::Blob)?;
    println!("{}", id);
    Ok(())
}

fn get_object_id(path: &Path) -> Result<()> {
    let id = hash_file(path, ObjectType::Blob)?;
    println!("{}", id);
    Ok(())
}

fn parse_index(path: &Path) -> Result<()> {
    let repo = Repository::open(path)?;
    let index = repo.read_index()?;
    index.entries.iter().for_each(|e| {
        println!(
            "{:#o} {} {}",
            e.mode,
            e.id,
            String::from_utf8(e.path.clone()).unwrap(),
        );
    });
    println!("{:?}", index.entries.len());
    Ok(())
}

fn index_to_tree(repo: &Path) -> Result<()> {
    let repo = Repository::open(repo)?;
    let odb = repo.object_db();
    let index = repo.read_index()?;
    let id = create_trees_from_index(&odb, &index)?;
    println!("{}", id);
    Ok(())
}

fn add_to_index(repo: &Path, path: &Path) -> Result<()> {
    let repo = Repository::open(repo)?;
    let mut index = repo.read_index()?;
    add_path_to_index(path, &repo, &mut index)?;
    write_index(&repo, &index)?;
    Ok(())
}

fn index_to_commit(repo: &Path) -> Result<()> {
    let repo = Repository::open(repo)?;

    let author = Signature::build("author", "author@email", Time::new(1771253662, 10800)).unwrap();
    let committer =
        Signature::build("committer", "commiter@email", Time::new(1771253662, 10810)).unwrap();
    let message = "Commit Message";

    let refs = repo.refs();
    match refs.resolve_head()? {
        HeadState::Detached(_) => {
            println!("HEAD is detached. Switch to branch and commit changes");
        },
        HeadState::Normal(branch) => {
            let commit_id = create_commit_from_index(
                &repo,
                AuthorInfo(author),
                CommitterInfo(committer),
                message,
                &[branch.target],
            )?;
            println!("{}", commit_id);
        },
    }
    Ok(())
}

fn remove_from_index(repo: &Path, path: &Path) -> Result<()> {
    let repo = Repository::open(repo)?;
    let mut index = repo.read_index()?;
    remove_path_from_index(path, &mut index)?;
    write_index(&repo, &index)?;
    Ok(())
}

fn update_branch(repo: &Path, branch_name: String, commit_id: String) -> Result<()> {
    let repo = Repository::open(repo)?;
    let refs = repo.refs();
    let commit_id = ObjectId::from_str(commit_id.trim_ascii()).unwrap();
    refs.update_branch(branch_name, commit_id)?;
    Ok(())
}

fn create_branch(repo: &Path, branch_name: String, commit_id: String) -> Result<()> {
    let repo = Repository::open(repo)?;
    let refs = repo.refs();
    let commit_id = if commit_id == "master" {
        refs.lookup_branch("master")?.target
    } else {
        ObjectId::from_str(&commit_id)?
    };
    refs.create_branch(&branch_name, commit_id)?;
    Ok(())
}

fn walk_tree(repo: &Path, id: String) -> Result<()> {
    let repo = Repository::open(repo)?;
    let id = ObjectId::from_str(&id)?;
    let mut walker = TreeWalker::from_id(id, &repo)?;
    while let Some(entry) = walker.current() {
        let path = entry.make_fullpath();
        println!("{}", str::from_utf8(&path).unwrap());
        walker.advance()?;
    }
    Ok(())
}

fn lookup_tag(repo: &Path, str: String) -> Result<()> {
    let repo = Repository::open(repo)?;
    let tag = match ObjectId::from_str(&str) {
        Ok(id) => lookup_tag_by_id(&repo, id),
        Err(_) => lookup_tag_by_name(&repo, &str),
    }?;

    println!("name: {}", tag.name);
    println!("tagger: {}", tag.tagger);
    println!("message: {}", tag.message);
    println!("point-to: {} {}", tag.object_type, tag.object_id);
    Ok(())
}

fn create_tag(repo: &Path, id: String, name: String, message: String) -> Result<()> {
    let repo = Repository::open(repo)?;
    let id = ObjectId::from_str(&id)?;
    let tagger = Signature::build("author", "author@email", Time::new(1771253662, 10800)).unwrap();

    let tag = Tag {
        object_id: id,
        object_type: ObjectType::Commit, // TODO
        tagger,
        name,
        message,
    };

    let tag_id = create_annotated_tag(&repo, &tag)?;
    println!("{}", tag_id);
    Ok(())
}

fn print_object(repo: &Path, id: String) -> Result<()> {
    let repo = Repository::open(repo)?;
    let id = ObjectId::from_str(&id)?;
    let odb = repo.object_db();
    match odb.read_object(id)? {
        Object::Blob(blob) => {
            println!("blob");
            if let Ok(str) = str::from_utf8(&blob.data) {
                print!("{}", str);
            } else {
                println!("{:?}", blob.data);
            }
        },
        Object::Tree(tree) => {
            println!("tree");
            for entry in tree.entries {
                println!(
                    "{:#08o} {} {}",
                    entry.mode,
                    entry.id,
                    String::from_utf8(entry.filename.clone()).unwrap(),
                )
            }
        },
        Object::Commit(commit) => {
            println!("commit");
            println!("tree: {}", commit.tree_id);
            for parent in commit.parents {
                println!("parent: {}", parent);
            }

            println!("author: {}", commit.author);
            println!("committer: {}", commit.committer);
            println!("message: {}", commit.message);
        },
        Object::Tag(tag) => {
            println!("name: {}", tag.name);
            println!("tagger: {}", tag.tagger);
            println!("message: {}", tag.message);
            println!("point-to: {} {}", tag.object_type, tag.object_id);
        },
    };
    Ok(())
}

fn checkout(repo: &Path, id: String) -> Result<()> {
    let repo = Repository::open(repo)?;
    checkout_tree(&repo, &id)?;
    Ok(())
}

fn diff(left: String, right: String) -> Result<()> {
    let mut left = File::open(left)?;
    let mut a = Vec::new();
    left.read_to_end(&mut a)?;

    let mut right = File::open(right)?;
    let mut b = Vec::new();
    right.read_to_end(&mut b)?;

    let edits = Myers::new(&a, &b).diff();
    for edit in edits {
        match edit.action {
            Action::Nothing => {
                let a = edit.a.as_ref().unwrap();
                let b = edit.b.as_ref().unwrap();
                let str = std::str::from_utf8(&a.1).unwrap();
                println!("  {} {} {}", a.0, b.0, str);
            },
            Action::Add => {
                let b = edit.b.as_ref().unwrap();
                let str = std::str::from_utf8(&b.1).unwrap();
                println!("+   {} {}", b.0, str);
            },
            Action::Delete => {
                let a = edit.a.as_ref().unwrap();
                let str = std::str::from_utf8(&a.1).unwrap();
                println!("- {}   {}", a.0, str);
            },
        }
    }

    Ok(())
}

fn run() -> Result<()> {
    let current_dir = env::current_dir()?;
    let args = Args::parse();
    match args.command {
        Command::Init => init(&current_dir),
        Command::GetId { file } => get_object_id(&file),
        Command::StoreToOdb { file } => store_file_to_odb(&current_dir, &file),
        Command::ParseIndex => parse_index(&current_dir),
        Command::AddToIndex { file } => add_to_index(&current_dir, &file),
        Command::RemoveFromIndex { file } => remove_from_index(&current_dir, &file),
        Command::IndexToTree => index_to_tree(&current_dir),
        Command::IndexToCommit => index_to_commit(&current_dir),
        Command::UpdateBranch {
            branch_name,
            commit_id,
        } => update_branch(&current_dir, branch_name, commit_id),
        Command::CreateBranch {
            branch_name,
            commit_id,
        } => create_branch(&current_dir, branch_name, commit_id),
        Command::WalkTree { id } => walk_tree(&current_dir, id),
        Command::CreateTag { id, name, message } => create_tag(&current_dir, id, name, message),
        Command::LookupTag { str } => lookup_tag(&current_dir, str),
        Command::PrintObject { id } => print_object(&current_dir, id),
        Command::Checkout { id } => checkout(&current_dir, id),
        Command::Diff { left, right } => diff(left, right),
    }
}

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
