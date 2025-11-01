# basefmt

basefmt is a formatter for any text files. It provides universal rules applicable to any text file, such as ensuring final newlines and removing trailing spaces.

Missing final newlines and trailing spaces are common formatting issues that occur during code generation or manual editing. While language-specific formatters handle these for source code, general text files often lack such tools. basefmt fills this gap and works alongside other formatters.

## Installation

```bash
cargo install --path .
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
