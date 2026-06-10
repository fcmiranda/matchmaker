//! Config Types.
//! See `src/bin/mm/config.rs` for an example

use std::{collections::HashMap, ffi::OsString};

use matchmaker_partial_macros::partial;

pub use crate::config_types::*;
pub use crate::utils::{Percentage, serde::StringOrVec};

use crate::{
    action::{Action, Actions, NullActionExt},
    tui::IoStream,
    utils::serde::{escaped_opt_char, escaped_opt_string},
};

use cba::serde::transform::{camelcase_normalized, camelcase_normalized_option};
use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{BorderType, Borders},
};

use serde::{Deserialize, Serialize};

/// Settings unrelated to event loop/picker_ui.
///
/// Does not deny unknown fields.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[partial(recurse, path, derive(Debug, Deserialize))]
pub struct MatcherConfig {
    #[serde(flatten)]
    #[partial(skip)]
    pub matcher: NucleoMatcherConfig,
    #[serde(flatten)]
    pub worker: WorkerConfig,
}

/// "Input/output specific". Configures the matchmaker worker.
///
/// Does not deny unknown fields.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
pub struct WorkerConfig {
    /// How "stable" the results are. Higher values prioritize the initial ordering.
    #[serde(alias = "sort")]
    pub sort_threshold: SortThreshold,
    /// TODO: Enable raw mode where non-matching items are also displayed in a dimmed color.
    #[partial(alias = "r")]
    pub raw: bool,
    /// TODO: Track the current selection when the result list is updated.
    pub track: bool,
    /// Reverse the order of the input
    pub reverse: bool, // TODO: test with sort_threshold
}

/// Configures how input is fed to to the worker(s).
///
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
pub struct StartConfig {
    #[serde(deserialize_with = "escaped_opt_char")]
    #[partial(alias = "is")]
    pub input_separator: Option<char>,

    #[serde(deserialize_with = "escaped_opt_string")]
    #[partial(alias = "os")]
    pub output_separator: Option<String>,

    /// Format string to print accepted items as.
    #[partial(alias = "ot")]
    #[serde(alias = "output")]
    pub output_template: Option<String>,

    /// (cli only)  Default command to execute when stdin is not being read.
    #[partial(alias = "cmd", alias = "x")]
    pub command: CommandSetting,
    /// (cli only) Additional command which can be cycled through using Action::ReloadNext
    #[partial(alias = "ax")]
    pub additional_commands: Vec<String>,

    #[partial(alias = "d")]
    pub directory: EnvValue,

    pub sync: bool,
    /// Whether to parse ansi sequences from input
    #[partial(alias = "a")]
    pub ansi: bool,
    /// Trim the input
    #[partial(alias = "t")]
    pub trim: bool,

    pub mode: Option<String>,

    /// Sort input lines alphabetically before injecting into the picker.
    /// Only applies when reading from stdin (not from a command).
    pub sort: bool,

    /// Reload interval in milliseconds. If set, triggers a reload action periodically.
    #[partial(alias = "ri")]
    pub reload_interval: Option<u64>,
}

/// Exit conditions of the render loop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
pub struct ExitConfig {
    /// Exit automatically if there is only one match.
    pub select_1: bool,
    /// Allow returning without any items selected.
    pub allow_empty: bool,
    /// Abort if no items.
    pub abort_empty: bool,
    /// Last processed key is written here.
    /// Set to an empty path to disable.
    pub last_key_path: Option<std::path::PathBuf>,
}

impl Default for ExitConfig {
    fn default() -> Self {
        Self {
            select_1: false,
            allow_empty: false,
            abort_empty: true,
            last_key_path: None,
        }
    }
}

/// The ui config.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[partial(recurse, path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
pub struct RenderConfig {
    /// The default overlay style
    pub ui: UiConfig,
    /// The input bar style
    #[partial(alias = "q")]
    pub query: QueryConfig,
    /// The results table style
    #[partial(alias = "r")]
    pub results: ResultsConfig,

    /// The results status style
    pub status: StatusConfig,
    /// The preview panel style
    #[partial(alias = "p")]
    pub preview: PreviewConfig,
    #[partial(alias = "f")]
    pub footer: DisplayConfig,
    #[partial(alias = "h")]
    pub header: DisplayConfig,
    /// Action dialog box above the filter input.
    pub action: ActionBoxConfig,
}

impl RenderConfig {
    pub fn tick_rate(&self) -> u8 {
        self.ui.tick_rate
    }
}

