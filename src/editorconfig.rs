// EditorConfig integration module
// This module is responsible for reading EditorConfig files and mapping properties
// to basefmt's formatting rules.

use ec4rs::property::{FinalNewline, TrimTrailingWs};
use ec4rs::{ConfigFile, Properties, PropertiesSource, Section};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Configuration rules for formatting a file
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormatRules {
    /// Whether to ensure the file ends with a newline
    pub ensure_final_newline: bool,
    /// Whether to remove trailing spaces from each line
    pub remove_trailing_spaces: bool,
    /// Whether to remove leading newlines from the file
    pub remove_leading_newlines: bool,
}

/// Get formatting rules for a file from EditorConfig
///
/// This function reads the EditorConfig file for the given path and returns
/// the corresponding formatting rules.
///
/// # EditorConfig Property Mapping
///
/// - `insert_final_newline` → `ensure_final_newline`
/// - `trim_trailing_whitespace` → `remove_trailing_spaces`
/// - `trim_leading_newlines` (custom) → `remove_leading_newlines`
///
/// # Property Value Interpretation
///
/// - `true` → rule enabled
/// - `false` → rule disabled
/// - `unset` → rule disabled
/// - unset → rule disabled (default)
pub fn get_format_rules(path: &Path) -> FormatRules {
    match path.canonicalize() {
        Ok(resolved) => {
            let mut cache = EditorConfigCache::new();
            cache.rules_for(&resolved)
        }
        Err(_) => FormatRules::default(),
    }
}

/// Caches parsed EditorConfig files to avoid redundant IO on large projects.
#[derive(Default)]
pub struct EditorConfigCache {
    dir_stacks: HashMap<PathBuf, Arc<Vec<Arc<ParsedConfig>>>>,
    config_files: HashMap<PathBuf, Option<Arc<ParsedConfig>>>,
    rules_cache: HashMap<PathBuf, FormatRules>,
}

impl EditorConfigCache {
    /// Creates an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns formatting rules for the given canonical path, caching repeated lookups.
    pub fn rules_for(&mut self, canonical_path: &Path) -> FormatRules {
        if let Some(rules) = self.rules_cache.get(canonical_path) {
            return rules.clone();
        }

        let mut properties = Properties::new();
        if let Some(parent) = canonical_path.parent() {
            for config in self.stack_for_dir(parent).iter() {
                config.apply_to(&mut properties, canonical_path);
            }
        }
        let rules = rules_from_properties(&properties);
        self.rules_cache
            .insert(canonical_path.to_path_buf(), rules.clone());
        rules
    }

    fn stack_for_dir(&mut self, dir: &Path) -> Arc<Vec<Arc<ParsedConfig>>> {
        if let Some(stack) = self.dir_stacks.get(dir) {
            return Arc::clone(stack);
        }

        let mut combined = if let Some(parent) = dir.parent() {
            self.stack_for_dir(parent).as_ref().clone()
        } else {
            Vec::new()
        };

        if let Some(config) = self.load_config_for_dir(dir) {
            if config.is_root {
                combined.clear();
            }
            combined.push(config);
        }

        let stack = Arc::new(combined);
        self.dir_stacks
            .insert(dir.to_path_buf(), Arc::clone(&stack));
        stack
    }

    fn load_config_for_dir(&mut self, dir: &Path) -> Option<Arc<ParsedConfig>> {
        if let Some(entry) = self.config_files.get(dir) {
            return entry.clone();
        }

        let config_path = dir.join(".editorconfig");
        let parsed = match ConfigFile::open(&config_path) {
            Ok(file) => self.parse_config_file(dir, file),
            Err(ec4rs::ParseError::Io(err)) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => {
                eprintln!(
                    "{}: failed to read .editorconfig: {}",
                    config_path.display(),
                    err
                );
                None
            }
        };

        self.config_files.insert(dir.to_path_buf(), parsed.clone());
        parsed
    }

