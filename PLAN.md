# Implementation Plan

> Re-implement fork features on a clean upstream base.
> Order: easiest → hardest. Each phase builds on the previous.
> Principle: **additive only** — new fields, new variants, new files.
> Never patch existing logic paths when a handler or config flag suffices.

---

## Rename: "Focus Mode" → **Navigation Mode (`--nav`)**

**Suggestion: `--nav`**

The current `--ui-fm` name is misleading — the core mechanism is not a file
manager, it is a **modal split**: Tab switches keyboard input between the filter
bar (typing mode) and the results list (navigation mode). File manager
operations are a *plugin* on top of this split; they only activate when
`fm.rs` registers its overlays.

Rename mapping:

| Old flags | New syntax |
|---|---|
| `--ui-fm` | `--nav` |
| `--ui-fm-bar` | `--nav bar` |
| `--ui-fm-bar=plain` | `--nav bar:plain` |
| `--ui-fm-blink` | `--nav blink` |
| `--ui-fm-blink-slow` | `--nav blink:slow` |
| `--ui-fm-blink-rapid` | `--nav blink:rapid` |
| `--ui-fm-bold` | `--nav bold` |
| `--ui-fm-marker '>'` | `--nav marker:'>'` |
| `--ui-fm-notify` | `--nav notify` |
| `--ui-fm-color Yellow` | `--nav color:Yellow` (or `--color nav:Yellow`) |
| `--prompt-marker '[NAV] '` | `--nav prompt:'[NAV] '` |
| `--focus-bind 'h:ChDir(..)'` | `--nav-bind 'h:ChDir(..)'` |
| Config: `focus_*` | Config: `nav_*` |
| `Action::ToggleFocus` | `Action::ToggleFocus` (keep — already clear) |

**`--nav-bind` is intentionally a separate flag** — its values contain colons
(`h:ChDir(..)`) that would collide with the `property:value` parser.

Config alias `fm` → `nav` so existing TOML files keep working during
transition.

**Composed example** (before vs. after):

```sh
# Before (8 separate flags)
mm --ui-fm --ui-fm-bar --ui-fm-blink-slow --ui-fm-color Cyan \
   --ui-fm-bold --ui-fm-marker '>' --prompt-marker '[NAV] ' --ui-fm-notify

# After (one flag + one bind flag)
mm --nav bar blink:slow color:Cyan bold marker:'>' prompt:'[NAV] ' notify
```

---

## Color System Redesign (`--color`)

Replace individual `--ui-hl-fg`, `--ui-hl-bg`, `--ui-fm-color` flags with a
single composable flag matching fzf's syntax:

```sh
mm --color 'fg:#cdd6f4,bg:#1e1e2e'                        # base text
mm --color 'hl-fg:White,hl-bg:Black'                      # highlighted row
mm --color 'border:#585b70,label:#cba6f7'                 # main border
mm --color 'preview-border:#9999cc,preview-label:#ccccff' # preview pane
mm --color 'list-border:#669966,list-label:#99cc99'       # results list
mm --color 'input-border:#996666,input-label:#ffcccc'     # input bar
mm --color 'header-border:#6699cc,header-label:#99ccff'   # header
mm --color 'nav:#a6e3a1'                                   # nav indicator
mm --color 'selected-fg:#cdd6f4,selected-bg:#313244'      # selected rows
mm --color 'yank:#f9e2af'                                  # yank prefix
mm --color 'symlink:#89dceb'                               # symlink target
```

The flag can be repeated; later values override earlier ones. Each `key:value`
pair is parsed by a single `parse_color_spec(s)` function and mapped onto the
existing nested `StyleSetting` / `Color` config fields in `start.rs`. No new
config struct is needed — the TOML shape is unchanged.

### Color key → config field mapping