/// Terminal settings.
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TerminalConfig {
    pub stream: IoStream, // consumed
    pub restore_fullscreen: bool,
    pub redraw_on_resize: bool,
    // https://docs.rs/crossterm/latest/crossterm/event/struct.PushKeyboardEnhancementFlags.html
    pub extended_keys: bool,
    pub sleep_ms: u64, // necessary to give ratatui a small delay before resizing after entering and exiting
    #[serde(flatten)]
    #[partial(recurse)]
    pub layout: Option<TerminalLayoutSettings>, // None for fullscreen
    pub clear_on_exit: bool,

    // unimplemented: currently favoring Execute2
    pub clear_after_execute: bool,

    /// Whether to use OSC 52 for clipboard copying.
    pub osc52: bool,
    /// Whether to enable mouse capture.
    pub mouse: bool,
    /// Whether to drop the end of the output of the copy command if it is a new line
    pub copy_trailing_newline: bool,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            stream: IoStream::default(),
            restore_fullscreen: true,
            redraw_on_resize: bool::default(),
            sleep_ms: 100,
            layout: Option::default(),
            extended_keys: true,
            clear_on_exit: true,
            clear_after_execute: true,
            osc52: true,
            mouse: true,
            copy_trailing_newline: false,
        }
    }
}

/// The container ui.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
pub struct UiConfig {
    #[partial(recurse)]
    #[partial(alias = "b")]
    pub border: BorderSetting,
    pub tick_rate: u8, // separate from render, but best place ig

    /// Enable navigation mode: pressing `ToggleFocus` switches keyboard focus
    /// between the input bar and the results list.
    #[partial(alias = "fm")]
    #[serde(alias = "focus_mode")]
    pub nav_mode: bool,

    /// Navigation-mode indicator colour (set via `--color nav:` or `--nav color:`).
    #[serde(deserialize_with = "camelcase_normalized")]
    #[serde(alias = "focus_color")]
    pub nav_color: Color,

    /// Make the navigation indicator blink.
    #[partial(alias = "fm_blink")]
    #[serde(alias = "focus_blink")]
    pub nav_blink: bool,

    /// Blink speed when `nav_blink = true`.
    #[partial(alias = "fm_blink_rate")]
    #[serde(alias = "focus_blink_rate")]
    pub nav_blink_rate: BlinkRate,

    /// Apply bold styling to navigation indicator.
    #[partial(alias = "fm_bold")]
    #[serde(alias = "focus_bold")]
    pub nav_bold: bool,

    /// Left-bar border style for the navigation indicator.
    #[partial(alias = "fm_bar")]
    #[serde(alias = "focus_bar")]
    pub nav_bar: Option<BorderType>,

    /// Marker rendered on the current result row when results pane is focused.
    #[partial(alias = "fm_marker")]
    #[serde(alias = "focus_marker")]
    pub nav_marker: String,

    /// Prompt text shown while the results pane is focused.
    #[partial(alias = "fm_prompt")]
    #[serde(alias = "focus_prompt")]
    pub nav_prompt: String,

    /// Key bindings active while navigation mode is enabled and results pane has focus.
    #[serde(alias = "focus_binds")]
    #[partial(no_recurse, unwrap)]
    pub nav_binds: HashMap<String, Actions<NullActionExt>>,

    /// Show notifications for file-manager clipboard actions.
    #[serde(alias = "focus_notify")]
    #[serde(alias = "fm_notify")]
    pub nav_notify: bool,

    /// When true, characters typed while the results pane is focused bypass the
    /// nav-bind intercept and fall through to the query input.  Useful when you
    /// want the navigation-mode visual indicators but still want live filtering
    /// while scrolling results.  Enable with `--nav no-filter` or in TOML as
    /// `nav_passthrough = true`.
    pub nav_passthrough: bool,

    /// When true only the basic j / k / J / K navigation binds are active;
    /// the h / l / gg / G / gb / gt binds are silenced and the file-manager
    /// semantic binds (d, a, r, …) are not injected.  Enable with
    /// `--nav basic` or in TOML as `nav_basic = true`.
    pub nav_basic: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        let mut nav_binds = HashMap::new();
        nav_binds.insert("j".to_string(), Actions::from([Action::Down(1)]));
        nav_binds.insert("k".to_string(), Actions::from([Action::Up(1)]));
        nav_binds.insert(
            "l".to_string(),
            Actions::from([
                Action::ChDir("{=}".to_string()),
                Action::Reload("".to_string()),
            ]),
        );
        nav_binds.insert(
            "h".to_string(),
            Actions::from([
                Action::ChDir("..".to_string()),
                Action::Reload("".to_string()),
            ]),
        );
        nav_binds.insert("J".to_string(), Actions::from([Action::PreviewDown(1)]));
        nav_binds.insert("K".to_string(), Actions::from([Action::PreviewUp(1)]));
        nav_binds.insert("gg".to_string(), Actions::from([Action::PreviewUp(0)]));
        nav_binds.insert("G".to_string(), Actions::from([Action::PreviewDown(0)]));
        nav_binds.insert("gb".to_string(), Actions::from([Action::Pos(-1)]));
        nav_binds.insert("gt".to_string(), Actions::from([Action::Pos(0)]));

