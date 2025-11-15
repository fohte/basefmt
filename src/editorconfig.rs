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
    ///
    /// # Arguments
    ///
    /// * `canonical_path` - A canonicalized (absolute) path to the file.
    ///   Must be obtained via `Path::canonicalize()`.
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
    use rstest::rstest;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    struct TestWorkspace {
        temp_dir: TempDir,
    }

    impl TestWorkspace {
        fn new() -> Self {
            Self {
                temp_dir: TempDir::new().unwrap(),
            }
        }

        fn join(&self, rel: impl AsRef<Path>) -> PathBuf {
            self.temp_dir.path().join(rel)
        }

        fn write_editorconfig(&self, rel_dir: impl AsRef<Path>, content: &str) {
            let dir_path = self.join(rel_dir);
            fs::create_dir_all(&dir_path).unwrap();
            fs::write(dir_path.join(".editorconfig"), content).unwrap();
        }

        fn write_file(&self, rel_path: impl AsRef<Path>, content: &str) -> PathBuf {
            let file_path = self.join(rel_path);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&file_path, content).unwrap();
            file_path
        }

        fn rules(&self, rel_path: impl AsRef<Path>) -> FormatRules {
            let file_path = self.join(rel_path);
            get_format_rules(&file_path)
        }
    }

    #[rstest]
    #[case::all_true(
        r#"
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = true
trim_leading_newlines = true
"#,
        FormatRules {
            ensure_final_newline: true,
            remove_trailing_spaces: true,
            remove_leading_newlines: true,
        }
    )]
    #[case::all_false(
        r#"
root = true

[*]
insert_final_newline = false
trim_trailing_whitespace = false
trim_leading_newlines = false
"#,
        FormatRules {
            ensure_final_newline: false,
            remove_trailing_spaces: false,
            remove_leading_newlines: false,
        }
    )]
    #[case::unset(
        r#"
root = true

[*]
insert_final_newline = unset
trim_trailing_whitespace = unset
trim_leading_newlines = unset
"#,
        FormatRules {
            ensure_final_newline: false,
            remove_trailing_spaces: false,
            remove_leading_newlines: false,
        }
    )]
    #[case::not_present(
        r#"
root = true

