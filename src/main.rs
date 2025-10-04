use clap::Parser;

#[derive(Parser)]
struct Args {
    #[clap(default_value = ".", help = "List of files/directories to format")]
    paths: Vec<String>,

    #[clap(short, long, help = "Check mode (don't write changes)")]
    check: bool,
}

fn find_files(paths: &[String]) -> Vec<String> {
    paths.to_vec()
}

fn main() {
    let args = Args::parse();
    if args.check {
        println!("Check mode engaged. No changes will be made.");
    }

    let files = find_files(&args.paths);

    println!("Processing the following files: {:?}", files);
}
