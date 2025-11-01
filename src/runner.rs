use crate::find::find_files;
use crate::format::{check_file, format_file};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Result of a formatting or checking operation.
pub struct FormatResult {
    /// Total number of files processed
    pub total_files: usize,
    /// Number of files that encountered errors
    pub error_count: usize,
    /// Number of files that were not properly formatted (check mode only)
    pub unformatted_count: usize,
}

impl FormatResult {
    /// Returns the appropriate exit code based on the result.
    ///
    /// Exit codes:
    /// - 0: Success (all files formatted/checked successfully)
    /// - 1: Some files need formatting (check mode only)
    /// - 2: Errors occurred during processing
    pub fn exit_code(&self) -> u8 {
        if self.error_count > 0 {
            2
        } else if self.unformatted_count > 0 {
            1
        } else {
            0
        }
    }
}

/// Formats files in the specified paths in parallel.
///
/// Finds all files in the given paths and formats them concurrently using rayon.
/// Files are only modified if formatting changes are needed.
///
/// # Arguments
///
/// * `paths` - A slice of paths (files or directories) to format
///
/// # Returns
///
/// Returns a `FormatResult` containing statistics about the operation, or an error
/// if file discovery fails.
///
/// # Examples
///
/// ```no_run
/// use basefmt::runner::run_format;
/// use std::path::Path;
///
/// let result = run_format(&[Path::new("src")]).unwrap();
/// println!("Formatted {} files", result.total_files);
/// ```
pub fn run_format(paths: &[impl AsRef<Path>]) -> io::Result<FormatResult> {
    let files = find_files(paths)?;
    let error_count = AtomicUsize::new(0);

    files.par_iter().for_each(|file| {
        match format_file(file) {
            Ok(_changed) => {
                // Successfully formatted
            }
            Err(err) => {
                eprintln!("{}: {}", file.display(), err);
                error_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    Ok(FormatResult {
        total_files: files.len(),
        error_count: error_count.load(Ordering::Relaxed),
        unformatted_count: 0,
    })
}

/// Checks if files in the specified paths are properly formatted, in parallel.
///
/// Finds all files in the given paths and checks them concurrently using rayon.
/// Files are not modified; only checked for proper formatting.
///
/// # Arguments
///
/// * `paths` - A slice of paths (files or directories) to check
///
/// # Returns
///
/// Returns a `FormatResult` containing statistics about the operation, including
/// the number of files that need formatting, or an error if file discovery fails.
///
/// # Examples
///
/// ```no_run
/// use basefmt::runner::run_check;
/// use std::path::Path;
///
/// let result = run_check(&[Path::new("src")]).unwrap();
/// if result.unformatted_count > 0 {
///     println!("{} files need formatting", result.unformatted_count);
/// }
/// ```
pub fn run_check(paths: &[impl AsRef<Path>]) -> io::Result<FormatResult> {
    let files = find_files(paths)?;
    let error_count = AtomicUsize::new(0);
    let unformatted_count = AtomicUsize::new(0);

    files.par_iter().for_each(|file| {
        match check_file(file) {
            Ok(is_clean) => {
                if !is_clean {
                    eprintln!("{}: not formatted", file.display());
                    unformatted_count.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(err) => {
                eprintln!("{}: {}", file.display(), err);
                error_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    Ok(FormatResult {
        total_files: files.len(),
        error_count: error_count.load(Ordering::Relaxed),
        unformatted_count: unformatted_count.load(Ordering::Relaxed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_format_result_exit_code_success() {
        let result = FormatResult {
            total_files: 5,
            error_count: 0,
            unformatted_count: 0,
        };
        assert_eq!(result.exit_code(), 0);
    }

    #[test]
    fn test_format_result_exit_code_unformatted() {
        let result = FormatResult {
            total_files: 5,
            error_count: 0,
            unformatted_count: 2,
        };
        assert_eq!(result.exit_code(), 1);
    }

    #[test]
    fn test_format_result_exit_code_error() {
        let result = FormatResult {
            total_files: 5,
            error_count: 1,
            unformatted_count: 0,
        };
        assert_eq!(result.exit_code(), 2);
    }

    #[test]
    fn test_format_result_exit_code_error_priority() {
        let result = FormatResult {
            total_files: 5,
            error_count: 1,
            unformatted_count: 2,
        };
        // Errors have higher priority than unformatted
        assert_eq!(result.exit_code(), 2);
    }

    #[test]
    fn test_run_format_single_file() {
        let temp_dir = TempDir::new().unwrap();
        let file = temp_dir.path().join("test.txt");
        fs::write(&file, "\n\ntest content  \n\n").unwrap();

        let result = run_format(&[&file]).unwrap();

        assert_eq!(result.total_files, 1);
        assert_eq!(result.error_count, 0);
        assert_eq!(result.unformatted_count, 0);
        assert_eq!(result.exit_code(), 0);

        let content = fs::read_to_string(&file).unwrap();
        assert_eq!(content, "test content\n");
    }

    #[test]
    fn test_run_format_multiple_files() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");
        fs::write(&file1, "\n\ntest1  \n").unwrap();
        fs::write(&file2, "test2\n").unwrap();

        let result = run_format(&[temp_dir.path()]).unwrap();

        assert_eq!(result.total_files, 2);
        assert_eq!(result.error_count, 0);
        assert_eq!(result.exit_code(), 0);

        assert_eq!(fs::read_to_string(&file1).unwrap(), "test1\n");
        assert_eq!(fs::read_to_string(&file2).unwrap(), "test2\n");
    }

    #[test]
    fn test_run_format_directory() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");
        fs::write(&file1, "\n\ntest1\n").unwrap();
        fs::write(&file2, "test2  \n").unwrap();

        let result = run_format(&[temp_dir.path()]).unwrap();

        assert_eq!(result.total_files, 2);
        assert_eq!(result.error_count, 0);
        assert_eq!(result.exit_code(), 0);
    }

    #[test]
    fn test_run_format_nonexistent_path() {
        let result = run_format(&["/nonexistent/path"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_check_clean_files() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");
        fs::write(&file1, "test1\n").unwrap();
        fs::write(&file2, "test2\n").unwrap();

        let result = run_check(&[temp_dir.path()]).unwrap();

        assert_eq!(result.total_files, 2);
        assert_eq!(result.error_count, 0);
        assert_eq!(result.unformatted_count, 0);
        assert_eq!(result.exit_code(), 0);
    }

    #[test]
    fn test_run_check_unformatted_files() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");
        fs::write(&file1, "\n\ntest1\n").unwrap();
        fs::write(&file2, "test2  \n").unwrap();

        let result = run_check(&[temp_dir.path()]).unwrap();

        assert_eq!(result.total_files, 2);
        assert_eq!(result.error_count, 0);
        assert_eq!(result.unformatted_count, 2);
        assert_eq!(result.exit_code(), 1);
    }

    #[test]
    fn test_run_check_mixed_files() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");
        fs::write(&file1, "test1\n").unwrap();
        fs::write(&file2, "\n\ntest2\n").unwrap();

        let result = run_check(&[temp_dir.path()]).unwrap();

        assert_eq!(result.total_files, 2);
        assert_eq!(result.error_count, 0);
        assert_eq!(result.unformatted_count, 1);
        assert_eq!(result.exit_code(), 1);
    }

    #[test]
    fn test_run_check_nonexistent_path() {
        let result = run_check(&["/nonexistent/path"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_check_does_not_modify_files() {
        let temp_dir = TempDir::new().unwrap();
        let file = temp_dir.path().join("test.txt");
        let original = "\n\ntest content  \n\n";
        fs::write(&file, original).unwrap();

        let _result = run_check(&[&file]).unwrap();

        let content = fs::read_to_string(&file).unwrap();
        assert_eq!(content, original);
    }
}