[*]
charset = utf-8
indent_style = space
"#,
        FormatRules {
            ensure_final_newline: false,
            remove_trailing_spaces: false,
            remove_leading_newlines: false,
        }
    )]
    #[case::mixed(
        r#"
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = false
trim_leading_newlines = true
"#,
        FormatRules {
            ensure_final_newline: true,
            remove_trailing_spaces: false,
            remove_leading_newlines: true,
        }
    )]
    fn test_property_matrix(#[case] config: &str, #[case] expected: FormatRules) {
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(".", config);
        workspace.write_file("test.txt", "test");

        let rules = workspace.rules("test.txt");
        assert_eq!(rules, expected);
    }

    #[test]
    fn test_section_override() {
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
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

        workspace.write_file("test.md", "test");
        let md_rules = workspace.rules("test.md");
        assert_eq!(
            md_rules,
            FormatRules {
                ensure_final_newline: true,
                remove_trailing_spaces: false,
                remove_leading_newlines: true,
            }
        );

        workspace.write_file("test.txt", "test");
        let txt_rules = workspace.rules("test.txt");
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
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
            r#"
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = true
trim_leading_newlines = true
"#,
        );

        workspace.write_file("test.txt", "test");
        let rules = workspace.rules("test.txt");
        assert!(rules.remove_leading_newlines);
    }

    #[test]
    fn test_directory_pattern_matching() {
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
            r#"
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = true

[test/**]
trim_trailing_whitespace = false
"#,
        );

        workspace.write_file("test/example.txt", "test");
        let test_rules = workspace.rules("test/example.txt");
        assert_eq!(
            test_rules,
            FormatRules {
                ensure_final_newline: true,
                remove_trailing_spaces: false,
                remove_leading_newlines: false,
            }
        );

        workspace.write_file("root.txt", "test");
        let root_rules = workspace.rules("root.txt");
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
    fn test_no_editorconfig_file() {
        let workspace = TestWorkspace::new();
        workspace.write_file("test.txt", "test");
        let rules = workspace.rules("test.txt");
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
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
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

        workspace.write_file("README.md", "test");
        let md_rules = workspace.rules("README.md");
        assert_eq!(
            md_rules,
            FormatRules {
                ensure_final_newline: false,
                remove_trailing_spaces: false,
                remove_leading_newlines: false,
            }
        );

        workspace.write_file("test.txt", "test");
        let txt_rules = workspace.rules("test.txt");
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
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
            r#"
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = true
"#,
        );

        workspace.write_file("subdir/test.txt", "test");
        let rules = workspace.rules("subdir/test.txt");
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
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
            r#"
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = true
"#,
        );

        workspace.write_editorconfig(
            "subdir",
            r#"
[*]
trim_trailing_whitespace = false
trim_leading_newlines = true
"#,
        );

        workspace.write_file("subdir/test.txt", "test");
        let rules = workspace.rules("subdir/test.txt");
        assert_eq!(
            rules,
            FormatRules {
                ensure_final_newline: true,
                remove_trailing_spaces: false,
                remove_leading_newlines: true,
            },
            "Child .editorconfig should override parent settings while inheriting others"
        );
    }

    #[test]
    fn test_root_directive_stops_search() {
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
            r#"
[*]
insert_final_newline = true
trim_trailing_whitespace = true
"#,
        );

        workspace.write_editorconfig(
            "subdir",
            r#"
root = true

[*]
trim_trailing_whitespace = false
"#,
        );

        workspace.write_file("subdir/test.txt", "test");
        let rules = workspace.rules("subdir/test.txt");
        assert_eq!(
            rules,
            FormatRules {
                ensure_final_newline: false,
                remove_trailing_spaces: false,
                remove_leading_newlines: false,
            },
            "root=true should stop search and not inherit from parent"
        );
    }

    #[test]
    fn test_root_false_allows_parent_lookup() {
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
            r#"

[*]
insert_final_newline = true
"#,
        );

        workspace.write_editorconfig(
            "child",
            r#"
root = false

[*]
trim_trailing_whitespace = true
"#,
        );

        workspace.write_file("child/test.txt", "test");
        let rules = workspace.rules("child/test.txt");
        assert_eq!(
            rules,
            FormatRules {
                ensure_final_newline: true,
                remove_trailing_spaces: true,
                remove_leading_newlines: false,
            }
        );
    }

    #[test]
    fn test_missing_root_directive_still_merges_ancestors() {
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
            r#"

[*]
insert_final_newline = true
"#,
        );

        workspace.write_editorconfig(
            "mid",
            r#"

[*]
trim_trailing_whitespace = true
"#,
        );

        workspace.write_file("mid/leaf/test.txt", "test");
        let rules = workspace.rules("mid/leaf/test.txt");
        assert_eq!(
            rules,
            FormatRules {
                ensure_final_newline: true,
                remove_trailing_spaces: true,
                remove_leading_newlines: false,
            }
        );
    }

    #[test]
    fn test_glob_pattern_brace_expansion() {
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
            r#"
root = true

[*.{js,ts}]
insert_final_newline = true
trim_trailing_whitespace = true
"#,
        );

        workspace.write_file("test.js", "test");
        let js_rules = workspace.rules("test.js");
        assert!(
            js_rules.ensure_final_newline,
            "*.{{js,ts}} pattern should match .js files"
        );
        assert!(js_rules.remove_trailing_spaces);

        workspace.write_file("test.ts", "test");
        let ts_rules = workspace.rules("test.ts");
        assert!(
            ts_rules.ensure_final_newline,
            "*.{{js,ts}} pattern should match .ts files"
        );
        assert!(ts_rules.remove_trailing_spaces);

        workspace.write_file("test.txt", "test");
        let txt_rules = workspace.rules("test.txt");
        assert!(
            !txt_rules.ensure_final_newline,
            "*.{{js,ts}} pattern should NOT match .txt files"
        );
    }

    #[test]
    fn test_glob_pattern_character_range() {
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
            r#"
root = true

[file[0-9].txt]
insert_final_newline = true
"#,
        );

        workspace.write_file("file5.txt", "test");
        assert!(
            workspace.rules("file5.txt").ensure_final_newline,
            "file[0-9].txt pattern should match file5.txt"
        );

        workspace.write_file("fileA.txt", "test");
        assert!(
            !workspace.rules("fileA.txt").ensure_final_newline,
            "file[0-9].txt pattern should NOT match fileA.txt"
        );
    }

    #[test]
    fn test_glob_pattern_double_asterisk() {
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
            r#"
root = true

[**/test/*.txt]
insert_final_newline = true
"#,
        );

        workspace.write_file("foo/bar/test/example.txt", "test");
        let rules = workspace.rules("foo/bar/test/example.txt");
        assert!(
            rules.ensure_final_newline,
            "**/test/*.txt pattern should match files in deeply nested test directories"
        );
    }

    #[test]
    fn test_invalid_boolean_value() {
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
            r#"
root = true

[*]
insert_final_newline = invalid_value
"#,
        );

        workspace.write_file("test.txt", "test");
        let rules = workspace.rules("test.txt");
        assert_eq!(
            rules,
            FormatRules::default(),
            "Invalid boolean values should be treated as false/disabled"
        );
    }

    #[test]
    fn test_malformed_editorconfig() {
        let workspace = TestWorkspace::new();
        let config_path = workspace.join(".editorconfig");
        fs::write(&config_path, "this is not valid INI format [[[").unwrap();

        workspace.write_file("test.txt", "test");
        let rules = workspace.rules("test.txt");
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

        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
            r#"
root = true

[*]
insert_final_newline = true
"#,
        );

        let real_file = workspace.write_file("real.txt", "test");
        let link_file = workspace.join("link.txt");
        unix_fs::symlink(&real_file, &link_file).unwrap();

        let rules = get_format_rules(&link_file);
        assert!(
            rules.ensure_final_newline,
            "Symlinks should be resolved to find .editorconfig"
        );
    }
}