| Key | Config path |
|---|---|
| `fg` | `render.style.fg` |
| `bg` | `render.style.bg` |
| `hl-fg` | `render.results.current.fg` |
| `hl-bg` | `render.results.current.bg` |
| `border` | `render.ui.border.fg` |
| `label` | `render.ui.border.title_fg` |
| `preview-border` | `render.preview[*].border.fg` |
| `preview-label` | `render.preview[*].border.title_fg` |
| `list-border` | `render.results.border.fg` (if added) |
| `list-label` | `render.results.border.title_fg` (if added) |
| `input-border` | `render.query.border.fg` (if added) |
| `input-label` | `render.query.border.title_fg` (if added) |
| `header-border` | `render.header.border.fg` (if added) |
| `header-label` | `render.header.border.title_fg` (if added) |
| `nav` | `render.ui.nav_color` |
| `selected-fg` | `render.results.selected.fg` |
| `selected-bg` | `render.results.selected.bg` |
| `selected-prefix` | `render.results.selected_prefix.fg` |
| `yank` | `render.results.yank_prefix_style.fg` |
| `symlink` | `render.results.symlink_target_style.fg` |

**Implementation:** `apply_color_spec(config: &mut Config, spec: &str)` in
`matchmaker-cli/src/color.rs` (new, ~60 lines). Called from `start.rs` after
all other overrides.

**Parsing note:** `ratatui::style::Color` already implements `FromStr` and
accepts both named colours (`Yellow`, `LightCyan`) and hex (`#RRGGBB`), and
256-colour indices (`214`). Reuse it directly — zero extra parsing code.

---

## Phases

---

### Phase 1 — Trivial fixes (no design, pure additions)

These are single-line or two-line changes with no dependencies.

#### 1a. Toggle advances cursor

**File:** `matchmaker-lib/src/render/mod.rs`

After the `Action::Toggle` arm dispatches `selections.toggle(item)`, add:

```rust
results.cursor_next();
```

This makes repeated Space-bar presses select consecutive items naturally.
Entirely additive — `cursor_next` already exists.

#### 1b. Key bind defaults

**File:** `matchmaker-lib/src/binds.rs`

Two changes in `BindMap::default_binds()`:

```rust
// add
key!(ctrl-p) => Action::SwitchPreview(None),
// change
key!('?') => Action::Help("".to_string()),  // was SwitchPreview(None)
```

`ctrl-p` was unbound; `?` moves from preview-toggle to help overlay.

---

### Phase 2 — Standalone additive features

Each item adds new config fields (defaulting to off) and isolated rendering
code. No interaction with each other or with nav mode.

#### 2a. Preview title

**Files:** `matchmaker-lib/src/ui/preview.rs`, `matchmaker-lib/src/config.rs`

1. Add `title_fg: Color` field to `BorderSetting` (default: `Color::Reset`).
2. Add `block_with_title<'a>(&'a self, title: Option<&'a str>) -> Block<'a>`
   method on `BorderSetting`. Falls back to `self.title` when `None`.
3. Add `title: Option<String>` field and `set_title(Option<String>)` method
   to `PreviewUI`.
4. In the render loop, after `preview_ui.visible()` check, call
   `preview_ui.set_title(current_item_first_column_text)`.
5. Replace `self.border().as_block()` → `self.border().block_with_title(self.title.as_deref())`
   inside `PreviewUI::draw`.

No new CLI flag needed — always on when a preview is open.

#### 2b. Sort flag

**Files:** `matchmaker-cli/src/clap.rs`, `matchmaker-cli/src/start.rs`

1. Add `#[arg(long)] pub sort: bool` to `Cli`.
2. Add `"--sort"` to the boolean flag allowlist in `first_pass`.
3. In `start()`, when `sort` is true:
   - Set `config.matcher.worker.sort_threshold = u32::MAX`.
   - Collect all input into `Vec<u8>`, split on separator, `sort_unstable`,
     re-join, inject via `std::io::Cursor`.

Self-contained: no lib changes required.

#### 2c. Inline match status

**Files:** `matchmaker-lib/src/config.rs`, `matchmaker-lib/src/ui/input.rs`,
`matchmaker-lib/src/render/mod.rs`, `matchmaker-lib/src/ui/mod.rs`

1. Add `pub status_inline: bool` to `QueryConfig` (default: `true`).
2. Extract `ResultsUI::status_line() -> Line<'_>` from `make_status` —
   returns the formatted spans without width-fit or indent.
