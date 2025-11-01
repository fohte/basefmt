use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub fn format_file(path: &Path) -> io::Result<bool> {
    let metadata = fs::metadata(path)?;
    let content = fs::read_to_string(path)?;
    let formatted = format_content(&content);

    let changed = content != formatted;
    if changed {
        // Write to a temporary file first, then rename to preserve metadata
        let temp_path = path.with_extension("tmp");
        let mut temp_file = fs::File::create(&temp_path)?;
        temp_file.write_all(formatted.as_bytes())?;
        temp_file.sync_all()?;
        drop(temp_file);

        // Set permissions before renaming
        fs::set_permissions(&temp_path, metadata.permissions())?;

        // Atomically replace the original file
        fs::rename(&temp_path, path)?;
    }

    Ok(changed)
}

pub fn check_file(path: &Path) -> io::Result<bool> {
    let content = fs::read_to_string(path)?;
    let formatted = format_content(&content);

    Ok(content == formatted)
}

fn format_content(content: &str) -> String {
    let lines: Vec<&str> = content
        .lines()
        .skip_while(|line| line.is_empty())
        .collect();

    // Find the last non-empty line
    let end = lines
        .iter()
        .rposition(|line| !line.is_empty())
        .map(|pos| pos + 1)
        .unwrap_or(0);

    if end == 0 {
        return String::new();
    }

    // Build result with capacity hint to avoid reallocations
    let mut result = String::with_capacity(content.len());
    for (i, line) in lines[..end].iter().enumerate() {
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
}
