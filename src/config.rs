use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use std::fs;
use std::io;
use std::path::Path;

/// Configuration for basefmt, typically loaded from .basefmt.toml
#[derive(Debug, Deserialize, PartialEq)]
pub struct Config {
    /// List of glob patterns to exclude from formatting
    #[serde(default)]
    pub exclude: Vec<String>,
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
        toml::from_str(&content).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse .basefmt.toml: {err}"),
            )
        })
    }

    /// Builds a GlobSet from the exclude patterns for efficient matching.
    ///
    /// # Returns
    ///
    /// Returns `Ok(GlobSet)` containing all exclude patterns, or an error
    /// if any pattern is invalid.
    pub fn build_exclude_matcher(&self) -> io::Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();

        for pattern in &self.exclude {
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
    /// # Arguments
    ///
    /// * `path` - Path to check (can be absolute or relative)
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` if the file should be excluded, `Ok(false)` if it should
    /// be processed, or an error if the patterns are invalid.
    pub fn is_excluded(&self, path: &Path) -> io::Result<bool> {
        let matcher = self.build_exclude_matcher()?;
        Ok(matcher.is_match(path))
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            exclude: Vec::new(),
        }
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
        assert_eq!(config, Config::default());
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

        assert_eq!(
            config.exclude,
            vec!["*.min.*", "test/**", "vendor/**"]
        );
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
        let config = Config {
            exclude: vec!["*.min.js".to_string()],
        };

        assert!(config.is_excluded(Path::new("app.min.js")).unwrap());
        assert!(!config.is_excluded(Path::new("app.js")).unwrap());
    }

    #[test]
    fn test_is_excluded_wildcard_pattern() {
        let config = Config {
            exclude: vec!["*.min.*".to_string()],
        };

        assert!(config.is_excluded(Path::new("app.min.js")).unwrap());
        assert!(config.is_excluded(Path::new("style.min.css")).unwrap());
        assert!(!config.is_excluded(Path::new("app.js")).unwrap());
    }

    #[test]
    fn test_is_excluded_directory_pattern() {
        let config = Config {
            exclude: vec!["test/**".to_string()],
        };

        assert!(config.is_excluded(Path::new("test/foo.txt")).unwrap());
        assert!(config.is_excluded(Path::new("test/sub/bar.txt")).unwrap());
        assert!(!config.is_excluded(Path::new("src/test.txt")).unwrap());
    }

    #[test]
    fn test_is_excluded_multiple_patterns() {
        let config = Config {
            exclude: vec![
                "*.min.*".to_string(),
                "test/**".to_string(),
                "vendor/**".to_string(),
            ],
        };

        // Should match first pattern
        assert!(config.is_excluded(Path::new("app.min.js")).unwrap());

        // Should match second pattern
        assert!(config.is_excluded(Path::new("test/foo.txt")).unwrap());

        // Should match third pattern
        assert!(config.is_excluded(Path::new("vendor/lib.js")).unwrap());

        // Should not match any pattern
        assert!(!config.is_excluded(Path::new("src/main.js")).unwrap());
    }

    #[test]
    fn test_is_excluded_no_patterns() {
        let config = Config {
            exclude: Vec::new(),
        };

        // Nothing should be excluded when there are no patterns
        assert!(!config.is_excluded(Path::new("anything.txt")).unwrap());
        assert!(!config.is_excluded(Path::new("test/foo.txt")).unwrap());
    }

    #[test]
    fn test_is_excluded_relative_path() {
        let config = Config {
            exclude: vec!["./test/**".to_string()],
        };

        // Glob patterns with ./ prefix require exact match
        assert!(config.is_excluded(Path::new("./test/foo.txt")).unwrap());

        // Without ./ prefix, it won't match the ./test/** pattern
        assert!(!config.is_excluded(Path::new("test/foo.txt")).unwrap());
    }

    #[test]
    fn test_is_excluded_nested_wildcards() {
        let config = Config {
            exclude: vec!["**/node_modules/**".to_string()],
        };

        assert!(config.is_excluded(Path::new("node_modules/pkg/file.js")).unwrap());
        assert!(config.is_excluded(Path::new("src/node_modules/pkg/file.js")).unwrap());
        assert!(!config.is_excluded(Path::new("src/file.js")).unwrap());
    }

    #[test]
    fn test_build_exclude_matcher_invalid_pattern() {
        let config = Config {
            exclude: vec!["[invalid".to_string()],
        };

        let result = config.build_exclude_matcher();

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("invalid glob pattern"));
    }

    #[test]
    fn test_is_excluded_specific_file() {
        let config = Config {
            exclude: vec!["specific/file.txt".to_string()],
        };

        assert!(config.is_excluded(Path::new("specific/file.txt")).unwrap());
        assert!(!config.is_excluded(Path::new("specific/other.txt")).unwrap());
        assert!(!config.is_excluded(Path::new("other/file.txt")).unwrap());
    }

    #[test]
    fn test_is_excluded_case_sensitive() {
        let config = Config {
            exclude: vec!["*.TXT".to_string()],
        };

        // Glob patterns are case-sensitive by default
        assert!(config.is_excluded(Path::new("file.TXT")).unwrap());
        assert!(!config.is_excluded(Path::new("file.txt")).unwrap());
    }
}
