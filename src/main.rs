use basefmt::find::find_files;
use basefmt::format::{check_file, format_file};
use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
struct Args {
    #[clap(default_value = ".", help = "List of files/directories to format")]
    paths: Vec<PathBuf>,

    #[clap(short, long, help = "Check mode (don't write changes)")]
    check: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let files = match find_files(&args.paths) {
        Ok(files) => files,
        Err(_) => return ExitCode::from(2),
    };

    if args.check {
        run_check(&files)
    } else {
        run_format(&files)
    }
}

fn run_format(files: &[PathBuf]) -> ExitCode {
    let mut has_error = false;

    for file in files {
        match format_file(file) {
            Ok(_changed) => {
                // Successfully formatted
            }
            Err(err) => {
                eprintln!("{}: {}", file.display(), err);
                has_error = true;
            }
        }
    }

    if has_error {
        ExitCode::from(2)
    } else {
        ExitCode::from(0)
    }
}

fn run_check(files: &[PathBuf]) -> ExitCode {
    let mut has_error = false;
    let mut has_unformatted = false;

    for file in files {
        match check_file(file) {
            Ok(is_clean) => {
                if !is_clean {
                    eprintln!("{}: not formatted", file.display());
                    has_unformatted = true;
                }
            }
            Err(err) => {
                eprintln!("{}: {}", file.display(), err);
                has_error = true;
            }
        }
    }

    if has_error {
        ExitCode::from(2)
    } else if has_unformatted {
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}
