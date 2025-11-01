use std::fs;
use std::io;
use std::path::Path;

pub fn format_file(path: &Path) -> io::Result<bool> {
    let content = fs::read_to_string(path)?;
    let formatted = format_content(&content);

    let changed = content != formatted;
    if changed {
        fs::write(path, formatted)?;
    }

    Ok(changed)
}

pub fn check_file(path: &Path) -> io::Result<bool> {
    let content = fs::read_to_string(path)?;
    let formatted = format_content(&content);

    Ok(content == formatted)
}

fn format_content(content: &str) -> String {
    let content = remove_leading_newlines(content);
    let content = remove_trailing_spaces(&content);
    let content = ensure_final_newline(&content);
    content
}

fn remove_leading_newlines(content: &str) -> String {
    let mut lines: Vec<&str> = content.lines().collect();

    while let Some(first) = lines.first() {
        if first.is_empty() {
            lines.remove(0);
        } else {
            break;
        }
    }

    lines.join("\n")
}

fn remove_trailing_spaces(content: &str) -> String {
    content
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

fn ensure_final_newline(content: &str) -> String {
    let mut lines: Vec<&str> = content.lines().collect();

    // Remove trailing empty lines
    while let Some(last) = lines.last() {
        if last.is_empty() {
            lines.pop();
        } else {
            break;
        }
    }

    // Ensure exactly one final newline
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
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
}
