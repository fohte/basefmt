use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use tempfile::NamedTempFile;

/// Result of a format operation.
#[derive(Debug, PartialEq, Eq)]
pub enum FormatResult {
    /// File was modified
    Changed,
    /// File was already properly formatted
    Unchanged,
    /// File was skipped (e.g., binary file)
    Skipped,
}

/// Result of a check operation.
#[derive(Debug, PartialEq, Eq)]
pub enum CheckResult {
    /// File is properly formatted
    Formatted,
    /// File needs formatting
    NeedsFormatting,
    /// File was skipped (e.g., binary file)
    Skipped,
}

fn read_and_format(path: &Path) -> io::Result<Option<(String, String, fs::Metadata)>> {
    let file = fs::File::open(path)?;
    let metadata = file.metadata()?;

    let mut content = String::new();
    let mut reader = io::BufReader::new(file);
    match reader.read_to_string(&mut content) {
        Ok(_) => {
            let formatted = format_content(&content);
            Ok(Some((content, formatted, metadata)))
        }
        Err(err) if err.kind() == io::ErrorKind::InvalidData => {
            // Skip binary files silently
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

/// Formats a file in place, preserving file permissions and metadata.
///
/// Applies formatting rules including:
/// - Removing leading newlines
/// - Removing trailing spaces from each line
/// - Ensuring exactly one final newline
///
/// Binary files (files containing invalid UTF-8) are silently skipped and
/// treated as if they don't need formatting.
///
/// The file is only modified if formatting changes are needed. File permissions
/// and other metadata are preserved through atomic write-and-rename operation.
///
/// # Arguments
///
/// * `path` - Path to the file to format
///
/// # Returns
///
/// Returns:
/// - `Ok(FormatResult::Changed)` if the file was modified
/// - `Ok(FormatResult::Unchanged)` if no changes were needed
/// - `Ok(FormatResult::Skipped)` if the file is binary
/// - `Err(...)` if the file cannot be read or written
///
/// # Examples
///
/// ```no_run
/// use basefmt::format::{format_file, FormatResult};
/// use std::path::Path;
///
/// match format_file(Path::new("file.txt")).unwrap() {
///     FormatResult::Changed => println!("File was formatted"),
///     FormatResult::Unchanged => println!("File was already formatted"),
///     FormatResult::Skipped => println!("File was skipped"),
/// }
/// ```
pub fn format_file(path: &Path) -> io::Result<FormatResult> {
    if let Some((content, formatted, metadata)) = read_and_format(path)? {
        let changed = content != formatted;
        if changed {
            // Write to a temporary file first, then rename to preserve metadata
            let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));
            let mut temp_file = NamedTempFile::new_in(parent_dir)?;
            temp_file.write_all(formatted.as_bytes())?;
            temp_file.as_file().sync_all()?;

            // Set permissions before persisting
            temp_file.as_file().set_permissions(metadata.permissions())?;

            // Atomically replace the original file
            temp_file.persist(path)?;

            Ok(FormatResult::Changed)
        } else {
            Ok(FormatResult::Unchanged)
        }
    } else {
        // Binary file, skip silently
        Ok(FormatResult::Skipped)
    }
}

/// Checks if a file is properly formatted without modifying it.
///
/// Binary files (files containing invalid UTF-8) are silently skipped.
///
/// # Arguments
///
/// * `path` - Path to the file to check
///
/// # Returns
///
/// Returns:
/// - `Ok(CheckResult::Formatted)` if the file is properly formatted
/// - `Ok(CheckResult::NeedsFormatting)` if formatting is needed
/// - `Ok(CheckResult::Skipped)` if the file is binary
/// - `Err(...)` if the file cannot be read
///
/// # Examples
///
/// ```no_run
/// use basefmt::format::{check_file, CheckResult};
/// use std::path::Path;
///
/// match check_file(Path::new("file.txt")).unwrap() {
///     CheckResult::Formatted => println!("File is properly formatted"),
///     CheckResult::NeedsFormatting => println!("File needs formatting"),
///     CheckResult::Skipped => println!("File was skipped"),
/// }
/// ```
pub fn check_file(path: &Path) -> io::Result<CheckResult> {
    if let Some((content, formatted, _metadata)) = read_and_format(path)? {
        if content == formatted {
            Ok(CheckResult::Formatted)
        } else {
            Ok(CheckResult::NeedsFormatting)
        }
    } else {
        // Binary file, skip silently
        Ok(CheckResult::Skipped)
    }
}

fn format_content(content: &str) -> String {
    // Collect lines while skipping leading empty lines
    let mut lines: Vec<&str> = content
        .lines()
        .skip_while(|line| line.is_empty())
        .collect();

    // Remove trailing empty lines
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }

    if lines.is_empty() {
        return String::new();
    }

    // Build result with capacity hint to avoid reallocations
    let mut result = String::with_capacity(content.len());
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            result.push('\n');
        }
        result.push_str(line.trim_end());
    }
    result.push('\n');
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_format_content_removes_leading_newlines() {
        let input = "\n\nfirst line\nsecond line\n";
        let expected = "first line\nsecond line\n";
        assert_eq!(format_content(input), expected);
    }

    #[test]
    fn test_format_content_removes_trailing_spaces() {
        let input = "line with trailing spaces   \nanother line with spaces  \n";
        let expected = "line with trailing spaces\nanother line with spaces\n";
        assert_eq!(format_content(input), expected);
    }

    #[test]
    fn test_format_content_adds_final_newline() {
        let input = "first line\nsecond line";
        let expected = "first line\nsecond line\n";
        assert_eq!(format_content(input), expected);
    }

    #[test]
    fn test_format_content_removes_multiple_final_newlines() {
        let input = "first line\nsecond line\n\n\n";
        let expected = "first line\nsecond line\n";
        assert_eq!(format_content(input), expected);
    }

    #[test]
    fn test_format_content_empty_file() {
        let input = "";
        let expected = "";
        assert_eq!(format_content(input), expected);
    }

    #[test]
    fn test_format_content_only_newlines() {
        let input = "\n\n\n";
        let expected = "";
        assert_eq!(format_content(input), expected);
    }

    #[test]
    fn test_format_file_creates_changes() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "\n\ntest content  \n\n").unwrap();

        let result = format_file(&file_path).unwrap();

        assert_eq!(result, FormatResult::Changed);
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "test content\n");
    }

    #[test]
    fn test_format_file_no_changes() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test content\n").unwrap();

        let result = format_file(&file_path).unwrap();

        assert_eq!(result, FormatResult::Unchanged);
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "test content\n");
    }

    #[test]
    fn test_check_file_clean() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test content\n").unwrap();

        let result = check_file(&file_path).unwrap();

        assert_eq!(result, CheckResult::Formatted);
    }

    #[test]
    fn test_check_file_dirty() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "\n\ntest content  \n\n").unwrap();

        let result = check_file(&file_path).unwrap();

        assert_eq!(result, CheckResult::NeedsFormatting);
    }

    #[test]
    #[cfg(unix)]
    fn test_format_file_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "\n\ntest content  \n\n").unwrap();

        // Set specific permissions (e.g., 0o644)
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&file_path, perms).unwrap();

        let original_mode = fs::metadata(&file_path).unwrap().permissions().mode();

        // Format the file
        format_file(&file_path).unwrap();

        // Check permissions are preserved
        let new_mode = fs::metadata(&file_path).unwrap().permissions().mode();
        assert_eq!(original_mode, new_mode);
    }

    #[test]
    fn test_format_file_skips_binary() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("binary.bin");
        // Write invalid UTF-8 bytes
        fs::write(&file_path, &[0xFF, 0xFE, 0xFD]).unwrap();

        let result = format_file(&file_path).unwrap();

        // Binary files should be skipped silently
        assert_eq!(result, FormatResult::Skipped);

        // Verify file was not modified
        let content = fs::read(&file_path).unwrap();
        assert_eq!(content, vec![0xFF, 0xFE, 0xFD]);
    }

    #[test]
    fn test_check_file_skips_binary() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("binary.bin");
        // Write invalid UTF-8 bytes
        fs::write(&file_path, &[0xFF, 0xFE, 0xFD]).unwrap();

        let result = check_file(&file_path).unwrap();

        // Binary files should be skipped silently
        assert_eq!(result, CheckResult::Skipped);
    }
}