        Self {
            border: Default::default(),
            tick_rate: 60,
            nav_mode: false,
            nav_color: Color::Yellow,
            nav_blink: false,
            nav_blink_rate: BlinkRate::Normal,
            nav_bold: false,
            nav_bar: None,
            nav_marker: String::new(),
            nav_prompt: String::new(),
            nav_binds,
            nav_notify: false,
            nav_passthrough: false,
            nav_basic: false,
        }
    }
}

/// The query (input) bar ui.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
pub struct QueryConfig {
    #[partial(recurse)]
    #[partial(alias = "b")]
    pub border: BorderSetting,

    // text styles
    #[partial(recurse)]
    pub style: StyleSetting,

    #[partial(recurse)]
    pub prompt_style: StyleSetting,

    /// The prompt prefix.
    #[serde(deserialize_with = "deserialize_string_or_char_as_double_width")]
    pub prompt: String,

    /// Cursor style.
    pub cursor: CursorSetting,

    /// Initial text in the input bar.
    #[partial(alias = "i")]
    pub initial: String,

    /// Maintain padding when moving the cursor in the bar.
    pub scroll_padding: bool,

    /// When true, display the match status (e.g. "3/42") right-aligned
    /// inside the input bar instead of in the separate status row.
    pub status_inline: bool,

    /// Whether to show the filter input bar.
    pub show: bool,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            border: Default::default(),
            style: Default::default(),
            prompt_style: StyleSetting {
                modifier: Modifier::empty(),
                ..Default::default()
            },
            prompt: "> ".to_string(),
            cursor: Default::default(),
            initial: Default::default(),

            scroll_padding: true,
            status_inline: false,
            show: true,
        }
    }
}

impl QueryConfig {}

/// Configuration for the action dialog box shown above the filter input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
pub struct ActionBoxConfig {
    /// Width of the action box as a percentage of the terminal width (0–100). Default: 100.
    pub width_pct: Percentage,
    /// Number of extra lines reserved below the input for a preview area. Default: 0 (no preview).
    pub preview_height: u16,
    /// Border drawn at the bottom of the action box, acting as a visual separator.
    /// Defaults to a plain bottom line. Set `color` to colorize it.
    #[partial(recurse)]
    #[partial(alias = "b")]
    pub border: BorderSetting,
}

