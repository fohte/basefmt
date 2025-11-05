use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn basefmt() -> Command {
    Command::new(env!("CARGO_BIN_EXE_basefmt"))
}

/// Helper function to recursively copy a directory tree
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
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

// ==============================================================================
// EditorConfig + exclude configuration integration tests
// ==============================================================================

/// Test that files controlled by EditorConfig and .basefmt.toml exclude patterns
/// are properly skipped during formatting
#[test]
fn test_editorconfig_and_exclude_integration() {
    let temp_dir = TempDir::new().unwrap();

    // Copy the entire config fixture directory to temp location
    let fixture_src = PathBuf::from("tests/fixtures/config");
    copy_dir_recursive(&fixture_src, temp_dir.path()).unwrap();

    // Run basefmt on the entire directory
    let status = basefmt()
        .arg(temp_dir.path().to_str().unwrap())
        .status()
        .unwrap();
    assert!(status.success());

    // Test 1: normal.txt should be formatted (no exclusions apply)
    let normal_content = fs::read_to_string(temp_dir.path().join("normal.txt")).unwrap();
    assert_eq!(
        normal_content,
        "normal file with trailing spaces\n",
        "normal.txt should have been formatted"
    );

    // Test 2: markdown.md should keep trailing spaces (EditorConfig: trim_trailing_whitespace = false)
    let md_content = fs::read_to_string(temp_dir.path().join("markdown.md")).unwrap();
    assert!(
        md_content.ends_with("  \n\n"),
        "markdown.md should preserve trailing spaces due to EditorConfig"
    );

    // Test 3: test/fixtures/data.txt should not be formatted (EditorConfig: unset)
    let test_fixture_content =
        fs::read_to_string(temp_dir.path().join("test/fixtures/data.txt")).unwrap();
    assert!(
        test_fixture_content.ends_with("  \n\n\n"),
        "test/fixtures/data.txt should not be formatted (EditorConfig unset)"
    );

    // Test 4: vendor/lib.js should not be formatted (EditorConfig: unset)
    let vendor_content = fs::read_to_string(temp_dir.path().join("vendor/lib.js")).unwrap();
    assert!(
        vendor_content.ends_with("  \n\n\n"),
        "vendor/lib.js should not be formatted (EditorConfig unset)"
    );

    // Test 5: generated/output.rs should not be formatted (.basefmt.toml exclude)
    let generated_content =
        fs::read_to_string(temp_dir.path().join("generated/output.rs")).unwrap();
    assert!(
        generated_content.ends_with("  \n\n\n"),
        "generated/output.rs should not be formatted (.basefmt.toml exclude)"
    );
}

/// Test that EditorConfig settings properly disable formatting rules
#[test]
fn test_editorconfig_disables_formatting() {
    let temp_dir = TempDir::new().unwrap();

    // Copy the config fixture
    let fixture_src = PathBuf::from("tests/fixtures/config");
    copy_dir_recursive(&fixture_src, temp_dir.path()).unwrap();

    // Test markdown file specifically
    let md_path = temp_dir.path().join("markdown.md");

    let status = basefmt().arg(md_path.to_str().unwrap()).status().unwrap();
    assert!(status.success());

    let formatted_content = fs::read_to_string(&md_path).unwrap();

    // Markdown should not have trailing spaces removed
    assert!(
        formatted_content.contains("trailing spaces  "),
        "Markdown file should preserve trailing spaces"
    );
}

