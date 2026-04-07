use std::env;
use std::fs::File;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use git_rs::add_path_to_index;
use git_rs::hash_file;
use git_rs::signature::{AuthorInfo, CommitterInfo};
use git_rs::write_index;
use git_rs::write_index_to_tree;
use git_rs::{ObjectType, remove_path_from_index};
use git_rs::{Repository, RepositoryInitOptions};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

use git_rs::commit::create_commit_from_index;
use git_rs::time::Time;

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init,
    GetId { file: PathBuf },
    StoreToOdb { file: PathBuf },
    ParseIndex,
    AddToIndex { file: PathBuf },
    RemoveFromIndex { file: PathBuf },
    IndexToTree,
    IndexToCommit,
}

fn init(path: &Path) -> Result<()> {
    let options = RepositoryInitOptions {
        default_branch: "master".to_string(),
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
    println!("{}", id.to_string());
    Ok(())
}

fn get_object_id(path: &Path) -> Result<()> {
    let id = hash_file(path, ObjectType::Blob)?;
    println!("{}", id.to_string());
    Ok(())
}

fn parse_index(path: &Path) -> Result<()> {
    let repo = Repository::open(path)?;
    let index = repo.read_index()?;
    index.entries.iter().for_each(|e| {
        println!(
            "{:#o} {} {}",
            e.mode,
            e.id.to_string(),
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
    let id = write_index_to_tree(&odb, &index)?;
    println!("{}", id.to_string());
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

    let author = AuthorInfo::build("author", "author@email", Time::new(1771253662, 10800)).unwrap();
    let commiter =
        CommitterInfo::build("committer", "commiter@email", Time::new(1771253662, 10810)).unwrap();
    let message = "Commit is created";
    let parents = &[];

    let commit_id = create_commit_from_index(&repo, author, commiter, message, parents)?;
    println!("{}", commit_id.to_string());
    Ok(())
}

fn remove_from_index(repo: &Path, path: &Path) -> Result<()> {
    let repo = Repository::open(repo)?;
    let mut index = repo.read_index()?;
    remove_path_from_index(path, &mut index)?;
    write_index(&repo, &index)?;
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
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
    }
}
