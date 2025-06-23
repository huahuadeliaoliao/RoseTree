# RST (RoseTree)

A fast and efficient command-line tool for scanning directories, analyzing file structures, and extracting file contents with support for `.gitignore` rules.

## Features

- 🚀 **Fast Performance**: Multi-threaded file scanning using Rayon
- 📁 **Smart File Detection**: Accurate UTF-8 text file detection using content inspection
- 🎯 **Selective Extraction**: Choose specific file types to include in the output
- 🔍 **GitIgnore Support**: Optionally apply `.gitignore` rules during scanning
- 📊 **Detailed Statistics**: Performance timing breakdown for all operations
- 🌳 **Tree Structure**: Generate beautiful directory tree visualization
- 📝 **Content Export**: Extract file contents to timestamped output files

## Installation

```bash
cargo install rosetree
```

### From Source (Development)

```bash
git clone https://github.com/huahuadeliaoliao/RoseTree.git
cd RoseTree
cargo build --release
```

## Usage

Simply run the command in any directory:

```bash
rst
```

The tool will:

1. **Scan** the current directory and all subdirectories
2. **Detect** `.gitignore` files and ask if you want to apply the rules
3. **Analyze** all UTF-8 readable files and group them by extension
4. **Display** available file types for selection
5. **Extract** selected file contents to a timestamped output file

### Interactive Prompts

- **GitIgnore Rules**: Choose `y` to respect `.gitignore` files, `n` to scan all files
- **File Type Selection**: 
  - Enter specific numbers (e.g., `1 3 5`) to select certain file types
  - Enter `a` to select all file types

### Example Session

```
Scanning current directory and subdirectories...

Found the following .gitignore files:
  - .gitignore
  - frontend/.gitignore

Apply .gitignore rules? (y/n):
y

Found the following UTF-8 file types:
1. rs
2. md
3. toml
4. json
5. js

Enter file type numbers to extract (space-separated, 'a' for all types):
1 2 3

Reading file contents...

File contents successfully extracted to: rosetree_20240623_143022.md
```

## Output Format

The generated Markdown file contains:

1. **Project Analysis Report**: Structured Markdown document
2. **File Structure**: A tree view of all scanned files in code blocks  
3. **File Contents**: Each file's content with syntax highlighting support

Example output structure:
```markdown
# Project Analysis Report

## File Structure

```
.
├── Cargo.toml
├── README.md
├── src/
│   └── main.rs
└── target/
```

## File Contents

### `Cargo.toml`

```toml
[package]
name = "rosetree"
version = "0.2.0"
...
```

### `README.md`

```markdown
# RST (RoseTree)
...
```
```

## Performance

RST is optimized for speed:

- **Parallel Processing**: Files are scanned and processed in parallel
- **Smart Sampling**: Only reads the first 1024 bytes for UTF-8 detection
- **Memory Efficient**: Streams file content instead of loading everything into memory
- **Timing Reports**: Built-in performance monitoring

Example timing output:
```
Program Operation Execution Times (µs):
-------------------------------------------
Find .gitignore files:           234
Collect files:                 1,567
Read selected contents:       12,890
Generate tree structure:         456
Generate output string:        2,341
Write to file:                   123
-------------------------------------------
Total processing time:        17,611 µs
                              17 ms (approx total)
```

## Dependencies

- [rayon](https://crates.io/crates/rayon) - Data parallelism
- [chrono](https://crates.io/crates/chrono) - Date and time handling
- [ignore](https://crates.io/crates/ignore) - GitIgnore rule processing
- [dashmap](https://crates.io/crates/dashmap) - Concurrent HashMap
- [content_inspector](https://crates.io/crates/content_inspector) - Binary/text file detection

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

## Changelog

### v0.2.0
- **Enhanced Output Format**: Switch from plain text to Markdown with syntax highlighting
- **Improved Internationalization**: All comments translated to English
- **Better Structure**: Organized output with project analysis report format
- **Syntax Highlighting**: Support for 15+ programming languages in output
- **Code Quality**: Improved error handling and code organization

### v0.1.0
- Initial release
- Basic directory scanning and file extraction
- GitIgnore support
- Multi-threaded processing
- UTF-8 file detection optimization