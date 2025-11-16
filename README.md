# basefmt

basefmt is a formatter for any text files. It provides universal rules applicable to any text file, such as ensuring final newlines and removing trailing spaces.

Missing final newlines and trailing spaces are common formatting issues that occur during code generation or manual editing. While language-specific formatters handle these for source code, general text files often lack such tools. basefmt fills this gap and works alongside other formatters.

## Installation

```bash
cargo install basefmt
```

## Usage

Format files in the current directory:

```bash
basefmt .
```

Format specific files or directories:

```bash
basefmt file1.txt file2.md src/
```

Check files without modifying them (useful for CI):

```bash
basefmt --check .
```

Exit codes:
- `0`: All files are properly formatted (or successfully formatted in non-check mode)
- `1`: Some files need formatting (check mode only)
- `2`: Error occurred during execution

## Formatting Rules

basefmt applies the following universal formatting rules:

| Rule | Description |
|------|-------------|
| Remove leading newlines | Empty lines at the beginning of files are removed |
| Remove trailing spaces | Whitespace at the end of each line is removed |
| Ensure final newline | Files must end with exactly one newline character |

## EditorConfig Support

basefmt integrates with [EditorConfig](https://editorconfig.org/) to respect project-specific formatting preferences. When an `.editorconfig` file is present, basefmt reads the relevant properties to determine formatting rules for each file.

### Property Mapping

The following EditorConfig properties are mapped to basefmt's formatting rules:

| EditorConfig Property | basefmt Rule | Description |
|----------------------|--------------|-------------|
| `insert_final_newline` | Ensure final newline | Controls whether files should end with a newline |
| `trim_trailing_whitespace` | Remove trailing spaces | Controls whether trailing whitespace should be removed |
| `trim_leading_newlines` **(custom)** | Remove leading newlines | **basefmt extension:** Controls leading newline removal |

**Note**: `trim_leading_newlines` is a custom property specific to basefmt and not part of the EditorConfig specification.

### Property Value Interpretation

- `true`: Rule is enabled
- `false`: Rule is disabled
- `unset`: Rule is disabled
- Not specified: Rule is enabled (default)

### Example

```ini
root = true

[*]
insert_final_newline = true
trim_trailing_whitespace = true
trim_leading_newlines = true

[*.md]
trim_trailing_whitespace = false
```

In this example, all files will have trailing whitespace removed except for Markdown files (`.md`), which often use trailing spaces for line breaks.

## Configuration

You can configure basefmt using a `.basefmt.toml` file in your project root.

### Excluding Files

basefmt automatically respects `.gitignore` files. Files ignored by git will not be formatted.

Additionally, you can use the `exclude` option in `.basefmt.toml` to specify glob patterns for files that should be excluded from formatting:

```toml
exclude = ["*.min.*", "test/**", "vendor/**"]
```

Common patterns:
- `*.min.*`: Exclude minified files
- `**/node_modules/**`: Exclude dependency directories
- `vendor/**`: Exclude vendor directories
- `*.generated.*`: Exclude generated files

If `.basefmt.toml` doesn't exist, basefmt will format all files except those in `.gitignore`.
