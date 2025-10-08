use ignore::Walk;
use std::io;
use std::path::Path;
use std::path::PathBuf;

pub fn find_files(paths: &[impl AsRef<Path>]) -> io::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut has_error = false;

    for path in paths {
        let path = path.as_ref();

        match path.metadata() {
            Ok(_) => {
                for result in Walk::new(path) {
                    match result {
                        Ok(entry) => {
                            if entry.file_type().is_some_and(|ft| ft.is_file()) {
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
        Err(io::Error::other(
            "some files had errors",
        ))
    } else {
        Ok(files)
    }
}