/// Test that .basefmt.toml exclude has highest priority
#[test]
fn test_basefmt_exclude_overrides_editorconfig() {
    let temp_dir = TempDir::new().unwrap();

    // Copy the config fixture
    let fixture_src = PathBuf::from("tests/fixtures/config");
    copy_dir_recursive(&fixture_src, temp_dir.path()).unwrap();

    // generated/output.rs has EditorConfig settings enabled (through [*])
    // but should still be excluded by .basefmt.toml
    let generated_path = temp_dir.path().join("generated/output.rs");
    let original_content = fs::read_to_string(&generated_path).unwrap();

    let status = basefmt()
        .arg(temp_dir.path().to_str().unwrap())
        .status()
        .unwrap();
    assert!(status.success());

    let after_content = fs::read_to_string(&generated_path).unwrap();

    // Should not be formatted due to .basefmt.toml exclude
    assert_eq!(
        original_content, after_content,
        "generated/output.rs should not be formatted (excluded by .basefmt.toml)"
    );
}

/// Test check mode with EditorConfig and exclude patterns
#[test]
fn test_check_mode_with_config() {
    let temp_dir = TempDir::new().unwrap();

    // Copy the config fixture
    let fixture_src = PathBuf::from("tests/fixtures/config");
    copy_dir_recursive(&fixture_src, temp_dir.path()).unwrap();

    // Check mode should report that normal.txt needs formatting
    // but should not report excluded files as needing formatting
    let status = basefmt()
        .arg("--check")
        .arg(temp_dir.path().to_str().unwrap())
        .status()
        .unwrap();

    // Should fail because normal.txt needs formatting
    assert!(!status.success());

    // All files should remain unchanged
    let normal_content = fs::read_to_string(temp_dir.path().join("normal.txt")).unwrap();
    assert!(
        normal_content.ends_with("  \n\n\n"),
        "check mode should not modify files"
    );
}

/// Test EditorConfig with unset values properly disables formatting
#[test]
fn test_editorconfig_unset_disables_formatting() {
    let temp_dir = TempDir::new().unwrap();

    // Copy the config fixture
    let fixture_src = PathBuf::from("tests/fixtures/config");
    copy_dir_recursive(&fixture_src, temp_dir.path()).unwrap();

    let vendor_path = temp_dir.path().join("vendor/lib.js");
    let original_content = fs::read_to_string(&vendor_path).unwrap();

    let status = basefmt()
        .arg(temp_dir.path().to_str().unwrap())
        .status()
        .unwrap();
    assert!(status.success());

    let after_content = fs::read_to_string(&vendor_path).unwrap();

    // vendor/ has all formatting rules unset, so should not be formatted
    assert_eq!(
        original_content, after_content,
        "vendor/lib.js should not be formatted (EditorConfig unset)"
    );
}

/// Test that multiple exclusion rules work together correctly
#[test]
fn test_multiple_exclusion_patterns() {
    let temp_dir = TempDir::new().unwrap();

    // Copy the config fixture
    let fixture_src = PathBuf::from("tests/fixtures/config");
    copy_dir_recursive(&fixture_src, temp_dir.path()).unwrap();

    let status = basefmt()
        .arg(temp_dir.path().to_str().unwrap())
        .status()
        .unwrap();
    assert!(status.success());

    // Verify that different exclusion mechanisms work independently:
    // 1. EditorConfig pattern-based exclusion (test/fixtures/**)
    let test_fixture =
        fs::read_to_string(temp_dir.path().join("test/fixtures/data.txt")).unwrap();
    assert!(
        test_fixture.ends_with("  \n\n\n"),
        "test/fixtures/** excluded by EditorConfig"
    );

    // 2. EditorConfig file extension-based rule (*.md)
    let markdown = fs::read_to_string(temp_dir.path().join("markdown.md")).unwrap();
    assert!(
        markdown.contains("trailing spaces  "),
        "*.md excluded by EditorConfig"
    );

    // 3. .basefmt.toml exclude pattern (generated/**)
    let generated = fs::read_to_string(temp_dir.path().join("generated/output.rs")).unwrap();
    assert!(
        generated.ends_with("  \n\n\n"),
        "generated/** excluded by .basefmt.toml"
    );

    // 4. Normal files should be formatted
    let normal = fs::read_to_string(temp_dir.path().join("normal.txt")).unwrap();
    assert_eq!(normal, "normal file with trailing spaces\n");
}