impl Default for ActionBoxConfig {
    fn default() -> Self {
        Self {
            width_pct: Percentage::new(100),
            preview_height: 0,
            border: BorderSetting {
                sides: Some(Borders::BOTTOM),
                ..Default::default()
            },
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
pub struct OverlayConfig {
    #[partial(recurse)]
    #[partial(alias = "b")]
    pub border: BorderSetting,
    pub outer_dim: bool,
    pub layout: OverlayLayoutSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
pub struct OverlayLayoutSettings {
    /// w, h
    #[partial(alias = "p")]
    pub percentage: [Percentage; 2],
    /// w, h
    pub min: [u16; 2],
    /// w, h
    pub max: [u16; 2],

    /// y_offset as a percentage of total height: 50 for neutral, (default: 55)
    pub y_offset: Percentage,
}

impl Default for OverlayLayoutSettings {
    fn default() -> Self {
        Self {
            percentage: [Percentage::new(60), Percentage::new(30)],
            min: [10, 10],
            max: [200, 30],
            y_offset: Percentage::new(55),
        }
    }
}

// pub struct OverlaySize

#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AutoscrollSettings {
    /// Number of characters at the start of the line to always keep visible.
    #[partial(alias = "i")]
    pub initial_preserved: usize,
    /// Enable/disable horizontal autoscroll.
    #[partial(alias = "a")]
    pub enabled: bool,
    /// Number of characters to show around the match.
    #[partial(alias = "c")]
    pub context: usize,
    /// Whether to autoscroll to the end of the line.
    #[partial(alias = "e")]
    pub end: bool,
    /// Enable autoscroll even when wrap = true. Ignored if enable = false.
    pub always: bool,
    /// The string to show when text is horizontally scrolled.
    pub indicator: String,
    /// Style of the hscroll indicator.
    pub indicator_style: StyleSetting,
}

impl Default for AutoscrollSettings {
    fn default() -> Self {
        Self {
            initial_preserved: 0,
            enabled: true,
            context: 4,
            end: false,
            always: false,
            indicator: "..".to_string(),
            indicator_style: crate::config_types::StyleSetting {
                fg: Some(ratatui::style::Color::DarkGray),
                ..Default::default()
            },
        }
    }
}

#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResultsConfig {
    #[partial(recurse)]
    #[partial(alias = "b")]
    pub border: BorderSetting,

    // prefixes
    #[serde(deserialize_with = "deserialize_string_or_char_as_double_width")]
    pub multi_prefix: String,
    pub unselected_prefix: String,
    pub default_prefix: String,

    pub spinner_prefix: String,
    pub spinner: String,
    pub spinner_inline: bool,

    #[serde(alias = "prefix")]
    #[partial(recurse)]
    pub prefix_style: StyleSetting,

    #[serde(alias = "prefix_inactive")]
    #[partial(recurse)]
    pub prefix_inactive_style: StyleSetting,

    #[partial(recurse)]
    pub unselected_prefix_style: StyleSetting,

    #[partial(recurse)]
    pub spinner_style: StyleSetting,

    /// Enable selections
    pub multi: bool,

    // text styles
    #[partial(recurse)]
    pub style: StyleSetting,

    // inactive_col styles
    #[serde(alias = "inactive")]
    #[partial(recurse)]
    pub inactive_style: StyleSetting,

    // inactive_col styles on the current item
    #[serde(alias = "inactive_current")]
    #[partial(recurse)]
    pub inactive_current_style: StyleSetting,

    #[serde(alias = "match")]
    #[partial(recurse)]
    pub match_style: StyleSetting,

    /// current item style
    #[serde(alias = "current")]
    #[partial(recurse)]
    pub current_style: StyleSetting,

    /// How the styles are applied across the row:
    /// Disjoint: Styles are applied per column.
    /// Capped: The inactive styles are applied per row, and the active styles applied on the active column.
    /// Full: Inactive column styles are ignored, the current style is applied on the current row.
    #[serde(deserialize_with = "camelcase_normalized")]
    pub row_connection: RowConnectionStyle,

    // scroll
    #[partial(alias = "c")]
    #[serde(alias = "cycle")]
    pub scroll_wrap: bool,
    #[partial(alias = "sp")]
    pub scroll_padding: u16,
    #[partial(alias = "r")]
    pub reverse: Option<bool>,

    // wrap
    #[partial(alias = "w")]
    pub wrap: bool,
    pub min_width: u16,

    // autoscroll
    #[partial(recurse, alias = "a")]
    pub autoscroll: AutoscrollSettings,

    // ------------
    // experimental
    // ------------
    pub column_spacing: Count,
    pub current_prefix: String,

    /// Maximum row height.
    /// VScroll/Preview can still be used to view the whole result.
    pub max_height: usize,
    pub show_skipped: bool,
    /// Always false if max_height is set
    pub vscroll_current_only: bool,

    // lowpri: maybe space-around/space-between instead?
    #[partial(alias = "ra")]
    pub right_align_last: bool,
    #[partial(alias = "v")]
    #[serde(alias = "vertical")]
    pub stacked_columns: bool,

    #[serde(alias = "hr")]
    #[serde(deserialize_with = "camelcase_normalized")]
    pub separator: HorizontalSeparator,
    pub separator_style: StyleSetting,

    /// Prepend a file-type icon (Nerd Font) before the first column text.
    /// Requires a Nerd Font in the terminal. Defaults to false.
    pub icons: bool,

    /// Show symlink targets appended to the first column text.
    /// Defaults to false.
    pub symlink_target: bool,
    /// Style for the appended symlink target text.
    #[partial(recurse)]
    pub symlink_target_style: StyleSetting,

    /// Style for multi-selected rows (not the cursor row).
    #[partial(recurse)]
    pub selected_style: StyleSetting,
    /// Style for the prefix marker of multi-selected rows.
    #[partial(recurse)]
    pub selected_prefix_style: StyleSetting,
    /// Style for the prefix marker of yanked rows (FM mode).
    #[partial(recurse)]
    pub yank_prefix_style: StyleSetting,
    /// Style for the prefix marker of cut rows (FM mode).
    #[partial(recurse)]
    pub cut_prefix_style: StyleSetting,

    /// Style for group header rows (set via --color group-header:…)
    #[partial(recurse)]
    pub group_header_style: StyleSetting,
}

impl Default for ResultsConfig {
    fn default() -> Self {
        ResultsConfig {
            border: Default::default(),

            multi_prefix: "▌ ".to_string(),
            unselected_prefix: "  ".to_string(),
            default_prefix: Default::default(),
            spinner_prefix: "".to_string(),
            spinner: "dot".to_string(),
            spinner_inline: false,
            prefix_style: Default::default(),
            prefix_inactive_style: Default::default(),
            unselected_prefix_style: Default::default(),
            spinner_style: Default::default(),
            multi: true,

            style: Default::default(),
            inactive_style: Default::default(),

            inactive_current_style: StyleSetting {
                // fg: Some(Color::DarkGray),
                // bg: Some(Color::Black),
                ..Default::default()
            },

            match_style: StyleSetting {
                fg: Some(Color::Green),
                modifier: Modifier::ITALIC,
                ..Default::default()
            },

            current_style: StyleSetting {
                bg: Some(Color::Black),
                modifier: Modifier::BOLD,
                ..Default::default()
            },

            row_connection: RowConnectionStyle::Capped,

            scroll_wrap: false,
            scroll_padding: 2,
            reverse: None,

            wrap: false,
            min_width: 2,
            max_height: 0,

            autoscroll: Default::default(),

            column_spacing: Default::default(),
            current_prefix: Default::default(),
            right_align_last: false,
            stacked_columns: false,
            separator: Default::default(),
            separator_style: Default::default(),
            show_skipped: true,
            vscroll_current_only: true,
            icons: false,
            symlink_target: false,
            symlink_target_style: StyleSetting {
                fg: Some(Color::DarkGray),
                ..Default::default()
            },
            selected_style: StyleSetting {
                modifier: Modifier::BOLD,
                ..Default::default()
            },
            selected_prefix_style: StyleSetting {
                fg: Some(Color::Cyan),
                modifier: Modifier::BOLD,
                ..Default::default()
            },
            yank_prefix_style: StyleSetting {
                fg: Some(Color::Yellow),
                modifier: Modifier::BOLD,
                ..Default::default()
            },
            cut_prefix_style: StyleSetting {
                fg: Some(Color::Red),
                modifier: Modifier::BOLD,
                ..Default::default()
            },
            group_header_style: StyleSetting {
                fg: Some(Color::Cyan),
                modifier: Modifier::BOLD,
                ..Default::default()
            },
        }
    }
}

#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StatusConfig {
    #[partial(recurse)]
    pub style: StyleSetting,

    /// Whether the status is visible.
    pub show: bool,
    /// Indent the status to match the results.
    pub match_indent: bool,

    /// Supports replacements:
    /// - `\r` -> cursor index
    /// - `\m` -> match count
    /// - `\t` -> total count
    /// - `\s` -> available whitespace / # appearances
    /// - `\S` -> Increment # appearances for `\s`
    ///
    /// For example: `r#"\m/\t"#.to_string()`
    #[partial(alias = "t")]
    pub template: String,

    /// - Full: available whitespace is computed using the full ui width when replacing `\s` in the template.
    /// - Disjoint: no effect.
    /// - Capped: no effect. (Since, unlike [`DisplayConfig`], status line can not display over the preview).
    pub row_connection: RowConnectionStyle,

    pub interactions: InteractionRegionSetting,
}
impl Default for StatusConfig {
    fn default() -> Self {
        Self {
            style: StyleSetting {
                fg: Some(Color::Green),
                modifier: Modifier::ITALIC,
                ..Default::default()
            },
            show: true,
            match_indent: true,
            template: String::new(),
            row_connection: RowConnectionStyle::Full,

            interactions: Default::default(),
        }
    }
}

impl StatusConfig {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
pub struct DisplayConfig {
    #[partial(recurse)]
    #[partial(alias = "b")]
    pub border: BorderSetting,

