use crate::config::Config;
use crate::editorconfig::{EditorConfigCache, FormatRules};
use crate::find::find_files;
use crate::format::{CheckResult, check_file_with_rules, format_file_with_rules};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Result of a formatting or checking operation on multiple files.
pub struct RunnerResult {
    /// Total number of files processed
    pub total_files: usize,
    /// Number of files that encountered errors
    pub error_count: usize,
    /// Number of files that were not properly formatted (check mode only)
    pub unformatted_count: usize,
}

impl RunnerResult {
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

/// A file that needs to be formatted along with its formatting rules.
///
/// This structure pre-computes and caches the formatting rules for each file
/// to avoid redundant EditorConfig lookups during parallel processing.
struct FileTask {
    /// Original path to the file (may be relative or absolute)
    path: PathBuf,
    /// Cached formatting rules from EditorConfig
    rules: FormatRules,
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
/// Returns a `RunnerResult` containing statistics about the operation, or an error
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
pub fn run_format(paths: &[impl AsRef<Path>]) -> io::Result<RunnerResult> {
    let config_dir = determine_config_dir(paths);
    let config = Config::load(config_dir).unwrap_or_default();
    let files = find_files(paths)?;
    let config_dir_abs = config_dir
        .canonicalize()
        .unwrap_or_else(|_| config_dir.to_path_buf());

    let mut rule_cache = EditorConfigCache::new();
    let filtered_files = collect_tasks(files, &config, &config_dir_abs, &mut rule_cache);

    let error_count = AtomicUsize::new(0);

    // Use parallel processing only for larger file counts to avoid overhead
    const PARALLEL_THRESHOLD: usize = 10;

    if filtered_files.len() < PARALLEL_THRESHOLD {
        for task in &filtered_files {
            if let Err(err) = format_file_with_rules(&task.path, &task.rules) {
                eprintln!("{}: {}", task.path.display(), err);
                error_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    } else {
        filtered_files.par_iter().for_each(|task| {
            if let Err(err) = format_file_with_rules(&task.path, &task.rules) {
                eprintln!("{}: {}", task.path.display(), err);
                error_count.fetch_add(1, Ordering::Relaxed);
            }
        });
    }

    Ok(RunnerResult {
        total_files: filtered_files.len(),
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
/// Returns a `RunnerResult` containing statistics about the operation, including
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
pub fn run_check(paths: &[impl AsRef<Path>]) -> io::Result<RunnerResult> {
    let config_dir = determine_config_dir(paths);
    let config = Config::load(config_dir).unwrap_or_default();
    let files = find_files(paths)?;
    let config_dir_abs = config_dir
        .canonicalize()
        .unwrap_or_else(|_| config_dir.to_path_buf());

    let mut rule_cache = EditorConfigCache::new();
    let filtered_files = collect_tasks(files, &config, &config_dir_abs, &mut rule_cache);

    let error_count = AtomicUsize::new(0);
    let unformatted_count = AtomicUsize::new(0);

    // Use parallel processing only for larger file counts to avoid overhead
    const PARALLEL_THRESHOLD: usize = 10;

    if filtered_files.len() < PARALLEL_THRESHOLD {
        for task in &filtered_files {
            match check_file_with_rules(&task.path, &task.rules) {
                Ok(CheckResult::Formatted | CheckResult::Skipped) => {}
                Ok(CheckResult::NeedsFormatting) => {
                    eprintln!("{}: not formatted", task.path.display());
                    unformatted_count.fetch_add(1, Ordering::Relaxed);
                }
                Err(err) => {
                    eprintln!("{}: {}", task.path.display(), err);
                    error_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    } else {
        filtered_files.par_iter().for_each(|task| {
            match check_file_with_rules(&task.path, &task.rules) {
                Ok(CheckResult::Formatted | CheckResult::Skipped) => {}
                Ok(CheckResult::NeedsFormatting) => {
                    eprintln!("{}: not formatted", task.path.display());
                    unformatted_count.fetch_add(1, Ordering::Relaxed);
                }
                Err(err) => {
                    eprintln!("{}: {}", task.path.display(), err);
                    error_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }

    Ok(RunnerResult {
        total_files: filtered_files.len(),
        error_count: error_count.load(Ordering::Relaxed),
        unformatted_count: unformatted_count.load(Ordering::Relaxed),
    })
}

fn determine_config_dir(paths: &[impl AsRef<Path>]) -> &Path {
    if let Some(first_path) = paths.first() {
        let path = first_path.as_ref();
        if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or_else(|| Path::new("."))
        }
    } else {
        Path::new(".")
    }
}

fn collect_tasks(
    files: Vec<PathBuf>,
    config: &Config,
    config_dir_abs: &Path,
    rule_cache: &mut EditorConfigCache,
) -> Vec<FileTask> {
    let mut tasks = Vec::with_capacity(files.len());
    for path in files {
        let canonical = match path.canonicalize() {
            Ok(abs) => abs,
            Err(err) => {
                eprintln!("{}: failed to canonicalize: {}", path.display(), err);
                continue;
            }
        };

        let rel_path = canonical
            .strip_prefix(config_dir_abs)
            .unwrap_or(canonical.as_path());

        if config.is_excluded(rel_path) {
            continue;
        }

        let rules = rule_cache.rules_for(&canonical);
        tasks.push(FileTask { path, rules });
    }
    tasks
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // Helper function to create a .editorconfig file with all rules enabled
    fn create_default_editorconfig(dir: &TempDir) {
        let config_path = dir.path().join(".editorconfig");
        fs::write(
            config_path,
            r#"
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = true
trim_leading_newlines = true
"#,
        )
        .unwrap();
    }

    #[test]
    fn test_runner_result_exit_code_success() {
        let result = RunnerResult {
            total_files: 5,
            error_count: 0,
            unformatted_count: 0,
        };
        assert_eq!(result.exit_code(), 0);
    }

    #[test]
    fn test_runner_result_exit_code_unformatted() {
        let result = RunnerResult {
            total_files: 5,
            error_count: 0,
            unformatted_count: 2,
        };
        assert_eq!(result.exit_code(), 1);
    }

    #[test]
    fn test_runner_result_exit_code_error() {
        let result = RunnerResult {
            total_files: 5,
            error_count: 1,
            unformatted_count: 0,
        };
        assert_eq!(result.exit_code(), 2);
    }

    #[test]
    fn test_runner_result_exit_code_error_priority() {
        let result = RunnerResult {
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
        create_default_editorconfig(&temp_dir);
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
        create_default_editorconfig(&temp_dir);
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
        create_default_editorconfig(&temp_dir);
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
        create_default_editorconfig(&temp_dir);
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
        create_default_editorconfig(&temp_dir);
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
