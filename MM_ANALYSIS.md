# Matchmaker (mm) - Deep Analysis & Feature Report

## 1. What is Matchmaker (mm)?
Matchmaker (`mm`) is a blazing-fast, highly configurable, and intuitive fuzzy searcher written in Rust. Taking inspiration from `fzf`, it reimagines the terminal search experience with a focus on rich UI, responsiveness, and complex workflow building. 

At its core, it leverages `nucleo` for matching and provides a robust feature set including:
- Interactive, multi-layout preview panels with scrolling and text wrapping.
- Multi-column input splitting, filtering, and highlighting.
- Powerful templating and semantic trigger systems for actions.
- Full customizability via a typed TOML config or inline CLI flags.

The `fecavmi` branch introduces a paradigm shift by extending `mm` beyond a simple fuzzy finder into a **modal terminal navigator and file manager**.

---

## 2. What Works (Current Features in `fecavmi` branch)

The `fecavmi` branch (and the `features` branch it's based on) cleanly layers several powerful additions on top of the upstream core:

### Navigation Mode (`--nav` / `--ui-fm`)
- **Modal Split:** Decouples the keyboard focus between the input query bar and the results list (toggled via `Tab`). 
- **Navigation Indicators:** Visual cues like a blinking cursor, configurable focus colors, and custom prompts inform the user of the current mode.

### Integrated File Manager Overlays (`fm.rs`)
- **Core Operations:** Native overlays to Create (`a`), Delete (`d`), and Rename (`r`) files/directories.
- **Archive Support:** Built-in ability to Zip (`z`) and Unzip (`Z`) archives dynamically.
- **Clipboard System:** Yank (`y`), Cut (`x`), and Paste (`p`) functionality that tracks selected paths and highlights them in the UI.
- **Robust Undo/Redo:** A tracked history stack that allows reverting and redoing file operations seamlessly.

### UI & UX Polish
- **Nerd Font Icons:** Eza-style icons mapped to file types and directories.
- **Symlink Targets:** Inline resolution and display of symlink targets.
- **Unified Color System:** A composable `--color` flag (fzf-style) to define granular UI colors (e.g., `hl-fg`, `list-border`, `yank`).
- **Drag-to-Resize:** Mouse drag support on the preview gap to resize the preview panel on the fly.
- **Inline Match Status:** Cleanly integrates the match/total status into the input bar to save vertical space.

---

## 3. What Could Be Improved

Based on the `TODO.md`, `PLAN.md`, and current architecture, there are several areas for optimization:

### Performance & Responsiveness
- **Preview Caching & Debouncing:** Offloading large previews to disk, and caching/debouncing the preview generation to prevent UI stuttering when scrolling quickly through large files.
- **Async File Operations:** Ensure that heavy file manager operations (like copying large directories or extracting huge zips) do not block the main event loop. If they currently block, migrating them to background Tokio tasks with progress updates would be ideal.
- **Rendering Optimizations:** Implement a non-grapheme-aware rendering path for speed, and optimize `make_table` for unaligned headings to reduce allocation overhead.

### UI Enhancements
- **Adaptable Layouts:** Automatically adjust preview percentages or hide the preview entirely based on terminal size (responsive design).
- **Better Scrolling:** Fix edge cases with reverse scrolling and bottom padding so the view fills naturally.

### Architecture
- **Plugin Decoupling:** While `fm.rs` is a great addition, it currently lives in `matchmaker-cli`. Moving towards a proper plugin architecture or WASM/Lua scripting model would allow the community to build similar overlays without bloating the core CLI.

---

## 4. New Features & Inspiration (Looking at Yazi & Others)

To elevate Matchmaker to the next level and compete with tools like [Yazi](https://github.com/sxyazi/yazi) or `lf`, consider the following additions:

### Media Previews (Image / Video / PDF)
- **Kitty / Sixel / iTerm2 Support:** Integrate protocols to render images, video thumbnails, and PDF previews directly inside the Matchmaker preview panel. This is a killer feature in Yazi and Television.

### Advanced File Management
- **Bulk Renaming via Editor:** Allow users to select multiple files, press a hotkey to open their names in `$EDITOR`, and apply the renames upon saving and closing.
- **Trash Integration:** Instead of just moving files to a custom temp backup path on Delete, integrate with the OS Trash system (e.g., using the `trash` crate) for safer and standard file removal.
- **Archive Browsing:** Instead of only extracting archives, allow `ChDir` or "drilling down" into `.zip` or `.tar.gz` files to browse and extract specific files.

### Navigation & Context
- **Multi-Tab / Workspaces:** Allow multiple tabs or split panes to navigate different directories simultaneously, making copy/paste operations between them much easier.
- **Git Integration:** Display git status indicators (e.g., `[M]`, `[?]`, `[A]`) as an extra column or prefix in the results list for files within a git repository.
- **Cross-Directory Selections:** Ensure that files yanked or selected in one directory remain in the clipboard/selection pool when using `ChDir` to navigate elsewhere, allowing users to collect files from multiple places before pasting.

### Extensibility
- **Lua Plugin Engine:** Yazi's massive success is largely due to its Lua plugin system. Adding a scripting engine (like `mlua`) would allow users to write custom previewers, preloaders, and custom keybind actions without recompiling Matchmaker.