    #[partial(recurse)]
    pub style: StyleSetting,

    /// Indent content to match the results table.
    pub match_indent: bool,
    /// Enable line wrapping.
    pub wrap: bool,

    /// Static content to display.
    pub content: Option<StringOrVec>,

    /// This setting controls the effective width of the displayed content.
    /// - Full: Effective width is the full ui width.
    /// - Capped: Effective width is the full ui width, but
    ///   any width exceeding the width of the Results UI is occluded by the preview pane.
    /// - Disjoint: Same as capped. Additionally, the (bg) style is applied to individual
    /// columns instead of uniformly on the row.
    ///
    /// # Note
    /// The width effect only applies on the footer, and when the content is singular.
    #[serde(deserialize_with = "camelcase_normalized")]
    pub row_connection: RowConnectionStyle,

    /// (cli only) This setting controls how many lines are read from the input for display with the header.
    /// Note: Incoming lines are partitioned into columns the same way regular lines are.
    #[partial(alias = "h")]
    pub header_lines: usize,

    pub interactions: Vec<InteractionRegionSetting>,
}

pub type InteractionRegionSetting = Vec<(u8, String)>;

impl Default for DisplayConfig {
    fn default() -> Self {
        DisplayConfig {
            border: Default::default(),
            match_indent: true,
            style: StyleSetting {
                fg: Some(Color::Cyan),
                ..Default::default()
            },
            wrap: false,
            row_connection: Default::default(),
            content: None,
            header_lines: 0,

            interactions: Default::default(),
        }
    }
}

/// # Example
/// ```rust
/// use matchmaker::config::{PreviewConfig, PreviewSetting, PreviewLayout};
///
/// let _ = PreviewConfig {
///     layout: vec![
///         PreviewSetting {
///             layout: PreviewLayout::default(),
///             command: String::new(),
///             ..Default::default()
///         }
///     ],
///     ..Default::default()
/// };
/// ```
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PreviewConfig {
    #[partial(recurse)]
    #[partial(alias = "b")]
    pub border: BorderSetting,
    #[partial(recurse, set = "recurse")]
    #[partial(alias = "l")]
    pub layout: Vec<PreviewSetting>,
    #[serde(alias = "scroll")]
    #[partial(recurse)]
    #[partial(alias = "i")]
    pub initial: PreviewInitialSetting,
    /// Whether to cycle to top after scrolling to the bottom and vice versa.
    #[partial(alias = "c")]
    #[serde(alias = "cycle")]
    pub scroll_wrap: bool,
    pub wrap: bool,
    /// Whether to show the preview pane initially.
    /// Can either be a boolean or a number which the relevant dimension of the available ui area must exceed.
    pub show: ShowCondition,

    pub reevaluate_show_on_resize: bool,

    /// Width of the drag area for resizing the preview pane.
    /// If `None`, it defaults to the width of the preview border.
    /// If `0`, drag resizing is disabled.
    pub drag_width: Option<u16>,
    /// Whether to enable native media previews using ratatui-image
    #[partial(alias = "m")]
    pub media: bool,
    /// Overriding graphics protocol for media previews (e.g. "kitty", "sixel", "halfblocks", "iterm2")
    pub media_protocol: Option<String>,
}

impl PreviewConfig {
    pub fn trim_commands(&mut self) {
        for setting in &mut self.layout {
            setting.command = setting.command.trim().to_string();
        }
    }
}

impl Default for PreviewConfig {
    fn default() -> Self {
        PreviewConfig {
            border: BorderSetting {
                padding: Padding(ratatui::widgets::Padding::left(2)),
                ..Default::default()
            },
            initial: Default::default(),
            layout: Default::default(),
            scroll_wrap: false,
            wrap: false,
            show: Default::default(),
            reevaluate_show_on_resize: false,
            drag_width: None,
            media: false,
            media_protocol: None,
        }
    }
}

/// Determines the initial scroll offset of the preview window.
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PreviewInitialSetting {
    /// Extract the initial display index `n` of the preview window from this column.
    /// `n` lines are skipped after the header lines are consumed.
    pub index: Option<StringValue>,
    /// For adjusting the initial scroll index.
    #[partial(alias = "o")]
    pub offset: isize,
    /// How far from the bottom of the preview window the scroll offset should appear.
    #[partial(alias = "p")]
    pub percentage: Percentage,
    /// Keep the top N lines as the fixed header so that they are always visible.
    #[partial(alias = "h")]
    pub header_lines: usize,

    #[partial(alias = "t")]
    pub tail: bool,
}

impl Default for PreviewInitialSetting {
    fn default() -> Self {
        Self {
            index: Default::default(),
            offset: -1,
            percentage: Default::default(),
            header_lines: Default::default(),
            tail: false,
        }
    }
}

#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PreviewerConfig {
    pub try_lossy: bool,
    pub delay_clear: bool,

