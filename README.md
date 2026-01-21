# octorus

A TUI tool for GitHub PR review, designed for Helix editor users.

## Features

- Browse changed files in a PR
- View diffs with syntax highlighting
- Add inline comments on specific lines
- Submit reviews (Approve / Request Changes / Comment)
- Configurable keybindings and editor

## Requirements

- [GitHub CLI (gh)](https://cli.github.com/) - Must be installed and authenticated
- Rust 1.70+

## Installation

```bash
cargo install octorus
```

Or build from source:

```bash
git clone https://github.com/ushironoko/octorus.git
cd octorus
cargo build --release
cp target/release/or ~/.local/bin/
```

## Usage

```bash
or --repo owner/repo --pr 123
```

### Keybindings

#### File List View

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Enter` | Open diff view |
| `a` | Approve PR |
| `r` | Request changes |
| `c` | Add comment review |
| `?` | Show help |
| `q` | Quit |

#### Diff View

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Ctrl-d` | Page down |
| `Ctrl-u` | Page up |
| `c` | Add inline comment |
| `q` / `Esc` | Back to file list |

## Configuration

Create `~/.config/octorus/config.toml`:

```toml
# Editor to use for writing comments
editor = "hx"

[diff]
renderer = "delta"
side_by_side = true
line_numbers = true

[keybindings]
approve = 'a'
request_changes = 'r'
comment = 'c'
```

## License

MIT
