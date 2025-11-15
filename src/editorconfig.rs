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
    use indoc::indoc;
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
        indoc! {"
            root = true

            [*]
            insert_final_newline = true
            trim_trailing_whitespace = true
            trim_leading_newlines = true
        "},
        FormatRules {
            ensure_final_newline: true,
            remove_trailing_spaces: true,
            remove_leading_newlines: true,
        }
    )]
    #[case::all_false(
        indoc! {"
            root = true

            [*]
            insert_final_newline = false
            trim_trailing_whitespace = false
            trim_leading_newlines = false
        "},
        FormatRules {
            ensure_final_newline: false,
            remove_trailing_spaces: false,
            remove_leading_newlines: false,
        }
    )]
    #[case::unset(
        indoc! {"
            root = true

            [*]
            insert_final_newline = unset
            trim_trailing_whitespace = unset
            trim_leading_newlines = unset
        "},
        FormatRules {
            ensure_final_newline: false,
            remove_trailing_spaces: false,
            remove_leading_newlines: false,
        }
    )]
    #[case::not_present(
        indoc! {"
            root = true

            [*]
            charset = utf-8
            indent_style = space
        "},
        FormatRules {
            ensure_final_newline: false,
            remove_trailing_spaces: false,
            remove_leading_newlines: false,
        }
    )]
    #[case::mixed(
        indoc! {"
            root = true

            [*]
            insert_final_newline = true
            trim_trailing_whitespace = false
            trim_leading_newlines = true
        "},
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

    #[rstest]
    #[case::section_markdown(
        indoc! {"
            root = true

            [*]
            insert_final_newline = true
            trim_trailing_whitespace = true
            trim_leading_newlines = true

            [*.md]
            trim_trailing_whitespace = false
        "},
        "test.md",
        FormatRules {
            ensure_final_newline: true,
            remove_trailing_spaces: false,
            remove_leading_newlines: true,
        }
    )]
    #[case::section_txt(
        indoc! {"
            root = true

            [*]
            insert_final_newline = true
            trim_trailing_whitespace = true
            trim_leading_newlines = true

            [*.md]
            trim_trailing_whitespace = false
        "},
        "test.txt",
        FormatRules {
            ensure_final_newline: true,
            remove_trailing_spaces: true,
            remove_leading_newlines: true,
        }
    )]
    #[case::dir_match(
        indoc! {"
            root = true

            [*]
            insert_final_newline = true
            trim_trailing_whitespace = true

            [test/**]
            trim_trailing_whitespace = false
        "},
        "test/example.txt",
        FormatRules {
            ensure_final_newline: true,
            remove_trailing_spaces: false,
            ..FormatRules::default()
        }
    )]
    #[case::dir_outside(
        indoc! {"
            root = true

            [*]
            insert_final_newline = true
            trim_trailing_whitespace = true

            [test/**]
            trim_trailing_whitespace = false
        "},
        "root.txt",
        FormatRules {
            ensure_final_newline: true,
            remove_trailing_spaces: true,
            ..FormatRules::default()
        }
    )]
    #[case::extension_md(
        indoc! {"
            root = true

            [*]
            insert_final_newline = true

            [*.md]
            insert_final_newline = false
            trim_trailing_whitespace = false

            [*.txt]
            trim_trailing_whitespace = true
        "},
        "README.md",
        FormatRules {
            ensure_final_newline: false,
            remove_trailing_spaces: false,
            ..FormatRules::default()
        }
    )]
    #[case::extension_txt(
        indoc! {"
            root = true

            [*]
            insert_final_newline = true

            [*.md]
            insert_final_newline = false
            trim_trailing_whitespace = false

            [*.txt]
            trim_trailing_whitespace = true
        "},
        "test.txt",
        FormatRules {
            ensure_final_newline: true,
            remove_trailing_spaces: true,
            ..FormatRules::default()
        }
    )]
    fn test_pattern_matching(
        #[case] config: &str,
        #[case] file_path: &str,
        #[case] expected: FormatRules,
    ) {
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(".", config);
        workspace.write_file(file_path, "test");

        let rules = workspace.rules(file_path);
        assert_eq!(rules, expected);
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

    #[rstest]
    #[case::parent_lookup(
        vec![
            (
                ".",
                indoc! {"
                    root = true

                    [*]
                    insert_final_newline = true
                    trim_trailing_whitespace = true
                "},
            ),
        ],
        "subdir/test.txt",
        FormatRules {
            ensure_final_newline: true,
            remove_trailing_spaces: true,
            ..FormatRules::default()
        }
    )]
    #[case::child_overrides(
        vec![
            (
                ".",
                indoc! {"
                    root = true

                    [*]
                    insert_final_newline = true
                    trim_trailing_whitespace = true
                "},
            ),
            (
                "subdir",
                indoc! {"
                    [*]
                    trim_trailing_whitespace = false
                    trim_leading_newlines = true
                "},
            ),
        ],
        "subdir/test.txt",
        FormatRules {
            ensure_final_newline: true,
            remove_trailing_spaces: false,
            remove_leading_newlines: true,
        }
    )]
    #[case::root_stops_search(
        vec![
            (
                ".",
                indoc! {"
                    root = true

                    [*]
                    insert_final_newline = true
                    trim_trailing_whitespace = true
                "},
            ),
            (
                "subdir",
                indoc! {"
                    root = true

                    [*]
                    trim_trailing_whitespace = false
                "},
            ),
        ],
        "subdir/test.txt",
        FormatRules {
            ensure_final_newline: false,
            remove_trailing_spaces: false,
            ..FormatRules::default()
        }
    )]
    #[case::root_false_propagates(
        vec![
            (
                ".",
                indoc! {"

                    [*]
                    insert_final_newline = true
                "},
            ),
            (
                "child",
                indoc! {"
                    root = false

                    [*]
                    trim_trailing_whitespace = true
                "},
            ),
        ],
        "child/test.txt",
        FormatRules {
            ensure_final_newline: true,
            remove_trailing_spaces: true,
            ..FormatRules::default()
        }
    )]
    #[case::missing_root_merges(
        vec![
            (
                ".",
                indoc! {"

                    [*]
                    insert_final_newline = true
                "},
            ),
            (
                "mid",
                indoc! {"

                    [*]
                    trim_trailing_whitespace = true
                "},
            ),
        ],
        "mid/leaf/test.txt",
        FormatRules {
            ensure_final_newline: true,
            remove_trailing_spaces: true,
            ..FormatRules::default()
        }
    )]
    fn test_hierarchy(
        #[case] configs: Vec<(&str, &str)>,
        #[case] file_path: &str,
        #[case] expected: FormatRules,
    ) {
        let workspace = TestWorkspace::new();
        for (dir, config) in configs {
            workspace.write_editorconfig(dir, config);
        }
        workspace.write_file(file_path, "test");

        let rules = workspace.rules(file_path);
        assert_eq!(rules, expected);
    }

    #[rstest]
    #[case::brace_js("*.{js,ts}", "test.js", true)]
    #[case::brace_ts("*.{js,ts}", "test.ts", true)]
    #[case::brace_txt("*.{js,ts}", "test.txt", false)]
    #[case::range_match("file[0-9].txt", "file5.txt", true)]
    #[case::range_miss("file[0-9].txt", "fileA.txt", false)]
    #[case::double_star("**/test/*.txt", "foo/bar/test/example.txt", true)]
    fn test_glob_patterns(
        #[case] pattern: &str,
        #[case] file_path: &str,
        #[case] should_match: bool,
    ) {
        let workspace = TestWorkspace::new();
        let config = format!(
            "root = true\n\n[{}]\ninsert_final_newline = true\n",
            pattern
        );
        workspace.write_editorconfig(".", &config);
        workspace.write_file(file_path, "test");

        let rules = workspace.rules(file_path);
        assert_eq!(rules.ensure_final_newline, should_match);
    }

    #[test]
    fn test_invalid_boolean_value() {
        let workspace = TestWorkspace::new();
        workspace.write_editorconfig(
            ".",
            indoc! {"
                root = true

                [*]
                insert_final_newline = invalid_value
            "},
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
            indoc! {"
                root = true

                [*]
                insert_final_newline = true
            "},
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
