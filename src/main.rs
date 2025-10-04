use clap::Parser;
use ignore::Walk;
use std::path::{Path, PathBuf};

#[derive(Parser)]
struct Args {
    #[clap(default_value = ".", help = "List of files/directories to format")]
    paths: Vec<PathBuf>,

    #[clap(short, long, help = "Check mode (don't write changes)")]
    check: bool,
}

fn find_files(paths: &[impl AsRef<Path>]) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();

    for path in paths {
        let path = path.as_ref();

        match path.metadata() {
            Ok(_) => {
                for result in Walk::new(path) {
                    match result {
                        Ok(entry) => {
                            if entry.file_type().map_or(false, |ft| ft.is_file()) {
                                files.push(entry.path().to_path_buf());
                            }
                        }
                        Err(err) => eprintln!("Error reading entry: {}", err),
                    }
                }
            }
            Err(err) => {
                eprintln!("{}: {}", path.display(), err);
                continue;
            }
        }
    }
    files
}

fn main() {
    let args = Args::parse();

    if args.check {
        println!("Check mode engaged. No changes will be made.");
    }

    let files = find_files(&args.paths);

    println!("Processing the following files: {:?}", files);
}
