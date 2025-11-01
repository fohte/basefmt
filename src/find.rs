use ignore::Walk;
use std::io;
use std::path::Path;
use std::path::PathBuf;

/// Finds all files in the specified paths, respecting .gitignore patterns.
///
/// Recursively searches through directories and returns a list of all files found.
/// Hidden files and files specified in .gitignore are automatically excluded
/// by the `ignore` crate.
///
/// # Arguments
///
/// * `paths` - A slice of paths (files or directories) to search
///
/// # Returns
///
/// Returns `Ok(Vec<PathBuf>)` containing all files found, or an error if:
/// - Any path cannot be accessed or read
/// - Errors occurred during directory traversal
///
/// # Examples
///
/// ```no_run
/// use basefmt::find::find_files;
/// use std::path::Path;
///
/// let files = find_files(&[Path::new("src")]).unwrap();
/// println!("Found {} files", files.len());
/// ```
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
                                files.push(entry.into_path());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_find_single_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "content").unwrap();

        let result = find_files(&[&file_path]).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], file_path);
    }

    #[test]
    fn test_find_files_in_directory() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");
        fs::write(&file1, "content1").unwrap();
        fs::write(&file2, "content2").unwrap();

        let result = find_files(&[temp_dir.path()]).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.contains(&file1));
        assert!(result.contains(&file2));
    }

    #[test]
    fn test_find_files_in_nested_directory() {
        let temp_dir = TempDir::new().unwrap();
        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        let file1 = temp_dir.path().join("file1.txt");
        let file2 = subdir.join("file2.txt");
        fs::write(&file1, "content1").unwrap();
        fs::write(&file2, "content2").unwrap();

        let result = find_files(&[temp_dir.path()]).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.contains(&file1));
        assert!(result.contains(&file2));
    }

    #[test]
    fn test_find_files_from_multiple_paths() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let file1 = temp_dir1.path().join("file1.txt");
        let file2 = temp_dir2.path().join("file2.txt");
        fs::write(&file1, "content1").unwrap();
        fs::write(&file2, "content2").unwrap();

        let result = find_files(&[&file1, &file2]).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.contains(&file1));
        assert!(result.contains(&file2));
    }

    #[test]
    fn test_find_files_mixed_file_and_directory() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("file1.txt");
        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        let file2 = subdir.join("file2.txt");

        fs::write(&file1, "content1").unwrap();
        fs::write(&file2, "content2").unwrap();

        let result = find_files(&[&file1, &subdir]).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.contains(&file1));
        assert!(result.contains(&file2));
    }

    #[test]
    fn test_find_files_nonexistent_path() {
        let nonexistent = PathBuf::from("/nonexistent/path/file.txt");

        let result = find_files(&[&nonexistent]);

        assert!(result.is_err());
    }

    #[test]
    fn test_find_files_ignores_directories_in_results() {
        let temp_dir = TempDir::new().unwrap();
        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        let file = subdir.join("file.txt");
        fs::write(&file, "content").unwrap();

        let result = find_files(&[temp_dir.path()]).unwrap();

        // Should only include the file, not the directory itself
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], file);
    }

    #[test]
    fn test_find_files_empty_directory() {
        let temp_dir = TempDir::new().unwrap();

        let result = find_files(&[temp_dir.path()]).unwrap();

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_find_files_respects_gitignore() {
        let temp_dir = TempDir::new().unwrap();

        // Create .git directory to make ignore crate recognize .gitignore
        let git_dir = temp_dir.path().join(".git");
        fs::create_dir(&git_dir).unwrap();

        // Create a .gitignore file
        let gitignore_path = temp_dir.path().join(".gitignore");
        fs::write(&gitignore_path, "ignored.txt\n").unwrap();

        // Create both ignored and non-ignored files
        let ignored_file = temp_dir.path().join("ignored.txt");
        let normal_file = temp_dir.path().join("normal.txt");
        fs::write(&ignored_file, "ignored content").unwrap();
        fs::write(&normal_file, "normal content").unwrap();

        let result = find_files(&[temp_dir.path()]).unwrap();

        // The ignore crate automatically ignores .git directories and .gitignore files
        // So we should only see the normal.txt file
        assert_eq!(result.len(), 1, "Found files: {:?}", result);
        assert!(!result.contains(&ignored_file));
        assert!(result.contains(&normal_file));
    }
}