3. Change `QueryUI::make_input` signature to accept
   `right_label: Option<Line<'_>>, area_width: u16`. When `right_label` is
   `Some`, pad with spaces then append the spans right-aligned.
4. In `render/mod.rs`, pass `picker_ui.results.status_line()` as
   `right_label` when `status_inline` is true; hide the status row
   (`Constraint::Length(0)`) in `PickerUI::layout`.

#### 2d. Preview gap and drag-to-resize

**Files:** `matchmaker-lib/src/config.rs`, `matchmaker-lib/src/ui/mod.rs`,
`matchmaker-lib/src/ui/preview.rs`, `matchmaker-lib/src/render/mod.rs`

1. Add `pub gap: u16` to `PreviewLayout` (default: `1`). Change `max` default
   to `i16::MAX`.
2. `PreviewLayout::split` returns `[Rect; 3]` → `[preview, picker, gap]`.
   The gap chunk is *not* rendered, only returned for hit-testing.
3. Add `setting_mut() -> Option<&mut PreviewSetting>` to `PreviewUI`.
4. Widen `State::layout` to `[Rect; 6]`: add `gap_area`, `pane_area` slots.
5. In render loop:
   - Track `let mut dragging = false` and `mouse_hover: Option<Position>`.
   - `MouseDown` on `gap_area` → `dragging = true`.
   - `MouseDrag` while dragging → recalculate `setting.layout.percentage`
     from cursor position / `pane_area` dimensions for all four sides.
   - `MouseUp` → `dragging = false`.
   - `MouseMove` → update `mouse_hover`; render `DarkGray` block over
     `gap_area` when hovered.

#### 2e. Eza-style icons

**Files:** `matchmaker-lib/src/config.rs`, `matchmaker-lib/src/ui/results.rs`

1. Add `pub icons: bool` to `ResultsConfig` (default: `false`). Add CLI flag
   `--icons` with parse alias `icons → results.icons`.
2. Add private `fn icon_for_name(name: &str) -> (char, Color)` in
   `results.rs`. Checks `is_dir()`, `is_symlink()`, known filenames
   (`Cargo.toml`, `package.json`, `Makefile`), then extension. Falls back to
   `\u{f15b}`.
3. When `config.icons` is true, prepend a `Span::styled(format!("{icon} "), …)`
   as `line.spans.insert(1, …)` after the prefix span in the render loop.
4. `indentation()` returns `multi_prefix.width() + if config.icons { 2 } else { 0 }`.

#### 2f. Symlink target display

**Files:** `matchmaker-lib/src/config.rs`, `matchmaker-lib/src/ui/results.rs`

1. Add to `ResultsConfig`:
   ```rust
   pub symlink_target: bool,           // default: false
   pub symlink_target_style: StyleSetting,  // default: fg=DarkGray
   ```
2. During row render, when `config.symlink_target` is true, call
   `std::fs::read_link(name)` and append `" \u{f061} {target}"` as a `Span`
   to the first line of the first column.
3. No filesystem call when `symlink_target` is false — zero cost when off.

---

### Phase 3 — Unified `--color` flag

**New file:** `matchmaker-cli/src/color.rs`

```rust
/// Apply a single `--color key:value,key:value` spec to config.
pub fn apply_color_spec(config: &mut Config, spec: &str) { … }
```

- Split on `,`, parse each `key:value` or `key=#rrggbb` pair.
- Use `ratatui::style::Color::from_str` for all value parsing.
- Map key strings to config field paths as per the table above.

**`matchmaker-cli/src/clap.rs`:** Add:

```rust
/// fzf-compatible color spec. Repeatable.
/// e.g. --color 'hl-fg:White,hl-bg:Black' --color 'nav:#a6e3a1'
#[arg(long = "color")]
pub color: Vec<String>,
```

**`matchmaker-cli/src/start.rs`:** After all other overrides, apply:

```rust
for spec in &cli.color {
    apply_color_spec(&mut config, spec);
}
```

