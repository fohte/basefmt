use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn basefmt() -> Command {
    Command::new(env!("CARGO_BIN_EXE_basefmt"))
}

fn setup_test_file(temp_dir: &TempDir, fixture_name: &str) -> PathBuf {
    let input_path = PathBuf::from("tests/fixtures/input").join(fixture_name);
    let temp_file = temp_dir.path().join(fixture_name);
    fs::copy(&input_path, &temp_file).unwrap();
    temp_file
}

fn read_expected(fixture_name: &str) -> String {
    let expected_path = PathBuf::from("tests/fixtures/expected").join(fixture_name);
    fs::read_to_string(expected_path).unwrap()
}

#[test]
fn test_format_single_files() {
    let test_cases = [
        "leading_newlines.txt",
        "no_final_newline.txt",
        "trailing_space.txt",
        "multiple_final_newlines.txt",
    ];

    for fixture_name in test_cases {
        let temp_dir = TempDir::new().unwrap();
        let test_file = setup_test_file(&temp_dir, fixture_name);

        let status = basefmt().arg(test_file.to_str().unwrap()).status().unwrap();
        assert!(status.success(), "Failed to format {fixture_name}");

        let actual = fs::read_to_string(&test_file).unwrap();
        let expected = read_expected(fixture_name);
        assert_eq!(
            actual, expected,
            "File {fixture_name} was not formatted correctly"
        );
    }
}

#[test]
fn test_format_directory() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_file(&temp_dir, "leading_newlines.txt");
    setup_test_file(&temp_dir, "no_final_newline.txt");
    setup_test_file(&temp_dir, "trailing_space.txt");
    setup_test_file(&temp_dir, "multiple_final_newlines.txt");

    let status = basefmt()
        .arg(temp_dir.path().to_str().unwrap())
        .status()
        .unwrap();
    assert!(status.success());

    // Verify all files were formatted correctly
    for fixture_name in [
        "leading_newlines.txt",
        "no_final_newline.txt",
        "trailing_space.txt",
        "multiple_final_newlines.txt",
    ] {
        let actual = fs::read_to_string(temp_dir.path().join(fixture_name)).unwrap();
        let expected = read_expected(fixture_name);
        assert_eq!(
            actual,
            expected,
            "File {fixture_name} was not formatted correctly"
        );
    }
}

#[test]
fn test_check_mode_clean_file() {
    let temp_dir = TempDir::new().unwrap();
    let expected_path = PathBuf::from("tests/fixtures/expected/leading_newlines.txt");
    let test_file = temp_dir.path().join("leading_newlines.txt");
    fs::copy(&expected_path, &test_file).unwrap();

    let status = basefmt()
        .arg("--check")
        .arg(test_file.to_str().unwrap())
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn test_check_mode_dirty_file() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = setup_test_file(&temp_dir, "leading_newlines.txt");
    let original_content = fs::read_to_string(&test_file).unwrap();

    let status = basefmt()
        .arg("--check")
        .arg(test_file.to_str().unwrap())
        .status()
        .unwrap();
    assert!(!status.success());

    // Verify file was not modified
    let after_check = fs::read_to_string(&test_file).unwrap();
    assert_eq!(original_content, after_check);
}

#[test]
fn test_format_skips_binary_file() {
    let temp_dir = TempDir::new().unwrap();
    let binary_file = temp_dir.path().join("binary.bin");
    // Write invalid UTF-8 bytes
    fs::write(&binary_file, &[0xFF, 0xFE, 0xFD]).unwrap();

    let status = basefmt()
        .arg(binary_file.to_str().unwrap())
        .status()
        .unwrap();
    // Binary files should be silently skipped, exit code 0
    assert!(status.success());
    assert_eq!(status.code(), Some(0));

    // Verify file was not modified
    let content = fs::read(&binary_file).unwrap();
    assert_eq!(content, vec![0xFF, 0xFE, 0xFD]);
}

#[test]
fn test_format_directory_with_binary_file() {
    let temp_dir = TempDir::new().unwrap();

    // Create a text file that needs formatting
    let text_file = temp_dir.path().join("text.txt");
    fs::write(&text_file, "\n\ntest content  \n\n").unwrap();

    // Create a binary file
    let binary_file = temp_dir.path().join("binary.bin");
    fs::write(&binary_file, &[0xFF, 0xFE, 0xFD]).unwrap();

    let status = basefmt()
        .arg(temp_dir.path().to_str().unwrap())
        .status()
        .unwrap();
    // Binary file should be skipped, text file formatted successfully, exit code 0
    assert!(status.success());
    assert_eq!(status.code(), Some(0));

    // Text file should be formatted correctly
    let text_content = fs::read_to_string(&text_file).unwrap();
    assert_eq!(text_content, "test content\n");

    // Binary file should not be modified
    let binary_content = fs::read(&binary_file).unwrap();
    assert_eq!(binary_content, vec![0xFF, 0xFE, 0xFD]);
}

#[test]
fn test_check_skips_binary_file() {
    let temp_dir = TempDir::new().unwrap();
    let binary_file = temp_dir.path().join("binary.bin");
    // Write invalid UTF-8 bytes
    fs::write(&binary_file, &[0xFF, 0xFE, 0xFD]).unwrap();

    let status = basefmt()
        .arg("--check")
        .arg(binary_file.to_str().unwrap())
        .status()
        .unwrap();
    // Binary files should be silently skipped, exit code 0
    assert!(status.success());
    assert_eq!(status.code(), Some(0));

    // Verify file was not modified
    let content = fs::read(&binary_file).unwrap();
    assert_eq!(content, vec![0xFF, 0xFE, 0xFD]);
}
