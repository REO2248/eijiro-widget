# eijiro-widget

Modern GTK dictionary widget for Eijiro.

## Usage

### 1. Build Index
Convert your Eijiro text file into an optimized binary index.

```bash
# Default (creates ./.eijiro)
eijiro-widget build <PATH_TO_EIJIRO_TXT>

# Specify index directory globally
eijiro-widget --index-dir ~/.eijiro build <PATH_TO_EIJIRO_TXT>
```

### 2. Search (CLI)
Test the search performance directly from the terminal.

```bash
# Prefix search (headwords)
eijiro-widget prefix "apple"

# Full-text search (descriptions)
eijiro-widget fulltext "apple"

# Use specific index and result limit
eijiro-widget -i ~/.eijiro -n 50 fulltext "juice"
```

### 3. Launch GUI
Run without subcommands to open the desktop widget.

```bash
eijiro-widget
```

## Global Options
These options apply to **all** commands (build, search, and GUI mode):

- `-i, --index-dir <DIR>`: Path to the index directory.
  - *Build*: Saves the index here.
  - *Search/GUI*: Reads the index from here.
  - *Default*: Checks `./.eijiro` first, then `~/.eijiro`.
- `-n, --limit <LIMIT>`: Maximum results to show (default: `100`).
  - *Note: Ignored during 'build'.*
- `-l, --log-level <LEVEL>`: Set log level (`debug`, `info`, `warn`, `error`).