    // todo
    pub cache: u8,

    pub debounce_ms: u64,
    pub max_procs: usize,
    pub always_trigger: bool,

    pub help: HelpDisplayConfig,
    pub shell: Option<Vec<OsString>>,
    pub trim_commands: bool,
    pub hide_semantic_help: bool,

    /// See [`StartConfig`]
    pub command_args: Vec<OsString>,

    /// Pixel resolution for media previews (ffmpegthumbnailer -s). Default: 512. 0 = original
    pub media_size: u32,
}

impl Default for PreviewerConfig {
    fn default() -> Self {
        Self {
            try_lossy: false,
            delay_clear: true,
            cache: 0,
            debounce_ms: 0,
            max_procs: 4,
            always_trigger: true,
            help: Default::default(),
            shell: None,
            trim_commands: false,
            hide_semantic_help: true,

            command_args: Default::default(),
            media_size: 512,
        }
    }
}

#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HelpDisplayConfig {
    pub colors: Option<HelpColorConfig>,
    pub hide_semantic: bool,
    pub seq_brackets: Option<[char; 2]>,
    pub quote_traces: bool,
    pub max_len: usize,
    pub ellipsize_center: bool,
}

impl Default for HelpDisplayConfig {
    fn default() -> Self {
        Self {
            colors: Some(Default::default()),
            hide_semantic: true,
            seq_brackets: Some(['[', ']']),
            quote_traces: true,
            max_len: 25,
            ellipsize_center: false,
        }
    }
}

/// Help coloring
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelpColorConfig {
    #[serde(deserialize_with = "camelcase_normalized")]
    pub section: Color,
    #[serde(deserialize_with = "camelcase_normalized")]
    pub key: Color,
    #[serde(deserialize_with = "camelcase_normalized")]
    pub value: Color,
}

impl Default for HelpColorConfig {
    fn default() -> Self {
        Self {
            section: Color::Blue,
            key: Color::Green,
            value: Color::White,
        }
    }
}

// ----------- SETTING TYPES -------------------------

