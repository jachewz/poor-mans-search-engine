use anyhow::{Context, Result};
use clap::Parser;
use glob::{glob, Pattern};

use searcher::Searcher;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    query: String,
    path: String,
}

fn main() -> Result<()> {
    let args = Cli::parse();

    let path = args.path;

    let directory =
        glob(&path).with_context(|| format!("could not read directory `{:?}`", &path))?;

    let mut searcher = Searcher::new();

    for entry in directory {
        let entry =
            entry.with_context(|| format!("error while reading directory `{:?}`", &path))?;

        // TODO: handle symlinks and directories
        let metadata = entry.metadata()
            .with_context(|| format!("could not get metadata of `{:?}`", &entry.as_path()))?;
        match metadata.file_type()
        {
            t if t.is_file() => (),
            t if t.is_dir() => continue,
            t if t.is_symlink() => continue,
            _ => continue,
        }

        let filepath = entry.as_path();
        let filepath_str = filepath.to_str().with_context(|| format!("could not convert path `{:?}` to string", filepath))?;

        let contents = std::fs::read_to_string(filepath)
            .with_context(|| format!("could not read file `{:?}`", filepath))?;

        searcher.add_document(filepath_str, &contents);
    }

    let results = searcher.search(&args.query);

    if results.is_empty() {
        return Err(anyhow::anyhow!(format!(
            "No results found for query: {}",
            args.query
        )));
    }

    for (doc_id, score) in results {
        println!("doc_id: {}, score: {}", doc_id, score);
    }

    Ok(())
}
