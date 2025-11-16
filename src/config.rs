use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use std::fs;
use std::io;
use std::path::Path;

/// Configuration for basefmt, typically loaded from .basefmt.toml
#[derive(Debug)]
pub struct Config {
    /// List of glob patterns to exclude from formatting
    pub exclude: Vec<String>,

    /// Pre-built GlobSet for efficient matching
    matcher: GlobSet,
}

impl Config {
    /// Loads configuration from .basefmt.toml in the specified directory.
    ///
    /// If the file doesn't exist, returns a default configuration with no exclusions.
    ///
    /// # Arguments
    ///
    /// * `dir` - Directory containing .basefmt.toml
    ///
    /// # Returns
    ///
    /// Returns `Ok(Config)` with the loaded or default configuration, or an error
    /// if the file exists but cannot be read or parsed.
    pub fn load(dir: &Path) -> io::Result<Self> {
        let config_path = dir.join(".basefmt.toml");

        if !config_path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&config_path)?;

        #[derive(Deserialize)]
        struct ConfigFile {
            #[serde(default)]
            exclude: Vec<String>,
        }

        let config_file: ConfigFile = toml::from_str(&content).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse .basefmt.toml: {err}"),
            )
        })?;

        let matcher = Self::build_matcher(&config_file.exclude)?;

        Ok(Config {
            exclude: config_file.exclude,
            matcher,
        })
    }

    /// Builds a GlobSet from the exclude patterns for efficient matching.
    fn build_matcher(patterns: &[String]) -> io::Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();

        for pattern in patterns {
            let glob = Glob::new(pattern).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("invalid glob pattern '{pattern}': {err}"),
                )
            })?;
            builder.add(glob);
        }

        builder.build().map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("failed to build glob set: {err}"),
            )
        })
    }

    /// Checks if a file should be excluded based on the exclude patterns.
    ///
    /// Uses the pre-built GlobSet for efficient matching across multiple calls.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to check (can be absolute or relative)
    ///
    /// # Returns
    ///
    /// Returns `true` if the file should be excluded and `false` otherwise.
    pub fn is_excluded(&self, path: &Path) -> bool {
        self.matcher.is_match(path)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            exclude: Vec::new(),
            matcher: GlobSet::empty(),
        }
    }
}

#[cfg(test)]
impl Config {
    fn with_exclude(patterns: Vec<String>) -> io::Result<Self> {
        let matcher = Self::build_matcher(&patterns)?;
        Ok(Config {
            exclude: patterns,
            matcher,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(config.exclude.is_empty());
    }

    #[test]
    fn test_config_load_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let config = Config::load(temp_dir.path()).unwrap();

        // Should return default config when file doesn't exist
        assert!(config.exclude.is_empty());
    }

    #[test]
    fn test_config_load_empty_exclude() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".basefmt.toml");
        fs::write(&config_path, "exclude = []\n").unwrap();

        let config = Config::load(temp_dir.path()).unwrap();

        assert!(config.exclude.is_empty());
    }

    #[test]
    fn test_config_load_single_exclude_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".basefmt.toml");
        fs::write(&config_path, "exclude = [\"*.min.js\"]\n").unwrap();

        let config = Config::load(temp_dir.path()).unwrap();

        assert_eq!(config.exclude, vec!["*.min.js"]);
    }

    #[test]
    fn test_config_load_multiple_exclude_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".basefmt.toml");
        fs::write(
            &config_path,
            r#"exclude = ["*.min.*", "test/**", "vendor/**"]
"#,
        )
        .unwrap();

        let config = Config::load(temp_dir.path()).unwrap();

        assert_eq!(config.exclude, vec!["*.min.*", "test/**", "vendor/**"]);
    }

    #[test]
    fn test_config_load_invalid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".basefmt.toml");
        fs::write(&config_path, "invalid toml syntax [[\n").unwrap();

        let result = Config::load(temp_dir.path());

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("failed to parse"));
    }

    #[test]
    fn test_is_excluded_simple_pattern() {
        let config = Config::with_exclude(vec!["*.min.js".to_string()]).unwrap();

        assert!(config.is_excluded(Path::new("app.min.js")));
        assert!(!config.is_excluded(Path::new("app.js")));
    }

    #[test]
    fn test_is_excluded_wildcard_pattern() {
        let config = Config::with_exclude(vec!["*.min.*".to_string()]).unwrap();

        assert!(config.is_excluded(Path::new("app.min.js")));
        assert!(config.is_excluded(Path::new("style.min.css")));
        assert!(!config.is_excluded(Path::new("app.js")));
    }

    #[test]
    fn test_is_excluded_directory_pattern() {
        let config = Config::with_exclude(vec!["test/**".to_string()]).unwrap();

        assert!(config.is_excluded(Path::new("test/foo.txt")));
        assert!(config.is_excluded(Path::new("test/sub/bar.txt")));
        assert!(!config.is_excluded(Path::new("src/test.txt")));
    }

    #[test]
    fn test_is_excluded_multiple_patterns() {
        let config = Config::with_exclude(vec![
            "*.min.*".to_string(),
            "test/**".to_string(),
            "vendor/**".to_string(),
        ])
        .unwrap();

        // Should match first pattern
        assert!(config.is_excluded(Path::new("app.min.js")));

        // Should match second pattern
        assert!(config.is_excluded(Path::new("test/foo.txt")));

        // Should match third pattern
        assert!(config.is_excluded(Path::new("vendor/lib.js")));

        // Should not match any pattern
        assert!(!config.is_excluded(Path::new("src/main.js")));
    }

    #[test]
    fn test_is_excluded_no_patterns() {
        let config = Config::with_exclude(Vec::new()).unwrap();

        // Nothing should be excluded when there are no patterns
        assert!(!config.is_excluded(Path::new("anything.txt")));
        assert!(!config.is_excluded(Path::new("test/foo.txt")));
    }

    #[test]
    fn test_is_excluded_relative_path() {
        let config = Config::with_exclude(vec!["./test/**".to_string()]).unwrap();

        // Glob patterns with ./ prefix require exact match
        assert!(config.is_excluded(Path::new("./test/foo.txt")));

        // Without ./ prefix, it won't match the ./test/** pattern
        assert!(!config.is_excluded(Path::new("test/foo.txt")));
    }

    #[test]
    fn test_is_excluded_nested_wildcards() {
        let config = Config::with_exclude(vec!["**/node_modules/**".to_string()]).unwrap();

        assert!(config.is_excluded(Path::new("node_modules/pkg/file.js")));
        assert!(config.is_excluded(Path::new("src/node_modules/pkg/file.js")));
        assert!(!config.is_excluded(Path::new("src/file.js")));
    }

    #[test]
    fn test_with_exclude_invalid_pattern() {
        let result = Config::with_exclude(vec!["[invalid".to_string()]);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("invalid glob pattern"));
    }

    #[test]
    fn test_is_excluded_specific_file() {
        let config = Config::with_exclude(vec!["specific/file.txt".to_string()]).unwrap();

        assert!(config.is_excluded(Path::new("specific/file.txt")));
        assert!(!config.is_excluded(Path::new("specific/other.txt")));
        assert!(!config.is_excluded(Path::new("other/file.txt")));
    }

    #[test]
    fn test_is_excluded_case_sensitive() {
        let config = Config::with_exclude(vec!["*.TXT".to_string()]).unwrap();

        // Glob patterns are case-sensitive by default
        assert!(config.is_excluded(Path::new("file.TXT")));
        assert!(!config.is_excluded(Path::new("file.txt")));
    }
}