#[derive(Default, Debug, Clone, PartialEq, Deserialize, Serialize)]
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[serde(default, deny_unknown_fields)]
pub struct BorderSetting {
    #[serde(deserialize_with = "camelcase_normalized_option")]
    pub r#type: Option<BorderType>,
    #[serde(deserialize_with = "camelcase_normalized")]
    pub color: Color,
    /// Given as sides joined by `|`. i.e.:
    /// `sides = "TOP | BOTTOM"``
    /// `sides = "ALL"`
    /// When omitted, this either ALL or the side that sits between results and the corresponding layout if either padding or type are specified, otherwise NONE.
    ///
    /// An empty string enforces no sides:
    /// `sides = ""`
    // #[serde(deserialize_with = "uppercase_normalized_option")] // need ratatui bitflags to use transparent
    pub sides: Option<Borders>,
    /// Supply as either 1, 2, or 4 numbers for:
    ///
    /// - Same padding on all sides
    /// - Vertical and horizontal padding values
    /// - Top, Right, Bottom, Left padding values
    ///
    /// respectively.
    pub padding: Padding,
    pub title: String,
    // #[serde(deserialize_with = "transform_uppercase")]
    pub title_modifier: Modifier,
    pub modifier: Modifier,
    #[serde(deserialize_with = "camelcase_normalized")]
    pub bg: Color,
    /// Foreground color for the dynamic item title shown in the preview border.
    /// When `Color::Reset` (the default) the title inherits the border color.
    #[serde(deserialize_with = "camelcase_normalized")]
    pub title_fg: Color,
}

impl BorderSetting {
    pub fn as_block(&self) -> ratatui::widgets::Block<'_> {
        let mut ret = ratatui::widgets::Block::default()
            .padding(self.padding.0)
            .style(Style::default().bg(self.bg).add_modifier(self.modifier));

        if !self.title.is_empty() {
            let title = Span::styled(
                &self.title,
                Style::default().add_modifier(self.title_modifier),
            );

            ret = ret.title(title)
        };

        if !self.is_empty() {
            ret = ret
                .borders(self.sides())
                .border_type(self.r#type.unwrap_or_default())
                .border_style(ratatui::style::Style::default().fg(self.color))
        }

        ret
    }

    /// Like `as_block` but uses `title_override` (the dynamic item text) when
    /// provided, falling back to the static `self.title` when `None`.
    /// The title is styled with `title_fg` (or the border color when `title_fg`
    /// is `Color::Reset`).
    pub fn block_with_title<'a>(
        &'a self,
        title_override: Option<&'a str>,
    ) -> ratatui::widgets::Block<'a> {
        let mut ret = ratatui::widgets::Block::default()
            .padding(self.padding.0)
            .style(Style::default().bg(self.bg).add_modifier(self.modifier));

        let title_text: Option<&'a str> = title_override.or_else(|| {
            if self.title.is_empty() {
                None
            } else {
                Some(&self.title)
            }
        });

        if let Some(t) = title_text {
            let fg = if self.title_fg == Color::Reset {
                self.color
            } else {
                self.title_fg
            };
            let title = Span::styled(t, Style::default().fg(fg).add_modifier(self.title_modifier));
            ret = ret.title(title);
        }

        if !self.is_empty() {
            ret = ret
                .borders(self.sides())
                .border_type(self.r#type.unwrap_or_default())
                .border_style(ratatui::style::Style::default().fg(self.color))
        }

        ret
    }

    pub fn sides(&self) -> Borders {
        if let Some(s) = self.sides {
            s
        } else if self.color != Default::default() || self.r#type != Default::default() {
            Borders::ALL
        } else {
            Borders::NONE
        }
    }

    pub fn as_static_block(&self) -> ratatui::widgets::Block<'static> {
        let mut ret = ratatui::widgets::Block::default()
            .padding(self.padding.0)
            .style(Style::default().bg(self.bg).add_modifier(self.modifier));

        if !self.title.is_empty() {
            let title: Span<'static> = Span::styled(
                self.title.clone(),
                Style::default().add_modifier(self.title_modifier),
            );

            ret = ret.title(title)
        };

        if !self.is_empty() {
            ret = ret
                .borders(self.sides())
                .border_type(self.r#type.unwrap_or_default())
                .border_style(ratatui::style::Style::default().fg(self.color))
        }

        ret
    }

    pub fn is_empty(&self) -> bool {
        self.sides() == Borders::NONE
    }

    pub fn height(&self) -> u16 {
        let mut height = 0;
        height += self.sides().contains(Borders::TOP) as u16
            + self.sides().contains(Borders::BOTTOM) as u16;
        height += self.padding.top + self.padding.bottom;
        height += (!self.title.is_empty() as u16).saturating_sub(!self.is_empty() as u16);

        height
    }

    pub fn width(&self) -> u16 {
        let mut width = 0;
        width += self.sides().contains(Borders::LEFT) as u16
            + self.sides().contains(Borders::RIGHT) as u16;

        width += self.padding.left + self.padding.right;

        width
    }

    pub fn left(&self) -> u16 {
        let mut width = 0;
        width += !self.is_empty() as u16;
        width += self.padding.left;

        width
    }

    pub fn top(&self) -> u16 {
        let mut height = 0;
        height += !self.is_empty() as u16;
        height += self.padding.top;
        height += (!self.title.is_empty() as u16).saturating_sub(!self.is_empty() as u16);

        height
    }
}

// how to determine how many rows to allocate?
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
pub struct TerminalLayoutSettings {
    /// Percentage of total rows to occupy.
    #[partial(alias = "p")]
    pub percentage: Percentage,
    pub min: u16,
    pub max: u16, // 0 for terminal height cap
}

