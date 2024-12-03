use anyhow::{Context, Result};
use clap::Parser;

use searcher::Searcher;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    query: String,
    path: std::path::PathBuf,
}

fn main() -> Result<()> {
    let args = Cli::parse();

    let mut filepath = args.path;

    if filepath == std::path::PathBuf::from("") {
        filepath = std::path::PathBuf::from(".");
    }

    let directory = std::fs::read_dir(&filepath)
        .with_context(|| format!("could not read directory `{:?}`", &filepath))?;

    let mut searcher = Searcher::new();

    for entry in directory {
        let entry = entry.with_context(|| format!("error while reading directory `{:?}`", &filepath))?;

        // TODO: handle symlinks and directories
        match entry.file_type().with_context(|| format!("could not get file type of `{:?}`", &entry.path()))? {
            t if t.is_file() => (),
            t if t.is_dir() => continue,
            t if t.is_symlink() => continue,
            _ => continue,
        }

        let file_name_os_str = entry.file_name();
        let filename = file_name_os_str.to_string_lossy();
        
        let contents = std::fs::read_to_string(entry.path()).with_context(|| format!("could not read file `{:?}`", filename))?;

         searcher.add_document(&filename, &contents);
    }

    let results = searcher.search(&args.query);
    
    if results.is_empty() {
        return Err(anyhow::anyhow!(format!("No results found for query: {}", args.query)));
    }

    for (doc_id, score) in results {
        println!("doc_id: {}, score: {}", doc_id, score);
    }

    Ok(())
}
