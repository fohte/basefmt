use std::fs;
use std::io::{self, Write};
use std::path::Path;
use tempfile::NamedTempFile;

fn read_and_format(path: &Path) -> io::Result<(String, String)> {
    let content = fs::read_to_string(path).map_err(|err| {
        if err.kind() == io::ErrorKind::InvalidData {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("file contains invalid UTF-8: {err}"),
            )
        } else {
            err
        }
    })?;
    let formatted = format_content(&content);
    Ok((content, formatted))
}

/// Formats a file in place, preserving file permissions and metadata.
///
/// Applies formatting rules including:
/// - Removing leading newlines
/// - Removing trailing spaces from each line
/// - Ensuring exactly one final newline
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
/// Returns `Ok(true)` if the file was modified, `Ok(false)` if no changes were needed,
/// or an error if the file cannot be read or written.
///
/// # Examples
///
/// ```no_run
/// use basefmt::format::format_file;
/// use std::path::Path;
///
/// let changed = format_file(Path::new("file.txt")).unwrap();
/// if changed {
///     println!("File was formatted");
/// }
/// ```
pub fn format_file(path: &Path) -> io::Result<bool> {
    let metadata = fs::metadata(path)?;
    let (content, formatted) = read_and_format(path)?;

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
    }

    Ok(changed)
}

/// Checks if a file is properly formatted without modifying it.
///
/// Returns `true` if the file is already properly formatted, `false` if it needs formatting.
///
/// # Arguments
///
/// * `path` - Path to the file to check
///
/// # Returns
///
/// Returns `Ok(true)` if the file is properly formatted, `Ok(false)` if formatting is needed,
/// or an error if the file cannot be read.
///
/// # Examples
///
/// ```no_run
/// use basefmt::format::check_file;
/// use std::path::Path;
///
/// let is_formatted = check_file(Path::new("file.txt")).unwrap();
/// if !is_formatted {
///     println!("File needs formatting");
/// }
/// ```
pub fn check_file(path: &Path) -> io::Result<bool> {
    let (content, formatted) = read_and_format(path)?;
    Ok(content == formatted)
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

        let changed = format_file(&file_path).unwrap();

        assert!(changed);
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "test content\n");
    }

    #[test]
    fn test_format_file_no_changes() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test content\n").unwrap();

        let changed = format_file(&file_path).unwrap();

        assert!(!changed);
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "test content\n");
    }

    #[test]
    fn test_check_file_clean() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test content\n").unwrap();

        let is_clean = check_file(&file_path).unwrap();

        assert!(is_clean);
    }

    #[test]
    fn test_check_file_dirty() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "\n\ntest content  \n\n").unwrap();

        let is_clean = check_file(&file_path).unwrap();

        assert!(!is_clean);
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
    fn test_format_file_rejects_binary() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("binary.bin");
        // Write invalid UTF-8 bytes
        fs::write(&file_path, &[0xFF, 0xFE, 0xFD]).unwrap();

        let result = format_file(&file_path);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("invalid UTF-8"));
    }

    #[test]
    fn test_check_file_rejects_binary() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("binary.bin");
        // Write invalid UTF-8 bytes
        fs::write(&file_path, &[0xFF, 0xFE, 0xFD]).unwrap();

        let result = check_file(&file_path);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("invalid UTF-8"));
    }
}
