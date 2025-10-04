use clap::Parser;
use ignore::Walk;

#[derive(Parser)]
struct Args {
    #[clap(default_value = ".", help = "List of files/directories to format")]
    paths: Vec<String>,

    #[clap(short, long, help = "Check mode (don't write changes)")]
    check: bool,
}

fn find_files(paths: &[String]) -> Vec<String> {
    let mut files = Vec::new();
    for path in paths {
        if std::path::Path::new(path).is_file() {
            files.push(path.clone());
            continue;
        }

        for result in Walk::new(path) {
            match result {
                Ok(entry) => {
                    files.push(entry.path().display().to_string());
                }
                Err(err) => eprintln!("Error reading entry: {}", err),
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
