use basefmt::find::find_files;
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

    if args.check {
        println!("Check mode engaged. No changes will be made.");
    }

    match find_files(&args.paths) {
        Ok(files) => {
            println!("Processing the following files: {files:?}");
            ExitCode::from(0)
        }
        Err(_) => ExitCode::from(2),
    }
}