    fn parse_config_file(&self, dir: &Path, file: ConfigFile) -> Option<Arc<ParsedConfig>> {
        let ConfigFile { path, mut reader } = file;
        let mut sections = Vec::new();
        while let Some(section_result) = reader.next() {
            match section_result {
                Ok(section) => sections.push(section),
                Err(err) => {
                    eprintln!(
                        "{}:{}: failed to parse .editorconfig: {}",
                        path.display(),
                        reader.line_no(),
                        err
                    );
                    return None;
                }
            }
        }

        Some(Arc::new(ParsedConfig {
            dir: dir.to_path_buf(),
            is_root: reader.is_root,
            sections: Arc::new(sections),
        }))
    }
}

#[derive(Clone)]
struct ParsedConfig {
    dir: PathBuf,
    is_root: bool,
    sections: Arc<Vec<Section>>,
}

impl ParsedConfig {
    fn apply_to(&self, props: &mut Properties, file_path: &Path) {
        let rel_path = file_path.strip_prefix(&self.dir).unwrap_or(file_path);
        for section in self.sections.as_ref() {
            let _ = section.apply_to(props, rel_path);
        }
    }
}

fn rules_from_properties(properties: &Properties) -> FormatRules {
    let parse_bool_value = |prop: &str| -> bool {
        match prop.to_lowercase().as_str() {
            "true" => true,
            "false" => false,
            _ => false,
        }
    };

    let ensure_final_newline = properties
        .get::<FinalNewline>()
        .ok()
        .map(|prop| matches!(prop, FinalNewline::Value(true)))
        .unwrap_or(false);

    let remove_trailing_spaces = properties
        .get::<TrimTrailingWs>()
        .ok()
        .map(|prop| matches!(prop, TrimTrailingWs::Value(true)))
        .unwrap_or(false);

    let remove_leading_newlines = properties
        .get_raw_for_key("trim_leading_newlines")
        .into_option()
        .map(parse_bool_value)
        .unwrap_or(false);

    FormatRules {
        ensure_final_newline,
        remove_trailing_spaces,
        remove_leading_newlines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a temporary .editorconfig file
    fn create_editorconfig(temp_dir: &TempDir, content: &str) -> std::path::PathBuf {
        let config_path = temp_dir.path().join(".editorconfig");
        fs::write(&config_path, content).unwrap();
        config_path
    }

    #[test]
    fn test_basic_properties_true() {
        // Given: an EditorConfig file with all properties set to true
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = true
trim_leading_newlines = true
"#,
        );

        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test").unwrap();

        // When: getting format rules for the file
        let rules = get_format_rules(&test_file);

        // Then: all rules should be enabled
        assert_eq!(
            rules,
            FormatRules {
                ensure_final_newline: true,
                remove_trailing_spaces: true,
                remove_leading_newlines: true,
            }
        );
    }

    #[test]
    fn test_properties_explicitly_false() {
        // Given: an EditorConfig file with properties set to false
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[*]
insert_final_newline = false
trim_trailing_whitespace = false
trim_leading_newlines = false
"#,
        );

        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test").unwrap();

        // When: getting format rules for the file
        let rules = get_format_rules(&test_file);

        // Then: all rules should be disabled
        assert_eq!(
            rules,
            FormatRules {
                ensure_final_newline: false,
                remove_trailing_spaces: false,
                remove_leading_newlines: false,
            }
        );
    }

    #[test]
    fn test_properties_unset() {
        // Given: an EditorConfig file with properties explicitly set to "unset"
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[*]
insert_final_newline = unset
trim_trailing_whitespace = unset
trim_leading_newlines = unset
"#,
        );

        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test").unwrap();

        // When: getting format rules for the file
        let rules = get_format_rules(&test_file);

        // Then: all rules should be disabled (unset means disabled)
        assert_eq!(
            rules,
            FormatRules {
                ensure_final_newline: false,
                remove_trailing_spaces: false,
                remove_leading_newlines: false,
            }
        );
    }

    #[test]
    fn test_properties_not_set() {
        // Given: an EditorConfig file without our properties
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[*]
charset = utf-8
indent_style = space
"#,
        );

        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test").unwrap();

        // When: getting format rules for the file
        let rules = get_format_rules(&test_file);

        // Then: all rules should be disabled (default)
        assert_eq!(
            rules,
            FormatRules {
                ensure_final_newline: false,
                remove_trailing_spaces: false,
                remove_leading_newlines: false,
            }
        );
    }

    #[test]
    fn test_section_override() {
        // Given: an EditorConfig file with multiple sections that override each other
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = true
trim_leading_newlines = true

[*.md]
trim_trailing_whitespace = false
"#,
        );

        // When: getting format rules for a Markdown file
        let md_file = temp_dir.path().join("test.md");
        fs::write(&md_file, "test").unwrap();
        let md_rules = get_format_rules(&md_file);

        // Then: trim_trailing_whitespace should be disabled for .md files
        assert_eq!(
            md_rules,
            FormatRules {
                ensure_final_newline: true,
                remove_trailing_spaces: false, // overridden
                remove_leading_newlines: true,
            }
        );

        // When: getting format rules for a non-Markdown file
        let txt_file = temp_dir.path().join("test.txt");
        fs::write(&txt_file, "test").unwrap();
        let txt_rules = get_format_rules(&txt_file);

        // Then: all rules should remain enabled
        assert_eq!(
            txt_rules,
            FormatRules {
                ensure_final_newline: true,
                remove_trailing_spaces: true,
                remove_leading_newlines: true,
            }
        );
    }

    #[test]
    fn test_custom_property_trim_leading_newlines() {
        // Given: an EditorConfig file with our custom property
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = true
trim_leading_newlines = true
"#,
        );

        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test").unwrap();

        // When: getting format rules
        let rules = get_format_rules(&test_file);

        // Then: the custom property should be respected
        assert!(rules.remove_leading_newlines);
    }

    #[test]
    fn test_directory_pattern_matching() {
        // Given: an EditorConfig file with directory-specific rules
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = true

[test/**]
trim_trailing_whitespace = false
"#,
        );

        // When: getting format rules for a file in test/ directory
        let test_dir = temp_dir.path().join("test");
        fs::create_dir(&test_dir).unwrap();
        let test_file = test_dir.join("example.txt");
        fs::write(&test_file, "test").unwrap();
        let test_rules = get_format_rules(&test_file);

        // Then: the directory-specific rule should apply
        assert_eq!(
            test_rules,
            FormatRules {
                ensure_final_newline: true,
                remove_trailing_spaces: false, // overridden for test/**
                remove_leading_newlines: false,
            }
        );

        // When: getting format rules for a file outside test/ directory
        let root_file = temp_dir.path().join("root.txt");
        fs::write(&root_file, "test").unwrap();
        let root_rules = get_format_rules(&root_file);

        // Then: the default rules should apply
        assert_eq!(
            root_rules,
            FormatRules {
                ensure_final_newline: true,
                remove_trailing_spaces: true,
                remove_leading_newlines: false,
            }
        );
    }

    #[test]
    fn test_mixed_values() {
        // Given: an EditorConfig file with a mix of true/false values
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = false
trim_leading_newlines = true
"#,
        );

        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test").unwrap();

        // When: getting format rules
        let rules = get_format_rules(&test_file);

        // Then: rules should match the specified values
        assert_eq!(
            rules,
            FormatRules {
                ensure_final_newline: true,
                remove_trailing_spaces: false,
                remove_leading_newlines: true,
            }
        );
    }

    #[test]
    fn test_no_editorconfig_file() {
        // Given: a directory without .editorconfig file
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test").unwrap();

        // When: getting format rules
        let rules = get_format_rules(&test_file);

        // Then: all rules should be disabled (default)
        assert_eq!(
            rules,
            FormatRules {
                ensure_final_newline: false,
                remove_trailing_spaces: false,
                remove_leading_newlines: false,
            }
        );
    }

    #[test]
    fn test_extension_pattern_matching() {
        // Given: an EditorConfig file with extension-specific rules
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[*]
insert_final_newline = true

[*.md]
insert_final_newline = false
trim_trailing_whitespace = false

[*.txt]
trim_trailing_whitespace = true
"#,
        );

        // When: getting format rules for .md file
        let md_file = temp_dir.path().join("README.md");
        fs::write(&md_file, "test").unwrap();
        let md_rules = get_format_rules(&md_file);

        // Then: .md specific rules should apply
        assert_eq!(
            md_rules,
            FormatRules {
                ensure_final_newline: false,
                remove_trailing_spaces: false,
                remove_leading_newlines: false,
            }
        );

        // When: getting format rules for .txt file
        let txt_file = temp_dir.path().join("test.txt");
        fs::write(&txt_file, "test").unwrap();
        let txt_rules = get_format_rules(&txt_file);

        // Then: .txt specific rules should apply
        assert_eq!(
            txt_rules,
            FormatRules {
                ensure_final_newline: true,
                remove_trailing_spaces: true,
                remove_leading_newlines: false,
            }
        );
    }

    #[test]
    fn test_parent_directory_lookup() {
        // Given: .editorconfig in parent, file in subdirectory
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = true
"#,
        );

        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        let test_file = sub_dir.join("test.txt");
        fs::write(&test_file, "test").unwrap();

        // When: getting format rules for the file in subdirectory
        let rules = get_format_rules(&test_file);

        // Then: should inherit settings from parent directory
        assert_eq!(
            rules,
            FormatRules {
                ensure_final_newline: true,
                remove_trailing_spaces: true,
                remove_leading_newlines: false,
            },
            "File in subdirectory should inherit settings from parent .editorconfig"
        );
    }

    #[test]
    fn test_hierarchical_config_merging() {
        // Given: .editorconfig in both parent and child directories
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = true
"#,
        );

        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        let child_config = sub_dir.join(".editorconfig");
        fs::write(
            &child_config,
            r#"
[*]
trim_trailing_whitespace = false
trim_leading_newlines = true
"#,
        )
        .unwrap();

        let test_file = sub_dir.join("test.txt");
        fs::write(&test_file, "test").unwrap();

        // When: getting format rules
        let rules = get_format_rules(&test_file);

        // Then: child should override parent's trim_trailing_whitespace
        // but inherit insert_final_newline
        assert_eq!(
            rules,
            FormatRules {
                ensure_final_newline: true,    // from parent
                remove_trailing_spaces: false, // overridden by child
                remove_leading_newlines: true, // from child
            },
            "Child .editorconfig should override parent settings while inheriting others"
        );
    }

    #[test]
    fn test_root_directive_stops_search() {
        // Given: two .editorconfig files, child has root=true
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
[*]
insert_final_newline = true
trim_trailing_whitespace = true
"#,
        );

        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        let child_config = sub_dir.join(".editorconfig");
        fs::write(
            &child_config,
            r#"
root = true

[*]
trim_trailing_whitespace = false
"#,
        )
        .unwrap();

        let test_file = sub_dir.join("test.txt");
        fs::write(&test_file, "test").unwrap();

        // When: getting format rules
        let rules = get_format_rules(&test_file);

        // Then: should NOT inherit from parent due to root=true in child
        assert_eq!(
            rules,
            FormatRules {
                ensure_final_newline: false, // NOT inherited
                remove_trailing_spaces: false,
                remove_leading_newlines: false,
            },
            "root=true should stop search and not inherit from parent"
        );
    }

    #[test]
    fn test_glob_pattern_brace_expansion() {
        // Given: braces pattern like {js,ts}
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[*.{js,ts}]
insert_final_newline = true
trim_trailing_whitespace = true
"#,
        );

        // Test .js file
        let js_file = temp_dir.path().join("test.js");
        fs::write(&js_file, "test").unwrap();
        let js_rules = get_format_rules(&js_file);
        assert!(
            js_rules.ensure_final_newline,
            "*.{{js,ts}} pattern should match .js files"
        );
        assert!(js_rules.remove_trailing_spaces);

        // Test .ts file
        let ts_file = temp_dir.path().join("test.ts");
        fs::write(&ts_file, "test").unwrap();
        let ts_rules = get_format_rules(&ts_file);
        assert!(
            ts_rules.ensure_final_newline,
            "*.{{js,ts}} pattern should match .ts files"
        );
        assert!(ts_rules.remove_trailing_spaces);

        // Test .txt file (should not match)
        let txt_file = temp_dir.path().join("test.txt");
        fs::write(&txt_file, "test").unwrap();
        let txt_rules = get_format_rules(&txt_file);
        assert!(
            !txt_rules.ensure_final_newline,
            "*.{{js,ts}} pattern should NOT match .txt files"
        );
    }

    #[test]
    fn test_glob_pattern_character_range() {
        // Given: character range pattern
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[file[0-9].txt]
insert_final_newline = true
"#,
        );

        let file1 = temp_dir.path().join("file5.txt");
        fs::write(&file1, "test").unwrap();
        assert!(
            get_format_rules(&file1).ensure_final_newline,
            "file[0-9].txt pattern should match file5.txt"
        );

        let file2 = temp_dir.path().join("fileA.txt");
        fs::write(&file2, "test").unwrap();
        assert!(
            !get_format_rules(&file2).ensure_final_newline,
            "file[0-9].txt pattern should NOT match fileA.txt"
        );
    }

    #[test]
    fn test_glob_pattern_double_asterisk() {
        // Given: ** pattern for nested directories
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[**/test/*.txt]
insert_final_newline = true
"#,
        );

        let nested_dir = temp_dir.path().join("foo/bar/test");
        fs::create_dir_all(&nested_dir).unwrap();
        let nested_file = nested_dir.join("example.txt");
        fs::write(&nested_file, "test").unwrap();

        let rules = get_format_rules(&nested_file);
        assert!(
            rules.ensure_final_newline,
            "**/test/*.txt pattern should match files in deeply nested test directories"
        );
    }

    #[test]
    fn test_invalid_boolean_value() {
        // Given: invalid boolean value in config
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[*]
insert_final_newline = invalid_value
"#,
        );

        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test").unwrap();

        // When: getting format rules
        let rules = get_format_rules(&test_file);

        // Then: should treat as false/disabled (graceful handling)
        assert_eq!(
            rules,
            FormatRules::default(),
            "Invalid boolean values should be treated as false/disabled"
        );
    }

    #[test]
    fn test_malformed_editorconfig() {
        // Given: syntactically invalid .editorconfig
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".editorconfig");
        fs::write(&config_path, "this is not valid INI format [[[").unwrap();

        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test").unwrap();

        // When: getting format rules
        let rules = get_format_rules(&test_file);

        // Then: should return default rules without panicking
        assert_eq!(
            rules,
            FormatRules::default(),
            "Malformed .editorconfig should be handled gracefully"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_symlink_handling() {
        use std::os::unix::fs as unix_fs;

        // Given: file accessed through symlink
        let temp_dir = TempDir::new().unwrap();
        create_editorconfig(
            &temp_dir,
            r#"
root = true

[*]
insert_final_newline = true
"#,
        );

        let real_file = temp_dir.path().join("real.txt");
        fs::write(&real_file, "test").unwrap();

        let link_file = temp_dir.path().join("link.txt");
        unix_fs::symlink(&real_file, &link_file).unwrap();

        // When: getting format rules via symlink
        let rules = get_format_rules(&link_file);

        // Then: should resolve symlink and find .editorconfig
        assert!(
            rules.ensure_final_newline,
            "Symlinks should be resolved to find .editorconfig"
        );
    }
}
