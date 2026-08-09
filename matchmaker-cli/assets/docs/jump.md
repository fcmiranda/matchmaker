# Matchmaker Jump Mode (`mm -o jump` / `j` / `z`)

**Jump Mode** (`mm -o jump`) is Matchmaker's flagship interactive directory navigation and file manager preset. It transforms your terminal into a hybrid ultra-fast fuzzy finder and interactive TUI navigator, combining the instant directory jumping of tools like `zoxide`/`fzf` with the visual power of file managers like **Yazi** and **Superfiles**.

---

## 1. What is Jump Mode & Why Matchmaker?

Jump Mode is designed for instant shell directory switching (`j <keyword>`) and interactive filesystem exploration.

### How it Compares to Other Tools

| Feature | `fzf` + `zoxide` | Yazi / Superfiles | **Matchmaker Jump Mode (`mm -o jump`)** |
| :--- | :--- | :--- | :--- |
| **Speed & Engine** | Go / Shell scripts | Rust (Async I/O) | **Rust (`Nucleo` engine from Helix editor)** |
| **Interactive Navigation** | Static list (Exits on pick) | Full File Manager TUI | **Dynamic `l` (enter) / `h` (parent) without closing session** |
| **Directory Prioritization** | Plain text list | Tree view | **Native 3-Tier Sorting (`dir_first` in C-speed Rust)** |
| **TUI Interface & Breadcrumbs** | Basic border | Dual/Triple pane | **Custom rounded TUI, Breadcrumbs (`/`), Debounced Previews** |
| **Hybrid Discovery** | History OR File search | File tree only | **Hybrid: Local search + Global Frecency cycling (`ctrl-z`)** |

---

## 2. Key Features & Navigation UX

### Instant Interactive Traversal (`l` & `h`)
Unlike traditional fuzzy finders where picking an item terminates the session:
- Press **`l`**: Enters the highlighted directory (`ChDir({=})`), immediately refreshing the view for subfolder exploration.
- Press **`h`**: Navigates up to the parent directory (`ChDir(..)`) .
- Press **`Enter`**: Accepts the selected directory, automatically records it to the `frecency` database, and changes your shell working directory upon exit.

### Hybrid Source Switching (`ctrl-z` / `@reloadnext`)
Jump Mode allows cycling through multiple input sources on the fly:
1. **Source 0 (Default)**: Current directory contents + deeper subdirectories (`fd`).
2. **Source 1**: Local directory search with ignored files.
3. **Source 2**: Global frecency directory database (`mm list --dirs`).

### Visual Previews & Breadcrumbs
- **Live Tree Previews**: Previews folders using `eza --tree --level=2 --icons --git-ignore` and files using `bat`.
- **40 FPS Debounced Rendering**: Configured with `[previewer] debounce_ms = 25` to ensure buttery smooth scrolling (`j`/`k`) without spawning duplicate child processes.
- **Top Breadcrumb**: Displays your current directory path formatted with bold cyan separators (`Cyan /`).

---

## 3. File Manager & File Manipulation Possibilities

Matchmaker Jump Mode goes beyond simple directory jumping, supporting rich overlay actions and file operations:

### Selection & Path Operations
- **Single / Multi-Selection**: Mark items using `Space`; use `Shift-Space` to remove the last item marked and move back to it.
- **Yank / Cut Paths**: Mark paths for copy/move operations across directories.

### File Operations & Overlays
- **`a` Create File/Folder**: Create directories or files on the fly.
- **`r` Rename**: Rename files or directories with a live inline overlay.
- **`d` Delete / Move to Trash**: Confirm deletion with undo history.
- **`z` / `Z` Zip / Unzip**: Create an archive from the selected item(s), or extract the selected archive.
- **`D` Drag-and-Drop with ripdrag**: Send the selected item(s) to `ripdrag`; install the `ripdrag` command and drag the files from its window to the destination application.

---

## 4. Preset Configuration (`~/.config/matchmaker/presets/jump.toml`)

Below is the optimized Jump Mode configuration:

```toml
[ui]
nav_mode = true
nav_bar = "Plain"
nav_color = "Black"

[matcher]
sort = "smart"
sort_cap = 2000
depth_penalty = 15
frecency = true
frecency_weight = 2
typo_tolerance = true
dir_first = true

[results]
reverse = false
icons = true
symlink_target = true
symlink_target_style.fg = "cyan"

[previewer]
debounce_ms = 25
delay_clear = true

[preview]
media = true

[preview.border]
title_fg = "cyan"
color = "blue"
type = "Rounded"

[[preview.layout]]
command = "p={1}; p=\"${p/#\\~/$HOME}\"; if [ -d \"$p\" ]; then eza --tree --level=2 --icons --git-ignore --color=always \"$p\"; else bat --style=numbers --color=always --line-range=:300 \"$p\"; fi"
side = "right"
percentage = 80

[binds]
"@reloadnext" = "ReloadNext"
"ctrl-z" = "@reloadnext"
"ctrl-j" = "Down"
"ctrl-k" = "Up"

[ui.nav_binds]
"ctrl-z" = "@reloadnext"
"l" = ["ChDir({=})", "@reload_local"]
"h" = ["ChDir(..)", "@reload_local"]

[breadcrumb]
show = true
separator = "/"
style.fg = "Cyan"
style.modifier = "BOLD"
separator_style.fg = "Cyan"

[start]
additional_commands = [
    "",
    "fd -H -E.git -Enode_modules -E.cache -Etarget -Edist -Ebuild --strip-cwd-prefix . 2>/dev/null",
    "mm list --dirs 2>/dev/null | sed -E \"s#^$HOME(/|$)##\" | grep -v \"^$\""
]

[start.command]
command = '''
IGNORE="-E.git -Enode_modules -E.cache -Etarget -Edist -Ebuild -E__pycache__ -E.venv -Evenv -Eenv -Evendor -E.gradle -E.npm -E.pnpm-store -E.next -Eout -Ecoverage -E.tox -E.mypy_cache -E.pytest_cache -E.cargo -E.rustup -E.mozilla -E.thunderbird"

if command -v fd >/dev/null 2>&1; then
    fd -H $IGNORE --strip-cwd-prefix --min-depth 1 --max-depth 1 . 2>/dev/null
    fd -H $IGNORE --strip-cwd-prefix --min-depth 2 . 2>/dev/null
else
    find .
fi
'''
```
