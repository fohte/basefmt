use clap::Parser;
use ignore::Walk;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
struct Args {
    #[clap(default_value = ".", help = "List of files/directories to format")]
    paths: Vec<PathBuf>,

    #[clap(short, long, help = "Check mode (don't write changes)")]
    check: bool,
}

fn find_files(paths: &[impl AsRef<Path>]) -> io::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut has_error = false;

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
                        Err(err) => {
                            eprintln!("{}: {}", path.display(), err);
                            has_error = true;
                        }
                    }
                }
            }
            Err(err) => {
                eprintln!("{}: {}", path.display(), err);
                has_error = true;
                continue;
            }
        }
    }

    if has_error {
        Err(io::Error::new(io::ErrorKind::Other, "some files had errors"))
    } else {
        Ok(files)
    }
}

fn main() -> ExitCode {
    let args = Args::parse();

    if args.check {
        println!("Check mode engaged. No changes will be made.");
    }

    match find_files(&args.paths) {
        Ok(files) => {
            println!("Processing the following files: {:?}", files);
            ExitCode::from(0)
        }
        Err(_) => ExitCode::from(2),
    }
}