This replaces `--ui-hl-fg`, `--ui-hl-bg`, and `--ui-fm-color` (now `nav`
key). Keep the old flags behind a `#[deprecated]` annotation for one release
cycle, or remove immediately.

---

### Phase 4 — Row styling additions

These use config fields from Phase 3's color keys.

#### 4a. Selected row styling

**Files:** `matchmaker-lib/src/config.rs`, `matchmaker-lib/src/ui/results.rs`

1. Add to `ResultsConfig`:
   ```rust
   pub selected:        StyleSetting,  // default: BOLD
   pub selected_prefix: StyleSetting,  // default: fg=Cyan, BOLD
   ```
2. In all three render paths (normal, wrap, horizontal scroll), apply:
   - Row text: `config.selected` when `selector.contains(item)` and not the
     current row.
   - Prefix span: `Span::styled(marker_prefix, config.selected_prefix)` when
     selected.

#### 4b. Yank/cut prefix highlight

**Files:** `matchmaker-lib/src/config.rs`, `matchmaker-lib/src/ui/results.rs`

1. Add to `ResultsConfig`:
   ```rust
   pub yank_prefix_style: StyleSetting,  // default: fg=Yellow, BOLD
   ```
2. Add to `ResultsUI`:
   ```rust
   pub yank_paths: std::collections::HashSet<String>
   ```
3. During prefix span construction: if `yank_paths.contains(&row_name)`,
   use `yank_prefix_style` (overrides `selected_prefix`).
4. `FmSetYankPaths(Vec<String>)` action variant writes into
   `state.picker_ui.results.yank_paths`. Cleared (empty vec) after paste.

---

### Phase 5 — Navigation Mode (`--nav`)

The core modal split. Self-contained in lib; no dependency on fm.rs.

#### 5a. New action variants

**File:** `matchmaker-lib/src/action.rs`

Add two variants to `Action<A>`:

```rust
/// Toggle keyboard focus: Input ↔ Results. No-op when nav_mode = false.
ToggleFocus,
/// Change the process working directory. Supports {} placeholders.
ChDir(String),
```

Register in `enum_from_str_display!` — `ToggleFocus` in the unit arm, `ChDir`
in the tuple arm.

Add `impl Action<NullActionExt> { pub fn from_null<A: ActionExt>(self) -> Action<A> { … } }`.
This is a plain `match` with one arm per variant; the `Custom(x)` arm is
`match x {}` (statically unreachable).

#### 5b. `ChDir` interrupt and handler

**Files:** `matchmaker-lib/src/message.rs`, `matchmaker-lib/src/matchmaker.rs`

1. Add `Interrupt::ChDir` variant.
2. Add `Matchmaker::register_chdir_handler(formatter)`: on `Interrupt::ChDir`,
   expand `{}` placeholders via `use_formatter`, call
   `std::env::set_current_dir`. Warn (do not panic) on error.
3. Call `mm.register_chdir_handler(cli_formatter.clone())` in `start.rs`
   alongside the existing execute/become handlers.

#### 5c. `BlinkRate` enum

