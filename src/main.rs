use basefmt::runner::{run_check, run_format};
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

    let result = if args.check {
        run_check(&args.paths)
    } else {
        run_format(&args.paths)
    };

    match result {
        Ok(result) => ExitCode::from(result.exit_code() as u8),
        Err(_) => ExitCode::from(2),
    }
}
