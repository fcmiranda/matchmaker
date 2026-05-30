# Matchmaker — Feature Usage Guide

This document covers every feature added in the current fork, with concrete `mm` command
examples and the equivalent `config.toml` snippets.  Each section maps to one implemented
phase so you can cross-reference with `PLAN.md`.

---

## Table of Contents

1. [Sort Input (`--sort`)](#1-sort-input---sort)
2. [Nerd Font Icons (`--icons`)](#2-nerd-font-icons---icons)
3. [Symlink Target Display (`--symlink-target`)](#3-symlink-target-display---symlink-target)
4. [Preview Title](#4-preview-title)
5. [Inline Match Status (`status_inline`)](#5-inline-match-status-status_inline)
6. [Preview Gap and Drag-to-Resize](#6-preview-gap-and-drag-to-resize)
7. [Selected-Row Highlight Styling](#7-selected-row-highlight-styling)
8. [Yank Prefix Style and `FmSetYankPaths`](#8-yank-prefix-style-and-fmsetyankpaths)
9. [Unified `--color` Flag](#9-unified---color-flag)
10. [User Config Overlay](#10-user-config-overlay)
11. [Default Bind Changes](#11-default-bind-changes)
12. [Toggle Advances Cursor](#12-toggle-advances-cursor)
13. [Customizable Spinners and Multi-Select Markers](#13-customizable-spinners-and-multi-select-markers)
14. [Combining Features — Real-World Recipes](#14-combining-features--real-world-recipes)
15. [Media Previews (`--media` / `preview.media`)](#15-media-previews---media--previewmedia)
16. [Header Grouping (`--group-prefix`)](#16-header-grouping---group-prefix)

---

## 1. Sort Input (`--sort`)

**What it does:** Sorts stdin lines alphabetically before they are fed into the picker.
Only operates on piped input; has no effect when `start.command` is used.

### CLI

```bash
# Sort a plain list
printf 'zebra\napple\nmango\n' | mm --sort

# Sort with a custom separator (NUL-separated input)
find . -print0 | mm --sort i '\0'

# Sort combined with other flags
printf 'c.txt\na.txt\nb.txt\n' | mm --sort --icons
```

### Config (`~/.config/matchmaker/config.toml` or `-o`)

```toml
[start]
sort = true
```

### CLI override syntax

```bash
# Override key: start.sort
printf 'z\na\nb\n' | mm start.sort=true
# or using the short alias:
printf 'z\na\nb\n' | mm s.sort=true
```

---

## 2. Nerd Font Icons (`--icons`)

**What it does:** Prepends a Nerd Font file-type icon to the first column of every row.
The icon is derived from the file name / extension.  Requires a Nerd Font in your terminal.

### CLI

```bash
# File picker with icons
mm --icons

# Combine with fd for a fast file picker
fd --strip-cwd-prefix | mm --icons

# Icons + preview
mm --icons px 'bat --color=always {1}'
```

### Config

```toml
[results]
icons = true
```

### CLI override syntax

```bash
mm results.icons=true
# or short form:
mm r.icons=true
```

---

## 3. Symlink Target Display (`--symlink-target`)

**What it does:** When the selected item is a symlink, appends ` → <target>` after the
name.  The arrow and target are styled with `results.symlink_target_style` (default:
`DarkGray` fg).

### CLI

```bash
# Show symlink targets in a file list
mm --symlink-target

# Change the style of the symlink annotation
mm --symlink-target --color symlink:Cyan

# Combine with icons
mm --icons --symlink-target
```

### Config

```toml
[results]
symlink_target = true
symlink_target_style.fg = "DarkGray"   # default — change as desired
```

### CLI override syntax

```bash
mm results.symlink_target=true results.symlink_target_style.fg=Cyan
```

---

## 4. Preview Title

**What it does:** Displays a title on the preview pane border.

- By default (no `title` key), it shows the current item name (first column).
- Set `title` for a static title, or use `{item}` interpolation.
- Title color is controlled by `--color preview-title:<Color>` (alias: `preview-label`).

### CLI

```bash
# Dynamic item title with yellow label
mm p.l.type=Plain --color 'preview-title:Yellow'

# Static title
mm p.l.type=Plain p.l.title='Preview'

# Interpolated title
mm p.l.type=Plain p.l.title='File: {item}'
```

### Config

```toml
[[preview.layout]]
command = "bat --color=always {1}"
side = "right"
percentage = 40
border.type = "Plain"
title = "File: {item}"       # optional; omit for default dynamic item name
```

```bash
# Color the title via --color
mm p.l.border.type=Plain p.l.title='File: {item}' --color 'preview-title:Yellow'
```

---

## 5. Inline Match Status (`status_inline`)

**What it does:** Collapses the status line into the right side of the query bar instead
of occupying a separate row.  The status count is right-aligned inside the input widget.

### CLI

```bash
# Enable inline status
mm query.status_inline=true

# Short form via the query alias
mm q.status_inline=true
```

### Config

```toml
[query]
status_inline = true
```

> When `status_inline = true` the standalone status row is hidden (`height = 0`) and the
> count (e.g. `42/100`) appears inside the query bar on the right-hand side.

---

## 6. Preview Gap and Drag-to-Resize

**What it does:** Inserts a configurable gap (in terminal columns / rows) between the
results pane and the preview pane.  The gap is a drag handle: hover to highlight it, then
click-and-drag to resize the preview at runtime.

### CLI

```bash
# Set a 2-cell gap for a right-side preview
mm p.l.gap=2

# Disable the gap (gap == 0 falls back to border-edge detection)
mm p.l.gap=0
```

### Config

```toml
[[preview.layout]]
side = "right"
percentage = 40
gap = 2          # default is 1; set 0 to disable the drag handle
```

> **Drag behaviour:** when the cursor is over the gap the strip turns `DarkGray`.  Press
> and hold the primary mouse button on the gap, then drag left/right (for a right-side
> preview) to resize.

---

## 7. Selected-Row Highlight Styling

**What it does:** Multi-selected rows (those toggled with `tab` / `shift-backtab`) receive
distinct styles separate from the cursor row.

| Field | Default | Applies to |
|---|---|---|
| `results.selected_style` | `BOLD` modifier | entire row background / text of a selected non-current row |
| `results.selected_prefix_style` | `Cyan` fg, `BOLD` modifier | prefix glyph (`▌`) of a selected non-current row |

### CLI

```bash
# Cyan background for selected rows
mm results.selected_style.bg=Cyan results.selected_style.fg=Black

# Bold yellow prefix for selected rows
mm results.selected_prefix_style.fg=Yellow results.selected_prefix_style.modifier=BOLD

# Use the --color shorthand (see Section 9)
mm --color 'selected-fg:White,selected-bg:DarkGray,selected-prefix:Yellow'
```

### Config

```toml
[results]
selected_style.fg         = "White"
selected_style.bg         = "DarkGray"
selected_style.modifier   = "BOLD"

selected_prefix_style.fg       = "Cyan"
selected_prefix_style.modifier = "BOLD"
```

---

## 8. Yank Prefix Style and `FmSetYankPaths`

**What it does:** Marks a set of paths with a dedicated "yank" prefix style (default:
`Yellow` fg, `BOLD`).  Paths are registered via the `FmSetYankPaths` action, which accepts
a newline-separated list of col-0 values.

Priority order for the prefix glyph: **yank > selected > default**.

### Configuring the colour

```bash
# Via --color flag
mm --color 'yank:Magenta'

# Via config override
mm results.yank_prefix_style.fg=Magenta results.yank_prefix_style.modifier=BOLD
```

```toml
[results]
yank_prefix_style.fg       = "Yellow"   # default
yank_prefix_style.modifier = "BOLD"
```

### Invoking `FmSetYankPaths` from a bind

`FmSetYankPaths` takes a **newline-separated** string of col-0 paths.  The typical pattern
is to collect selected items into that string with a `Transform` or `ExecuteSilent` action,
then call `FmSetYankPaths`.

```bash
# Mark currently selected files as "yanked" on ctrl-y
mm b 'ctrl-y=Transform(printf "%s\n" {+1})|||FmSetYankPaths({MM_STORE})'
```

```toml
[binds]
# Use Transform to capture {+1} into MM_STORE, then apply yank highlight
"ctrl-y" = [
  "Store({+1})",
  "FmSetYankPaths({MM_STORE})",
]

# Clear yank highlight
"ctrl-shift-y" = "FmSetYankPaths()"
```

> `FmSetYankPaths()` with an empty string clears all yank marks.

---

## 9. Unified `--color` Flag

**What it does:** A single composable flag, inspired by fzf's `--color`, that maps named
colour keys onto config fields.  Multiple keys can be combined in one `--color` value,
comma-separated.  The flag can be repeated.

### Syntax

```
--color key:value[,key:value,...]
```

`value` accepts any ratatui color: named colors (`Red`, `DarkGray`, `LightCyan`, …),
hex (`#ff5f87`), and 256-palette indices (`200`).

### Available Keys

| Key | Alias | What it styles |
|---|---|---|
| `fg` | | Results text foreground |
| `bg` | | Results text background |
| `hl-fg` | `current-fg` | Cursor-row foreground |
| `hl-bg` | `current-bg` | Cursor-row background |
| `border` | | Global UI border colour |
| `label` | `title` | Global UI border title colour |
| `preview-border` | | Preview pane border colour |
| `preview-label` | `preview-title`, `preview-border-title` | Preview pane border title colour |
| `list-border` | | Results pane border colour |
| `list-label` | `list-title` | Results pane border title colour |
| `input-border` | | Query bar border colour |
| `input-label` | `input-title` | Query bar border title colour |
| `header-border` | | Header border colour |
| `header-label` | `header-title` | Header border title colour |
| `nav` | | Navigation-mode indicator colour (Phase 5) |
| `selected-fg` | | Selected-row foreground |
| `selected-bg` | | Selected-row background |
| `selected-prefix` | | Selected-row prefix glyph colour |
| `yank` | | Yank-marked row prefix colour |
| `symlink` | | Symlink target annotation colour |

### Examples

```bash
# Catppuccin Mocha palette — single --color call
mm --color 'fg:#cdd6f4,bg:#1e1e2e,hl-fg:#cba6f7,hl-bg:#313244,\
border:#6c7086,selected-fg:#cba6f7,selected-prefix:#89b4fa,\
yank:#f38ba8,symlink:#a6e3a1,preview-border:#45475a'

# Gruvbox accent highlights only
mm --color 'hl-bg:#3c3836,hl-fg:#ebdbb2,selected-prefix:#fabd2f,yank:#fb4934'

# Minimal tweak: red yanked items, green selected prefix
mm --color 'yank:Red,selected-prefix:Green'

# Multiple separate --color calls are merged left-to-right
mm --color 'border:Blue' --color 'hl-bg:DarkGray,selected-fg:White'

# Icons + symlink annotation with custom colour
mm --icons --symlink-target --color 'symlink:Cyan'
```

---

## 10. User Config Overlay

**What it does:** At startup, if `~/.config/matchmaker/config.toml` exists it is loaded
as a `PartialConfig` and merged on top of the built-in defaults — before any CLI overrides
are applied.  This gives you persistent personal defaults without touching the embedded
config.

An explicit `--config <path>` skips the overlay and loads the given file verbatim as the
full config.

### Personal config file

```toml
# ~/.config/matchmaker/config.toml

[results]
icons = true
symlink_target = true
symlink_target_style.fg = "DarkGray"

[query]
status_inline = true

[[preview.layout]]
gap = 2
```

### Preset overlays (`-o` / `--override`)

The `-o` flag layers an additional TOML file on top.  Relative paths without an extension
are resolved against the built-in `presets/` directory.

```bash
# Load the built-in git/status preset
mm -o git/status

# Layer your own override file on top of your personal config
mm -o ~/dotfiles/mm-work.toml

# Stack multiple overrides (applied left-to-right)
mm -o git/status -o ~/mm-dark-theme.toml
```

---

## 11. Default Bind Changes

Two default key bindings were updated in this fork:

| Key | Old action | New action |
|---|---|---|
| `ctrl-p` | `Up(10)` | `SwitchPreview` — toggle the active preview layout |
| `?` | `SwitchPreview(Some(0))` | `Help("")` — open the help pane |
| `shift-?` | — | `SwitchPreview(Some(0))` — show preview layout 0 |

### Override them back or remap

```bash
# Restore ctrl-p to scroll-up-10
mm b 'ctrl-p=Up(10)'

# Remap help to alt-h (already bound by default) and free ?
mm b '?=SwitchPreview' b 'alt-h=Help'

# Bind ? to cycle all preview layouts
mm b '?=CyclePreview'
```

```toml
[binds]
"ctrl-p" = "Up(10)"           # restore old behaviour
"?"      = "SwitchPreview"    # toggle preview
"alt-h"  = "Help"             # keep help accessible
```

---

## 12. Toggle Advances Cursor

**What it does:** After `Toggle` (tab / shift-backtab) the cursor automatically advances
one step in the same direction, so rapid multi-select flows feel like fzf: press `tab`
to select-and-move-down, `shift-backtab` to select-and-move-up.

This is a behaviour change with no config knob; it is always active.

```bash
# The default binds already express this:
#   tab          → [Toggle, Down(1)]
#   shift-backtab → [Toggle, Up(1)]
# No extra flags needed.

# If you prefer the old toggle-in-place behaviour, rebind:
mm b 'tab=Toggle' b 'shift-backtab=Toggle'
```

---

## 13. Customizable Spinners and Multi-Select Markers

**What it does:** Introduces high-performance time-based row spinners and customizable unselected markers.
- **Unselected Markers**: You can customize the prefix displayed for rows that are not selected (or have not yet been selected). By default, this is `"  "` (two spaces). You can also style this unselected prefix.
- **Customizable Spinners**: Rows with text starting with a designated `spinner_prefix` (e.g. `?`) will automatically strip that prefix and display an animated, time-based spinner frame instead of the standard selection prefix. The animation updates seamlessly via a highly optimized frame tick.

### Available Spinners
Supported named spinner styles:
- `dot` (default): ⣾ ⣽ ⣻ ⢿ ⡿ ⣟ ⣯ ⣷
- `line`: | / - \
- `jump`: ⢄ ⢂ ⢁ ⡁ ⡈ ⡐ ⡠
- `pulse`: █ ▓ ▒ ░
- `points`: ∙∙∙ ●∙∙ ∙●∙ ∙∙● ∙∙∙
- `meter`: ▱▱▱ ▰▱▱ ▰▰▱ ▰▰▰
- `hamburger`: ☱ ☲ ☴ ☲
- `ellipsis`:  . .. ...
- `globe`: 🌍 🌎 🌏
- `moon`: 🌑 🌒 🌓 🌔 🌕 🌖 🌗 🌘
- `monkey`: 🙈 🙉 🙊
- `arc`: ◜ ◠ ◝ ◞ ◡ ◟
- `nerd`: 󰇙
- `nerdarc`: ◜   ◝ ◞ ◡ ◟  
- `minidot`: ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏

### CLI

```bash
# Style unselected items with a custom prefix marker
mm results.unselected_prefix="-" --color unselected-prefix:DarkGray

# Direct matchmaker equivalent of:
# printf '@Building…\nReady item' | bfzf --spinner-prefix '@'
printf '@Building…\nReady item' | mm results.spinner_prefix='@' preview.show=false

# Use the earth (globe) spinner colored Cyan
printf '@Building…\nReady item' | \
  mm results.spinner_prefix='@' \
     results.spinner=globe \
     preview.show=false \
     --color spinner:Cyan

# Use the moon phases spinner colored Yellow
printf '@Building…\nReady item' | \
  mm results.spinner_prefix='@' \
     results.spinner=moon \
     preview.show=false \
     --color spinner:Yellow

# Use the meter loading bar spinner colored Green
printf '@Building…\nReady item' | \
  mm results.spinner_prefix='@' \
     results.spinner=meter \
     preview.show=false \
     --color spinner:Green

# Use the shy monkey emoji spinner colored Magenta
printf '@Building…\nReady item' | \
  mm results.spinner_prefix='@' \
     results.spinner=monkey \
     preview.show=false \
     --color spinner:Magenta

# Use the arc spinner styled Bold Blue
printf '@Building…\nReady item' | \
  mm results.spinner_prefix='@' \
     results.spinner=arc \
     preview.show=false \
     results.spinner_style.fg=Blue \
     results.spinner_style.modifier=BOLD
```

### Config

```toml
[results]
unselected_prefix = "-"
unselected_prefix_style.fg = "DarkGray"

spinner = "moon"
spinner_prefix = "?"
spinner_style.fg = "Yellow"
spinner_style.modifier = "BOLD"
```

---

## 14. Combining Features — Real-World Recipes

### File manager-style picker

```bash
mm \
  --icons \
  --symlink-target \
  --sort \
  --color 'selected-prefix:#89b4fa,yank:#f38ba8,symlink:#a6e3a1' \
  query.status_inline=true \
  p.l.gap=2 \
  p.l.border.type=Plain \
  p.l.title='File: {item}' \
  --color 'preview-label:Yellow' \
  b 'ctrl-y=FmSetYankPaths({+1})' \
  b 'ctrl-shift-y=FmSetYankPaths()'
```

### Catppuccin Mocha theme with all new options

```bash
mm \
  --icons --symlink-target --sort \
  --color 'fg:#cdd6f4,bg:#1e1e2e,hl-fg:#cba6f7,hl-bg:#313244,border:#6c7086,\
label:#cba6f7,preview-border:#45475a,preview-label:#89dceb,\
selected-fg:#cba6f7,selected-bg:#313244,selected-prefix:#89b4fa,\
yank:#f38ba8,symlink:#a6e3a1' \
  query.status_inline=true \
  p.l.gap=2
```

Persist as your user config:

```toml
# ~/.config/matchmaker/config.toml
[results]
icons            = true
symlink_target   = true
symlink_target_style.fg = "#a6e3a1"

selected_style.bg         = "#313244"
selected_style.fg         = "#cba6f7"
selected_prefix_style.fg  = "#89b4fa"
selected_prefix_style.modifier = "BOLD"
yank_prefix_style.fg      = "#f38ba8"
yank_prefix_style.modifier = "BOLD"

[query]
status_inline = true

[[preview.layout]]
gap             = 2
border.type     = "Plain"
border.color    = "#45475a"
title           = "File: {item}"
```

### Git status picker with yank highlights

```toml
# preset: git-status-yank.toml
[start]
command = "git status --short"
ansi = false

[results]
icons = true
yank_prefix_style.fg = "Red"
yank_prefix_style.modifier = "BOLD"

[[preview.layout]]
command = "git diff -- {2}"
side = "right"
percentage = 50
gap = 1

[binds]
"ctrl-y" = ["FmSetYankPaths({+2})", "ExecuteSilent(git add {+2})"]
"ctrl-shift-y" = "FmSetYankPaths()"
```

```bash
mm -o ./git-status-yank.toml --icons
```

### Sort + status inline + icons for a log browser

```bash
journalctl --no-pager -n 500 | \
  mm --sort \
     query.status_inline=true \
     results.icons=false \
     px 'echo {}' \
     --color 'selected-prefix:Cyan,hl-bg:DarkGray'
```

---

## 15. Media Previews (`--media` / `preview.media`)

**What it does:** Renders images, video thumbnails, and PDF previews directly inside the Matchmaker preview panel.

Matchmaker automatically queries your terminal capabilities using `from_query_stdio()` to determine the best rendering protocol (supporting Kitty graphics, Sixel, and iTerm2).

- **Images**: Decoded natively (no external tools required) via the `image` crate. Supports `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.bmp`, `.ico`, `.tiff`.
- **Videos**: Spawns `ffmpegthumbnailer` to extract a high-quality frame buffer (requires `ffmpegthumbnailer` installed). Supports `.mp4`, `.mkv`, `.avi`, `.mov`.
- **PDFs**: Spawns `pdftoppm` to extract the first page (requires `pdftoppm` installed). Supports `.pdf`.

### CLI

```bash
# Start Matchmaker with native media previews enabled
mm --media

# Alternatively, override the media key directly on the CLI
mm preview.media=true
# or using short alias:
mm p.media=true
```

### Config

```toml
[preview]
media = true # Enable terminal image protocols for media previews

[binds]
# Resize the preview panel dynamically
"alt-right" = "ExpandPreview(5)"
"alt-left" = "ShrinkPreview(5)"
```

---

## 16. Header Grouping (`--group-prefix`)

**What it does:** Organizes items in the results list under visual group headers. Any lines in the input stream starting with the specified prefix are rendered as non-selectable, bold group headers. The TUI cursor and multi-selection skip over these headers automatically.

### CLI

```bash
# Define group headers using the '#' prefix
printf "# Languages\nPython\nRust\nGo\n# Frameworks\nDjango\nAxum\nGin" | mm --group-prefix '#'

# Define group headers using a custom prefix (e.g. '---')
printf "--- Group A\nItem 1\nItem 2\n--- Group B\nItem 3" | mm --group-prefix '---'
```

---

## Quick Reference — All New Flags and Config Keys

### CLI flags

| Flag | Config key | Default |
|---|---|---|
| `--sort` | `start.sort` | `false` |
| `--icons` | `results.icons` | `false` |
| `--symlink-target` | `results.symlink_target` | `false` |
| `--media` | `preview.media` | `false` |
| `--group-prefix` | — | — |
| `--color key:val,...` | *(see table in §9)* | — |

### Config-only keys (use `mm key=value` to override on the CLI)

| Config path | Type | Default |
|---|---|---|
| `query.status_inline` | `bool` | `false` |
| `preview.layout[n].gap` | `u16` | `1` |
| `preview.layout[n].title` | `Option<String>` | `None` (dynamic current item name) |
| `results.selected_style` | `StyleSetting` | `modifier: BOLD` |
| `results.selected_prefix_style` | `StyleSetting` | `fg: Cyan, modifier: BOLD` |
| `results.unselected_prefix` | `string` | `"  "` |
| `results.unselected_prefix_style` | `StyleSetting` | — |
| `results.spinner_prefix` | `string` | `""` |
| `results.spinner` | `string` | `"dot"` |
| `results.spinner_style` | `StyleSetting` | — |
| `results.yank_prefix_style` | `StyleSetting` | `fg: Yellow, modifier: BOLD` |
| `results.symlink_target_style` | `StyleSetting` | `fg: DarkGray` |
| `ui.nav_color` | `Color` | `Reset` |

### `--color` shorthand keys

```
fg  bg  hl-fg  hl-bg  border  label
preview-border  preview-label
list-border  list-label
input-border  input-label
header-border  header-label
nav  selected-fg  selected-bg  selected-prefix  unselected-prefix  spinner  yank  symlink
```

### New actions (for use in `[binds]` or `mm b '...'`)

| Action | Argument | Description |
|---|---|---|
| `FmSetYankPaths(paths)` | Newline-separated col-0 strings | Mark paths with yank prefix style; empty clears all marks |