**File:** `matchmaker-lib/src/config_types.rs`

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BlinkRate { Slow, #[default] Normal, Rapid }
impl BlinkRate { pub fn ticks(self) -> u8 { … } }
```

`Slow = 90`, `Normal = 30`, `Rapid = 10` (ticks at 60 Hz = half-cycle length).

#### 5d. Nav mode config

**File:** `matchmaker-lib/src/config.rs`, struct `UiConfig`

Add fields:

```rust
pub nav_mode:       bool,                                    // --nav
pub nav_color:      Color,                                   // --color nav:
pub nav_blink:      bool,                                    // --nav-blink
pub nav_blink_rate: BlinkRate,                               // --nav-blink-slow / --nav-blink-rapid
pub nav_bold:       bool,                                    // --nav-bold
pub nav_bar:        Option<BorderType>,                      // --nav-bar
pub nav_marker:     String,                                  // --nav-marker
pub nav_prompt:     String,                                  // --nav-prompt
pub nav_binds:      HashMap<String, Actions<NullActionExt>>, // --nav-bind
```

`nav_binds` default entries (inserted in `UiConfig::default()`):

```
j  → Down(1)
k  → Up(1)
l  → ChDir("{=}") + Reload("")
h  → ChDir("..") + Reload("")
```

Config alias `fm` → `nav` for backward compatibility.

#### 5e. Focus state in render

**File:** `matchmaker-lib/src/render/state.rs`

```rust
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus { #[default] Input, Results }

// New fields on State:
pub focus: Focus
pub(crate) nav_blink: bool      // current blink phase
pub(crate) nav_tick:  u8        // tick counter
```

#### 5f. `ToggleFocus` dispatch

**File:** `matchmaker-lib/src/render/mod.rs`

In the `Action::ToggleFocus` arm:
1. Flip `state.focus`. Reset `nav_tick = 0`, `nav_blink = true`.
2. If `nav_prompt` is non-empty: call `picker_ui.query.set_prompt(Some(…))` when
   switching to Results, `None` when returning to Input.

Blink tick in the per-frame section:

```rust
if ui.config.nav_mode {
    state.nav_tick = state.nav_tick.wrapping_add(1);
    if state.nav_tick >= ui.config.nav_blink_rate.ticks() {
        state.nav_tick = 0;
        state.nav_blink = !state.nav_blink;
    }
}
```

#### 5g. `apply_nav_binds` pre-processing

**File:** `matchmaker-lib/src/render/mod.rs`

```rust
fn apply_nav_binds<A: ActionExt>(
    buffer: &mut Vec<RenderCommand<A>>,
    initial_focus: Focus,
    nav_binds: &HashMap<String, Actions<NullActionExt>>,
    overlay_active: bool,
) { … }
```

- Skip when `overlay_active` (overlay has its own text input).
- Simulate `ToggleFocus` events in the batch to track effective focus.
- When `focus == Results`: replace `Action::Char(c)` with the bound actions
  via `.from_null()`; drop unbound chars silently.
- After translation, re-run `apply_aliases` so `Semantic("fm_yank")` etc.
  resolve before event dispatch.

#### 5h. Visual indicators

Pass `NavFocusInfo { focused, blink_phase, color, do_blink, bold, bar, marker }`
(small stack struct) into `render_input` and `render_results`.

`NavFocusInfo::indicator_color() -> Option<Color>`:
- `Some(color)` when focused and (not blinking or blink phase on).
- `Some(DarkGray)` when focused and blinking and blink phase off.
- `None` when not focused.

In `render_results`: draw `Borders::LEFT` block with configurable `BorderType`
and colour; render `marker` span at the cursor row y-offset.

In `render_input`: draw the same bar on the input area; skip
`frame.set_cursor_position` when Results pane is focused.

#### 5i. CLI flags — grouped `--nav` syntax

All nav visual options collapse into a **single flag** that accepts zero or
more `property` / `property:value` tokens:

```sh
mm --nav                                          # on, all defaults
mm --nav bar                                      # + thick bar
mm --nav bar:plain blink:slow bold                # bar style + rate + bold
mm --nav bar color:#a6e3a1 marker:'>' prompt:'[NAV]' notify
```

**`--nav-bind` stays separate** — its values already contain colons
(`h:ChDir(..)`) which would collide with the `property:value` parser.

**`matchmaker-cli/src/clap.rs`:**

```rust
/// Navigation mode (modal split: Tab toggles input ↔ results focus).
///
/// Accepts zero or more property tokens:
///
///   Boolean:  bar  blink  bold  notify
///   Valued:   bar:STYLE  blink:RATE  marker:CHAR  prompt:TEXT  color:VALUE
///
/// STYLE : plain | thick (default) | rounded | double
/// RATE  : slow | normal (default) | rapid
/// CHAR  : any Unicode character or short string
/// VALUE : named colour (Yellow) or hex (#RRGGBB) or 256-index
///
/// Examples:
///   --nav
///   --nav bar blink:slow
///   --nav bar:plain color:#a6e3a1 marker:'>' bold
///   --nav bar blink:rapid color:Cyan prompt:'[NAV] ' notify
#[arg(long = "nav", num_args = 0.., value_name = "PROP")]
pub nav: Option<Vec<String>>,

/// Navigation-mode key binding. Repeatable.
/// Format: "char:Action[;Action2…]"   (semicolons inside parens are not split)
/// Example: --nav-bind 'h:ChDir(..)' --nav-bind 'l:ChDir({=});Reload'
#[arg(long = "nav-bind")]
pub nav_bind: Vec<String>,
```

**`matchmaker-cli/src/nav.rs`** (new, ~60 lines):

```rust
/// Parse `--nav` property tokens and apply them to config.
pub fn apply_nav_props(props: &[String], config: &mut Config) {
    config.render.ui.nav_mode = true;
    for prop in props {
        match prop.split_once(':') {
            None => match prop.as_str() {
                "bar"    => config.render.ui.nav_bar    = Some(BorderType::Thick),
                "blink"  => config.render.ui.nav_blink  = true,
                "bold"   => config.render.ui.nav_bold   = true,
                "notify" => config.render.ui.nav_notify = true,
                _        => eprintln!("warning: unknown --nav property '{prop}'"),
            },
            Some(("bar",    s)) => config.render.ui.nav_bar        = Some(parse_border_type(s)),
            Some(("blink",  s)) => {
                config.render.ui.nav_blink = true;
                config.render.ui.nav_blink_rate = parse_blink_rate(s);
            }
            Some(("marker", s)) => config.render.ui.nav_marker     = s.to_string(),
            Some(("prompt", s)) => config.render.ui.nav_prompt      = s.to_string(),
            Some(("color",  s)) => apply_nav_color(s, config),
            Some((k, _))        => eprintln!("warning: unknown --nav property '{k}'"),
        }
    }
}
```

`apply_nav_color` delegates to the same `ratatui::style::Color::from_str`
used by the `--color` flag.

**`matchmaker-cli/src/start.rs`:**

```rust
if let Some(props) = &cli.nav {
    apply_nav_props(props, &mut config);
}
```

`first_pass` recognises `--nav` as a multi-value flag (stops at the next
`--flag`), so it must appear in the multi-value allowlist rather than the
boolean one.

When `nav_mode` is true, `enter()`:
- Inserts default FM binds with `.entry().or_insert()`.
- Force-binds `Tab` → `ToggleFocus`.
- Registers FM overlays and `CursorChange` handler (Phase 6).

`split_nav_bind_actions(s: &str) -> Vec<&str>`: splits on `;` while
respecting parenthesis depth, so `Execute(a;b)` is not split.

#### 5j. Parse aliases and `--nav-bind` / `--bind` sugar

**File:** `matchmaker-cli/src/parse.rs`

New ALIASES (TOML override path shortcuts):

```rust
("nav",        "ui.nav_mode"),
("nav-color",  "ui.nav_color"),   // also via --color nav:
("nav-bar",    "ui.nav_bar"),
("nav-marker", "ui.nav_marker"),
("nav-prompt", "ui.nav_prompt"),
("icons",      "results.icons"),
// backward-compat
("fm",         "ui.nav_mode"),
("ui-fm-color","ui.nav_color"),
```

Special-case `nav-bind "char:action"` in `get_pairs()` → expands to
`(["ui", "nav_binds", char], action)`.

Special-case `bind "key:action"` → expands to `(["binds", key], action)` after
`valid_key()` check.

Strip leading `--` from path tokens.

#### 5k. `binds.rs` test

```rust
#[test]
fn tab_trigger_matches_real_keypress() { … }
```

Verifies `key!(tab).into(): Trigger` equals the trigger produced by a real
`crossterm::event::KeyCode::Tab` event. Prevents silent regressions if the
`crokey` dependency ever changes Tab's representation.

---

### Phase 6 — File Manager (`--nav` required)

File manager operations are a **plugin** on top of nav mode. They activate
only when `nav_mode = true` and `fm.rs` registers its overlays. The lib crate
has no knowledge of `fm.rs`; all FM logic lives in `matchmaker-cli`.

#### 6a. Core types — `matchmaker-cli/src/fm.rs` (new file)

```rust
pub type CurrentItem = Arc<Mutex<Option<String>>>;
pub type UndoStack   = Arc<Mutex<Vec<UndoAction>>>;
pub type Clipboard   = Arc<Mutex<Option<FmClipboard>>>;

pub struct FmClipboard { pub items: Vec<PathBuf>, pub op: ClipOp }
pub enum ClipOp { Copy, Cut }

pub enum UndoAction {
    DeletedFile { original: PathBuf, backup: PathBuf },
    CreatedFile { path: PathBuf },
    Renamed     { from: PathBuf, to: PathBuf },
    Copied      { dest: PathBuf },
    Moved       { from: PathBuf, to: PathBuf },
}

pub fn apply_undo(action: &UndoAction) -> std::io::Result<()>
pub fn copy_into(src: &Path, dest_dir: &Path) -> std::io::Result<()>
pub fn move_into(src: &Path, dest_dir: &Path) -> std::io::Result<()>
```

#### 6b. Overlays

Four structs implementing `matchmaker::OverlayUI` (index 0–3):

| Index | Key | Struct | Behaviour |
|---|---|---|---|
| 0 | `d` | `DeleteOverlay` | Confirm dialog → move to temp backup path → push `UndoAction::DeletedFile` |
| 1 | `a` | `CreateOverlay` | Text input → `fs::create_dir_all` or `File::create` → push `UndoAction::CreatedFile` |
| 2 | `r` | `RenameOverlay` | Text input pre-filled with filename → `fs::rename` → push `UndoAction::Renamed` |
| 3 | `z` | `ZipOverlay`    | Confirm dialog → compress selected/current items to a .zip archive |
| 4 | `Z` | `UnzipOverlay`  | Confirm dialog → detect format (zip/tar.gz/tar.bz2/tar.xz/gz/bz2/tar) → extract |

Each overlay:
- Sends `Action::Reload("")` on success.
- Sends `Action::Reload("")` is the *only* lib-level side-effect — no direct
  access to config or state.
- Uses `CurrentItem` arc to know which file is being acted on.

Archive extraction chooses the backend by extension:
- `.zip` → `zip` crate.
- `.tar.gz` / `.tgz` → `flate2` + `tar`.
- `.tar.bz2` → `bzip2` + `tar`.
- `.tar.xz` → `xz2` + `tar`.
- `.gz` → `flate2` (single file inflate).
- `.bz2` → `bzip2` (single file inflate).
- `.tar` → `tar` only.

Add these as optional Cargo features on `matchmaker-cli` so they don't bloat
minimal builds.

#### 6c. `MMAction` variants

**File:** `matchmaker-cli/src/action.rs`

```rust
MMAction::FmYank,
MMAction::FmCut,
MMAction::FmPaste,
MMAction::FmUndo,
MMAction::FmRedo,
MMAction::FmSetYankPaths(Vec<String>),
```

`ActionContext` gains:

```rust
pub clipboard:  Clipboard,
pub fm_notify:  bool,
pub undo_stack: UndoStack,
pub redo_stack: UndoStack,
```

Semantic aliases in `ext_aliaser` (in `start.rs`):

```rust
Action::Semantic(ref s) if s == "fm_yank"  => acs![Action::Custom(MMAction::FmYank)],
// … cut, paste, undo, redo
```

**`FmYank` / `FmCut`:** collect current/selected items, write `FmClipboard`
to the arc, send `FmSetYankPaths` to highlight rows.

**`FmPaste`:** iterate clipboard items, call `copy_into` / `move_into`, push
`UndoAction`, clear clipboard on cut success, send `FmSetYankPaths([])`,
send `Reload`.

**`FmUndo`:** pop from `undo_stack`, push inverse onto `redo_stack`,
call `apply_undo`, send `Reload`.

**`FmRedo`:** pop from `redo_stack`, invert the action, call `apply_undo` on
the inverted form, push inverted back onto `undo_stack`, send `Reload`.
Non-invertible operations (`CreatedFile`, `Copied`) are no-ops in redo.

**`FmSetYankPaths`:** write directly into
`state.picker_ui.results.yank_paths`.

#### 6d. Status bar notifications (`--nav-notify`)

When `fm_notify` is true, each FM action sends `MMAction::SetStyledStatus`
with a styled message:
- Yank → green `"Copied: <name>"`.
- Cut → yellow `"Cut: <name>"`.
- Paste/Move → cyan on success, red on error.

Style tags are the existing `{green}` / `{reset}` markup understood by the
status renderer.

#### 6e. `CursorChange` event handler

Registered in `start.rs` when `nav_mode` is true:

```rust
mm.register_event_handler(Event::CursorChange, move |state, _| {
    let name = state.current_raw().map(|i| i.to_cow().to_string());
    *ci_clone.lock().unwrap() = name;
});
```

Keeps `current_item: CurrentItem` in sync with the cursor. Overlays read from
it without touching picker state directly.

#### 6f. Default nav-bind registration (start.rs)

When `nav_mode` is true, insert default FM binds *after* user `--nav-bind`
overrides using `.entry().or_insert()`:

```
d → Overlay(0)   (delete)
a → Overlay(1)   (create)
r → Overlay(2)   (rename)
z → Semantic("fm_zip") (zip)
Z → Semantic("fm_unzip") (unzip)
Space → Toggle
y → Semantic("fm_yank") (yank)
Y → Semantic("fm_unyank") (unyank)
x → Semantic("fm_cut") (cut)
p → Semantic("fm_paste") (paste)
ctrl-z → Semantic("fm_undo") (undo)
ctrl-y → Semantic("fm_redo") (redo)
```

---

## Dependency Graph

```
Phase 1 (trivial fixes)
    └── no deps

Phase 2 (standalone features)
    ├── 2a preview title     → no deps
    ├── 2b sort              → no deps
    ├── 2c inline status     → no deps
    ├── 2d preview gap       → no deps
    ├── 2e icons             → no deps
    └── 2f symlink           → no deps (but nicer with icons)

Phase 3 (--color)
    └── should land before Phase 4 (phases 4 use color keys)

Phase 4 (row styling)
    ├── 4a selected styling  → no deps (color keys optional)
    └── 4b yank highlight    → needs 4a (yank overrides selected_prefix)
                             → needs Phase 6 for FmSetYankPaths

Phase 5 (nav mode)
    ├── 5a-5b actions/ChDir  → no deps
    ├── 5c BlinkRate         → no deps
    ├── 5d-5f config/state   → needs 5a-5c
    ├── 5g apply_nav_binds   → needs 5d (Focus enum)
    ├── 5h visual indicators → needs 5d-5f
    ├── 5i-5j CLI/parse      → needs 5d-5h
    └── 5k test              → needs 5i

Phase 6 (file manager)
    └── all items → needs Phase 5 (nav_mode), Phase 4b (yank highlight)
```

---

## Non-invasive Extension Checklist

Before submitting any phase, verify:

- [ ] `--nav` is registered in `first_pass` as a **multi-value** flag (not the
      boolean allowlist) so its property tokens are not consumed as override args.
- [ ] All new config fields have a `false` / `None` / empty-string default so
      existing behaviour is unchanged when the flag is not set.
- [ ] New `Action` variants are added to `enum_from_str_display!` (both
      `Display` and `FromStr` arms) so they survive round-trips through TOML
      bind specs.
- [ ] New `Interrupt` variants are handled in every `match interrupt` arm
      (use `_` as a catch-all only if the compiler flags it as unreachable).
- [ ] `State::layout` array widening is a compile-time break caught immediately
      by the destructuring `let [a, b, c, d, e, f] = state.layout`.
- [ ] Any filesystem call (`is_dir`, `is_symlink`, `read_link`) is gated behind
      its feature flag (`icons`, `symlink_target`) — zero I/O cost when off.
- [ ] `fm.rs` is only compiled and registered when `nav_mode = true`. The
      `matchmaker-lib` crate has no `use` of any FM type.
- [ ] `dprint fmt` passes on all modified `.toml` and `.md` files.
- [ ] `cargo test -p matchmaker-lib` passes before moving to CLI changes.
- [ ] `cargo test --workspace` passes before committing a phase.