impl Default for TerminalLayoutSettings {
    fn default() -> Self {
        Self {
            percentage: Percentage::new(50),
            min: 10,
            max: 120,
        }
    }
}

#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreviewSetting {
    #[serde(flatten)]
    #[partial(recurse)]
    pub layout: PreviewLayout,
    #[partial(recurse)]
    pub border: Option<BorderSetting>,
    /// Optional preview title for this layout.
    ///
    /// - Omit for default dynamic title (current item name)
    /// - Set to `{item}` for explicit dynamic title
    /// - Set to any other string for a static title
    /// - `{item}` inside a longer string is interpolated
    pub title: Option<String>,
    #[serde(default, alias = "cmd", alias = "x")]
    pub command: String,

    #[cfg(feature = "partial")]
    #[partial(unwrap)]
    #[serde(alias = "scroll")]
    #[serde(default)]
    pub initial: PartialPreviewInitialSetting,
}

#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreviewLayout {
    pub side: Side,
    /// Percentage of total rows/columns to occupy.
    #[serde(alias = "p")]
    // we need serde here since its specified inside the value but i don't think there's another case for it.
    pub percentage: Percentage,
    pub min: i16,
    pub max: i16,
    /// Width (or height) in cells of the draggable gap between the preview and
    /// the picker.  When `0` the gap is not rendered but the drag-zone still
    /// falls back to the border edge (existing behaviour).
    pub gap: u16,
    /// Row offset from the top of the gap at which the selection counter is
    /// drawn.  Defaults to `5`.
    #[partial(alias = "gco")]
    pub gap_counter_offset: u16,
}

impl Default for PreviewLayout {
    fn default() -> Self {
        Self {
            side: Side::Right,
            percentage: Percentage::new(60),
            min: 15,
            max: i16::MAX,
            gap: 1,
            gap_counter_offset: 0,
        }
    }
}

use crate::utils::serde::bounded_usize;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[partial(path, derive(Debug, Clone, PartialEq, Deserialize, Serialize))]
pub struct ColumnsConfig {
    /// The strategy of how columns are parsed from input lines
    #[partial(alias = "s")]
    pub split: Split,
    /// Column names
    #[partial(alias = "n")]
    // #[partial(recurse, set = "recurse")] // partial application is better on the command line but we don't want it for overrides
    pub names: Vec<ColumnSetting>,
    /// Maximum number of columns to autogenerate when names is unspecified. Minimum of 1, maximum of 16.
    #[serde(deserialize_with = "bounded_usize::<_, 1, 16>")]
    #[serde(alias = "max")]
    max_columns: usize,
    #[partial(alias = "i")]
    pub default: Option<StringValue>,
    /// When autogenerating column names, start from 0 instead of 1.
    pub names_from_zero: bool,
}

impl ColumnsConfig {
    pub fn max_cols(&self) -> usize {
        self.max_columns.min(16).max(1)
    }
}

impl Default for ColumnsConfig {
    fn default() -> Self {
        Self {
            split: Default::default(),
            names: Default::default(),
            max_columns: 6,
            default: None,
            names_from_zero: false,
        }
    }
}

// ----------- Nucleo config helper
#[derive(Debug, Clone, PartialEq)]
pub struct NucleoMatcherConfig(pub nucleo::Config);

impl Default for NucleoMatcherConfig {
    fn default() -> Self {
        Self(nucleo::Config::DEFAULT)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
struct MatcherConfigHelper {
    pub normalize: Option<bool>,
    pub ignore_case: Option<bool>,
    pub prefer_prefix: Option<bool>,
    pub match_paths: bool,
}

impl serde::Serialize for NucleoMatcherConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let helper = MatcherConfigHelper {
            normalize: Some(self.0.normalize),
            ignore_case: Some(self.0.ignore_case),
            prefer_prefix: Some(self.0.prefer_prefix),
            match_paths: false,
        };
        helper.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for NucleoMatcherConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let helper = MatcherConfigHelper::deserialize(deserializer)?;
        let mut config = nucleo::Config::DEFAULT;

        if helper.match_paths {
            config.set_match_paths();
        }

        if let Some(norm) = helper.normalize {
            config.normalize = norm;
        }
        if let Some(ic) = helper.ignore_case {
            config.ignore_case = ic;
        }
        if let Some(pp) = helper.prefer_prefix {
            config.prefer_prefix = pp;
        }

        Ok(NucleoMatcherConfig(config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preview_config_trim_commands() {
        let mut config = PreviewConfig {
            layout: vec![
                PreviewSetting {
                    command: "  echo hello  ".to_string(),
                    ..Default::default()
                },
                PreviewSetting {
                    command: "\nls -la\n".to_string(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        config.trim_commands();

        assert_eq!(config.layout[0].command, "echo hello");
        assert_eq!(config.layout[1].command, "ls -la");
    }
}
