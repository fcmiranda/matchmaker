use std::collections::HashSet;

use cba::bring::split::split_on_nesting;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Row, Table},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    SSS, Selection, Selector,
    config::{HorizontalSeparator, ResultsConfig, RowConnectionStyle, StatusConfig},
    nucleo::{Status, Worker},
    render::Click,
    utils::{
        string::{allocate_widths, fit_width, substitute_escaped},
        text::{clip_text_lines, expand_indents, prefix_span},
    },
};

#[derive(Debug)]
pub struct ResultsUI {
    cursor: u16,
    bottom: u32,
    col: Option<usize>,
    pub hscroll: i8,
    pub vscroll: u8,

    /// available height
    height: u16,
    /// available width
    width: u16,
    // column widths.
    // Note that the first width include the indentation.
    widths: Vec<u16>,
    medians: Vec<u16>,

    pub hidden_columns: Vec<bool>,

    pub status: Status,
    status_template: Line<'static>,
    pub status_config: StatusConfig,

    pub config: ResultsConfig,

    bottom_clip: Option<u16>,
    cursor_above: u16,

    pub cursor_disabled: bool,

    /// Set of col-0 names whose prefix should be rendered with `yank_prefix_style`.
    /// Populated externally via `Action::Custom(FmSetYankPaths(...))`.
    pub yank_paths: HashSet<String>,
    pub cut_paths: HashSet<String>,
}

impl ResultsUI {
    pub fn new(config: ResultsConfig, mut status_config: StatusConfig) -> Self {
        status_config.interactions.sort_by_key(|(i, _)| *i);

        Self {
            cursor: 0,
            bottom: 0,
            col: None,
            hscroll: 0,
            vscroll: 0,

            widths: Vec::new(),
            medians: Vec::new(),
            height: 0, // uninitialized, so be sure to call update_dimensions
            width: 0,
            hidden_columns: Default::default(),

            status: Default::default(),
            status_template: Line::from(status_config.template.clone()).style(status_config.style),
            status_config,
            config,

            cursor_disabled: false,
            bottom_clip: None,
            cursor_above: 0,
            yank_paths: HashSet::new(),
            cut_paths: HashSet::new(),
        }
    }

    pub fn hidden_columns(&mut self, hidden_columns: Vec<bool>) {
        self.hidden_columns = hidden_columns;
    }

    /// Return the correct inactive prefix style for a given row.
    ///
    /// Priority: yank (highest) > selected > default.
    fn inactive_prefix_style(
        &self,
        col0_name: &str,
        is_selected: bool,
        is_spinner: bool,
        cwd: &std::path::Path,
    ) -> crate::config::StyleSetting {
        if is_spinner {
            return self.config.spinner_style;
        }
        let is_yanked = if !col0_name.is_empty() {
            let path = std::path::Path::new(col0_name);
            let abs_path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
            };
            self.yank_paths
                .contains(&abs_path.to_string_lossy().to_string())
        } else {
            false
        };

        let is_cut = if !col0_name.is_empty() {
            let path = std::path::Path::new(col0_name);
            let abs_path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
            };
            self.cut_paths
                .contains(&abs_path.to_string_lossy().to_string())
        } else {
            false
        };

        if is_cut {
            self.config.cut_prefix_style
        } else if is_yanked {
            self.config.yank_prefix_style
        } else if is_selected {
            self.config.selected_prefix_style
        } else {
            self.config.prefix_inactive_style
        }
    }

    fn active_prefix_style(
        &self,
        col0_name: &str,
        is_selected: bool,
        is_spinner: bool,
        cwd: &std::path::Path,
    ) -> crate::config::StyleSetting {
        if is_spinner {
            return self.config.spinner_style;
        }
        let is_yanked = if !col0_name.is_empty() {
            let path = std::path::Path::new(col0_name);
            let abs_path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
            };
            self.yank_paths
                .contains(&abs_path.to_string_lossy().to_string())
        } else {
            false
        };

        let is_cut = if !col0_name.is_empty() {
            let path = std::path::Path::new(col0_name);
            let abs_path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                cwd.join(path)
            };
            self.cut_paths
                .contains(&abs_path.to_string_lossy().to_string())
        } else {
            false
        };

        if is_cut {
            self.config.cut_prefix_style
        } else if is_yanked {
            self.config.yank_prefix_style
        } else if is_selected {
            self.config.selected_prefix_style
        } else {
            self.config.prefix_style
        }
    }

    // as given by ratatui area
    pub fn update_dimensions(&mut self, area: &Rect) {
        let [bw, bh] = [self.config.border.height(), self.config.border.width()];
        self.width = area.width.saturating_sub(bw);
        self.height = area.height.saturating_sub(bh);
        log::debug!("Updated results dimensions: {}x{}", self.width, self.height);
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    // ------ config -------
    pub fn reverse(&self) -> bool {
        self.config.reverse == Some(true)
    }
    pub fn is_wrap(&self) -> bool {
        self.config.wrap
    }
    pub fn wrap(&mut self, wrap: bool) {
        self.config.wrap = wrap;
    }

    // ----- columns --------
    // todo: support cooler things like only showing/outputting a specific column/cycling columns
    pub fn toggle_col(&mut self, col_idx: usize) -> bool {
        self.reset_current_scroll();

        if self.col == Some(col_idx) {
            self.col = None
        } else {
            self.col = Some(col_idx);
        }
        self.col.is_some()
    }
    pub fn cycle_col(&mut self) {
        self.reset_current_scroll();

        self.col = match self.col {
            None => self.widths.is_empty().then_some(0),
            Some(c) => {
                let next = c + 1;
                if next < self.widths.len() {
                    Some(next)
                } else {
                    None
                }
            }
        };
    }

    // ------- NAVIGATION ---------
    fn scroll_padding(&self) -> u16 {
        self.config.scroll_padding.min(self.height / 2)
    }
    pub fn end(&self) -> u32 {
        self.status.matched_count.saturating_sub(1)
    }

    /// Index in worker snapshot of current item.
    /// Use with worker.get_nth().
    //  Equivalently, the cursor progress in the match list
    pub fn index(&self) -> u32 {
        if self.cursor_disabled {
            u32::MAX
        } else {
            self.cursor as u32 + self.bottom
        }
    }

    pub fn cursor_offset(&self) -> Option<u16> {
        if self.cursor_disabled {
            None
        } else {
            Some(self.cursor)
        }
    }

    /// Returns whether scroll wrap caused it to jump to the end
    pub fn cursor_prev(&mut self) -> bool {
        self.reset_current_scroll();

        if (self.cursor_above <= self.scroll_padding() || self.cursor <= self.scroll_padding())
            && self.bottom > 0
        {
            self.bottom -= 1;
            self.bottom_clip = None;
        } else if self.cursor > 0 {
            self.cursor -= 1;
        } else if self.config.scroll_wrap {
            log::trace!("d");

            log::trace!(
                "Cursor prev caused jump: above: {} bottom: {}",
                self.cursor_above,
                self.bottom
            );
            self.cursor_jump(self.end());
            return true;
        }

        false
    }

    /// Returns whether scroll wrap caused it to jump to start
    pub fn cursor_next(&mut self) -> bool {
        self.reset_current_scroll();

        if self.cursor_disabled {
            self.cursor_disabled = false
        }

        if self.cursor + 1 + self.scroll_padding() >= self.height
            && self.bottom + (self.height as u32) < self.status.matched_count
        {
            self.bottom += 1;
        } else if self.index() < self.end() {
            self.cursor += 1;
        } else if self.config.scroll_wrap {
            self.cursor_jump(0);
            return true;
        }
        false
    }

    pub fn cursor_jump(&mut self, index: u32) {
        self.reset_current_scroll();

        self.cursor_disabled = false;
        self.bottom_clip = None;

        let end = self.end();
        let index = index.min(end);

        if index < self.bottom as u32 || index >= self.bottom + self.height as u32 {
            self.bottom = (end + 1)
                .saturating_sub(self.height as u32) // don't exceed the first item of the last self.height items
                .min(index);
        }
        self.cursor = (index - self.bottom) as u16;
        log::debug!("cursor jumped to {}: {index}, end: {end}", self.cursor);
    }

    pub fn current_scroll(&mut self, x: i8, horizontal: bool) {
        if horizontal {
            self.hscroll = if x == 0 {
                0
            } else {
                self.hscroll.saturating_add(x)
            };
        } else {
            self.vscroll = if x == 0 {
                0
            } else if x.is_negative() {
                self.vscroll.saturating_add(x.unsigned_abs())
            } else {
                self.vscroll.saturating_sub(x as u8)
            };
        }
    }

    pub fn reset_current_scroll(&mut self) {
        self.hscroll = 0;
        self.vscroll = 0;
    }

    // ------- RENDERING ----------
    pub fn indentation(&self) -> usize {
        self.config.multi_prefix.width() + if self.config.icons { 2 } else { 0 }
    }
    pub fn col(&self) -> Option<usize> {
        self.col
    }

    /// Column widths.
    /// Note that the first width doesn't include the indentation.
    pub fn widths(&self) -> &Vec<u16> {
        &self.widths
    }

    /// Adapt the stored widths (initialized by [`Worker::results`]) to the fit within the available width (self.width)
    /// widths <= min_wrap_width don't shrink and aren't wrapped
    pub fn max_widths(&self) -> Vec<u16> {
        let mut base_widths = self.medians.clone();

        // uninitialized
        if base_widths.is_empty() || base_widths.iter().all(|x| *x == 0) {
            return vec![];
        }

        for w in base_widths.iter_mut() {
            *w = (*w).max(self.config.min_width);
        }

        base_widths.resize(self.hidden_columns.len().max(base_widths.len()), 0);

        for (i, is_hidden) in self.hidden_columns.iter().enumerate() {
            if *is_hidden {
                base_widths[i] = 0;
            }
        }

        let target = self.content_width();

        let sum: u16 = base_widths.iter().sum();

        if sum < target {
            let nonzero_count = base_widths.iter().filter(|w| **w > 0).count();

            let extra = target - sum;
            let extra_per_column = extra / nonzero_count as u16;
            let mut remainder = extra % nonzero_count as u16;

            for w in base_widths.iter_mut().filter(|w| **w > 0) {
                if *w > 0 {
                    *w += extra_per_column;

                    if remainder > 0 {
                        *w += 1;
                        remainder -= 1;
                    }
                }
            }
        }

        // log::trace!("base_widths: {:?}, target: {target}", base_widths);

        match allocate_widths(&base_widths, target, self.config.min_width) {
            Ok(s) | Err(s) => s,
        }
    }

    pub fn content_width(&self) -> u16 {
        self.width
            .saturating_sub(self.indentation() as u16)
            .saturating_sub(self.column_spacing_width())
    }

    pub fn column_spacing_width(&self) -> u16 {
        let pos = self.widths.iter().rposition(|&x| x != 0);
        self.config.column_spacing.0 * (pos.unwrap_or_default() as u16)
    }

    pub fn table_width(&self) -> u16 {
        if self.config.stacked_columns {
            self.width
        } else {
            self.widths.iter().sum::<u16>()
                + self.config.border.width()
                + self.column_spacing_width()
        }
    }

    // this updates the internal status, so be sure to call make_status afterward
    // some janky wrapping is implemented, dunno whats causing flickering, padding is fixed going down only
    pub fn make_table<'a, T: SSS>(
        &mut self,
        active_column: usize,
        worker: &'a mut Worker<T>,
        selector: &mut Selector<T, impl Selection>,
        matcher: &mut nucleo::Matcher,
        click: &mut Click,
        nav_bar_style: Option<(ratatui::widgets::BorderType, ratatui::style::Style)>,
        freeze_snapshot: bool,
    ) -> Table<'a> {
        let cwd = std::env::current_dir().unwrap_or_default();
        let offset = self.bottom as u32;
        let end = self.bottom + self.height as u32;
        let as_cols = !self.config.stacked_columns;

        let get_border_char = |is_first: bool, is_last: bool, border_type: ratatui::widgets::BorderType| -> &'static str {
            match border_type {
                ratatui::widgets::BorderType::Plain | ratatui::widgets::BorderType::Rounded => {
                    if is_first && is_last {
                        "│"
                    } else if is_first {
                        "╷"
                    } else if is_last {
                        "╵"
                    } else {
                        "│"
                    }
                },
                ratatui::widgets::BorderType::Double => "║",
                ratatui::widgets::BorderType::Thick | ratatui::widgets::BorderType::QuadrantOutside => {
                    if is_first && is_last {
                        "▌"
                    } else if is_first {
                        "▖"
                    } else if is_last {
                        "▘"
                    } else {
                        "▌"
                    }
                },
                ratatui::widgets::BorderType::QuadrantInside => {
                    if is_first && is_last {
                        "▐"
                    } else if is_first {
                        "▗"
                    } else if is_last {
                        "▝"
                    } else {
                        "▐"
                    }
                },
                _ => "│",
            }
        };

        let nav_bar_style_clone = nav_bar_style.clone();
        let get_nav_bar_span = move |is_first: bool, is_last: bool| -> Option<ratatui::text::Span<'static>> {
            nav_bar_style_clone.clone().map(|(border_type, style)| {
                let border_char = get_border_char(is_first, is_last, border_type);
                ratatui::text::Span::styled(border_char, style)
            })
        };

        let nav_bar_style_clone2 = nav_bar_style.clone();
        let multi_prefix = self.config.multi_prefix.clone();
        let get_dynamic_multi_prefix = move |is_first: bool, is_last: bool| -> String {
            if let Some((border_type, _)) = nav_bar_style_clone2 {
                let border_char = get_border_char(is_first, is_last, border_type);
                if let Some(_first_char) = multi_prefix.chars().next() {
                    let rest: String = multi_prefix.chars().skip(1).collect();
                    format!("{}{}", border_char, rest)
                } else {
                    format!("{} ", border_char)
                }
            } else {
                multi_prefix.clone()
            }
        };


        macro_rules! get_prefix {
            ($row:expr, $is_selected:expr, $idx:expr, $item:expr, $columns:expr, $is_first:expr, $is_last:expr) => {{
                let mut icon_name = String::new();
                let mut is_spinner = false;
                let mut spinner_col_idx = 0;

                if !$row.is_empty() && !self.config.spinner_prefix.is_empty() {
                    for (i, col_text) in $row.iter().enumerate() {
                        let text_content = extract_col0_name(col_text);
                        if text_content.contains(&self.config.spinner_prefix) {
                            is_spinner = true;
                            spinner_col_idx = i;
                            break;
                        }
                    }
                }

                if !$row.is_empty() {
                    icon_name = $columns[0].raw($item).into_owned();
                    if is_spinner && spinner_col_idx == 0 {
                        icon_name = icon_name.replace(&self.config.spinner_prefix, "");
                    }
                }

                if is_spinner {
                    if self.config.spinner_inline {
                        let frame =
                            crate::spinner::Spinner::from_name(&self.config.spinner).current_frame();
                        crate::utils::text::replace_string_in_text(
                            &mut $row[spinner_col_idx],
                            &self.config.spinner_prefix,
                            &format!(" {frame}"),
                        );
                    } else {
                        crate::utils::text::strip_string_from_text(
                            &mut $row[spinner_col_idx],
                            &self.config.spinner_prefix,
                        );
                    }
                }
                let is_yanked = if !icon_name.is_empty() {
                    let path = std::path::Path::new(&icon_name);
                    let abs_path = if path.is_absolute() {
                        path.to_path_buf()
                    } else {
                        cwd.join(path)
                    };
                    self.yank_paths.contains(&abs_path.to_string_lossy().to_string())
                } else {
                    false
                };

                let is_cut = if !icon_name.is_empty() {
                    let path = std::path::Path::new(&icon_name);
                    let abs_path = if path.is_absolute() {
                        path.to_path_buf()
                    } else {
                        cwd.join(path)
                    };
                    self.cut_paths.contains(&abs_path.to_string_lossy().to_string())
                } else {
                    false
                };

                let prefix = if is_spinner && !self.config.spinner_inline {
                    let frame =
                        crate::spinner::Spinner::from_name(&self.config.spinner).current_frame();
                    let f = format!("{frame} ");
                    crate::utils::string::fit_width(&f, self.config.multi_prefix.width())
                } else if $is_selected || is_yanked || is_cut {
                    get_dynamic_multi_prefix($is_first, $is_last)
                } else {
                    self.default_prefix($idx)
                };
                (prefix, icon_name, is_spinner, spinner_col_idx, is_yanked, is_cut)
            }};
        }

        let width_limits = if as_cols {
            self.max_widths()
        } else {
            let default = self.width.saturating_sub(self.indentation() as u16);

            (0..worker.columns.len())
                .map(|i| {
                    if self.hidden_columns.get(i).copied().unwrap_or(false) {
                        0
                    } else {
                        default
                    }
                })
                .collect()
        };

        let columns = worker.columns.clone();
        let (mut results, mut widths, medians, status) = worker.results(
            offset,
            end,
            &width_limits,
            self.config.wrap,
            self.config.max_height,
            self.config.match_style.into(),
            matcher,
            self.config.autoscroll.clone(),
            self.hscroll,
            (
                if self.config.vscroll_current_only {
                    0
                } else {
                    self.vscroll
                },
                !as_cols,
            ),
            self.config.show_skipped,
            freeze_snapshot,
        );
        let results_len = results.len();

        // log::trace!(
        //     "len: {}, hscroll: {},  offset: {}, end: {}, limits: {:?}, medians: {:?}, last_widths: {:?}",
        //     results.len(),
        //     self.hscroll,
        //     offset,
        //     end,
        //     width_limits,
        //     medians,
        //     self.widths
        // );

        self.status = status.clone();
        self.medians = medians;
        widths[0] += self.indentation() as u16;

        // When symlink targets are enabled, expand column 0 to use all
        // remaining horizontal space so the annotation has room to display.
        if self.config.symlink_target {
            let other_cols: u16 = widths[1..].iter().sum();
            let col0_max = self
                .width
                .saturating_sub(other_cols)
                .saturating_sub(self.column_spacing_width());
            widths[0] = widths[0].max(col0_max);
        }

        // should generally be true already, but act as a safeguard
        // for x in widths.iter_mut().zip(&self.hidden_columns) {
        //     if *x.1 {
        //         *x.0 = 0
        //     }
        // }
        let widths = widths;

        let match_count = status.matched_count;

        if match_count < self.bottom + self.cursor as u32 && !self.cursor_disabled {
            self.cursor_jump(match_count);
        } else {
            self.cursor = self.cursor.min(results.len().saturating_sub(1) as u16)
        }

        let mut rows = vec![];
        let mut total_height = 0;

        if results.is_empty() {
            return Table::new(rows, widths);
        }

        let height_of = |t: &(Option<std::sync::Arc<str>>, Vec<ratatui::text::Text<'a>>, _)| {
            let group_h = if t.0.is_some() { 1 } else { 0 };
            group_h
                + self._hr()
                + if as_cols {
                    t.1.iter()
                        .map(|t| t.height() as u16)
                        .max()
                        .unwrap_or_default()
                } else {
                    t.1.iter().map(|t| t.height() as u16).sum::<u16>()
                }
        };

        let style_text = |mut t: ratatui::text::Text<'a>, x: usize, is_current_row: bool| {
            let is_active_col = active_column == x;
            match self.config.row_connection {
                RowConnectionStyle::Disjoint => {
                    if is_active_col {
                        t = t.style(if is_current_row {
                            self.config.current_style
                        } else {
                            self.config.style
                        });
                    } else {
                        t = t.style(if is_current_row {
                            self.config.inactive_current_style
                        } else {
                            self.config.inactive_style
                        });
                    }
                }
                RowConnectionStyle::Capped => {
                    if is_active_col {
                        t = t.style(if is_current_row {
                            self.config.current_style
                        } else {
                            self.config.style
                        });
                    }
                }
                RowConnectionStyle::Full => {}
            }
            t
        };

        // log::trace!("results initial: {}, {}, {}, {}, {}", self.bottom, self.cursor, total_height, self.height, results.len());
        let h_at_cursor = height_of(&results[self.cursor as usize]);
        let h_after_cursor = results[self.cursor as usize + 1..]
            .iter()
            .map(height_of)
            .sum();
        let h_to_cursor = results[0..self.cursor as usize]
            .iter()
            .map(height_of)
            .sum::<u16>();
        let cursor_end_should_lte = self.height - self.scroll_padding().min(h_after_cursor);
        // let cursor_start_should_gt = self.scroll_padding().min(h_to_cursor);

        // log::trace!(
        //     "Computed heights: {}, {h_at_cursor}, {h_to_cursor}, {h_after_cursor}, {cursor_end_should_lte}",
        //     self.cursor
        // );

        // begin adjustment
        let mut start_index = 0; // the index in results of the first complete item
        let is_current_row = false;
        if h_at_cursor >= cursor_end_should_lte {
            start_index = self.cursor;
            self.bottom += self.cursor as u32;
            self.cursor = 0;
            self.cursor_above = 0;
            self.bottom_clip = None;
        } else
        // increase the bottom index so that cursor_should_above is maintained
        if let h_to_cursor_end = h_to_cursor + h_at_cursor
            && h_to_cursor_end > cursor_end_should_lte
        {
            let mut trunc_height = h_to_cursor_end - cursor_end_should_lte;
            // note that there is a funny side effect that scrolling up near the bottom can scroll up a bit, but it seems fine to me

            for r in results[start_index as usize..self.cursor as usize].iter_mut() {
                let h = height_of(r);
                let (_, row, item) = r;
                start_index += 1; // we always skip at least the first item

                if trunc_height < h {
                    let mut remaining_height = h - trunc_height;
                    let is_selected = selector.contains(item);
                    let is_first = rows.is_empty();
                    let is_last = (self.height <= total_height + remaining_height) || (start_index as usize >= results_len);
                    let nav_bar_span = get_nav_bar_span(is_first, is_last);
                    let (prefix, icon_name, is_spinner, spinner_col_idx, is_yanked, is_cut) = get_prefix!(row, is_selected, 0, item, columns, is_first, is_last);

                    total_height += remaining_height;

                    // log::debug!("r: {remaining_height}");
                    if as_cols {
                        if remaining_height < h - self._hr() {
                            for (_, t) in
                                row.iter_mut().enumerate().filter(|(i, _)| widths[*i] != 0)
                            {
                                clip_text_lines(t, remaining_height, !self.reverse());
                            }
                        }

                        let last_visible = widths
                            .iter()
                            .enumerate()
                            .rev()
                            .find_map(|(i, w)| (*w != 0).then_some(i));

                        let mut row_texts: Vec<_> = row
                            .iter()
                            .take(last_visible.map(|x| x + 1).unwrap_or(0))
                            .cloned()
                            .enumerate()
                            .map(|(x, mut t)| {
                                t = style_text(t, x, is_current_row);
                                if self.config.dim_directory_path && x == 0 {
                                    apply_dim_directory_path(&mut t, self.config.directory_path_style.into());
                                }
                                if x == spinner_col_idx {
                                    prefix_span(
                                        &mut t,
                                        prefix.clone(),
                                        self.active_prefix_style(
                                            &icon_name,
                                            is_selected,
                                            is_spinner,
                                            &cwd,
                                        ),
                                        self.inactive_prefix_style(
                                            &icon_name,
                                            is_selected,
                                            is_spinner,
                                            &cwd,
                                        ),
                                        is_current_row,
                                        if !is_selected && !is_yanked && !is_cut {
                                            nav_bar_span.clone()
                                        } else {
                                            None
                                        },
                                    );
                                    if self.config.icons {
                                        insert_icon_span(
                                            &mut t,
                                            &icon_name,
                                            !is_selected && !is_yanked && !is_cut && nav_bar_span.is_some(),
                                        );
                                    }
                                    if self.config.symlink_target {
                                        maybe_append_symlink_target(
                                            &mut t,
                                            &icon_name,
                                            self.config.symlink_target_style.into(),
                                            widths[spinner_col_idx],
                                        );
                                    }
                                }
                                t
                            })
                            .collect();

                        if self.config.right_align_last && row_texts.len() > 1 {
                            row_texts.last_mut().unwrap().alignment = Some(Alignment::Right)
                        }

                        let row = Row::new(row_texts).height(remaining_height);
                        let row = if is_selected {
                            row.style(Style::from(self.config.selected_style))
                        } else {
                            row
                        };
                        rows.push(row);
                    } else {
                        let col_count = row.len();
                        let mut push = vec![];

                        for (rev_i, mut col) in row.into_iter().rev().enumerate() {
                            let col_idx = col_count.saturating_sub(1 + rev_i);
                            let mut height = col.height() as u16;
                            if remaining_height == 0 {
                                break;
                            } else if remaining_height < height {
                                clip_text_lines(&mut col, remaining_height, !self.reverse());
                                height = remaining_height;
                            }
                            remaining_height -= height;

                            if self.config.dim_directory_path && col_idx == 0 {
                                apply_dim_directory_path(col, self.config.directory_path_style.into());
                            }
                            prefix_span(
                                &mut col,
                                prefix.clone(),
                                self.active_prefix_style(&icon_name, is_selected, is_spinner, &cwd),
                                self.inactive_prefix_style(
                                    &icon_name,
                                    is_selected,
                                    is_spinner,
                                    &cwd,
                                ),
                                is_current_row,
                                if !is_selected && !is_yanked && !is_cut {
                                    nav_bar_span.clone()
                                } else {
                                    None
                                },
                            );
                            if self.config.icons && col_idx == 0 {
                                insert_icon_span(
                                    &mut col,
                                    &icon_name,
                                    !is_selected && !is_yanked && !is_cut && nav_bar_span.is_some(),
                                );
                            }
                            if self.config.symlink_target && col_idx == 0 {
                                maybe_append_symlink_target(
                                    col,
                                    &icon_name,
                                    self.config.symlink_target_style.into(),
                                    self.width,
                                );
                            }

                            let row = Row::new(vec![col.clone()]).height(height);
                            let row = if is_selected {
                                row.style(Style::from(self.config.selected_style))
                            } else {
                                row
                            };
                            push.push(row);
                        }
                        rows.extend(push.into_iter().rev());
                    }

                    self.bottom += start_index as u32 - 1;
                    self.cursor -= start_index - 1;
                    self.bottom_clip = Some(remaining_height);
                    break;
                } else if trunc_height == h {
                    self.bottom += start_index as u32;
                    self.cursor -= start_index;
                    self.bottom_clip = None;
                    break;
                }

                trunc_height -= h;
            }
        } else if let Some(mut remaining_height) = self.bottom_clip {
            start_index += 1;
            // same as above
            let h = height_of(&results[0]);
            let (_, row, item) = &mut results[0];
            let is_selected = selector.contains(item);
            let is_first = rows.is_empty();
            let is_last = (self.height <= total_height + remaining_height) || (start_index as usize >= results_len);
            let nav_bar_span = get_nav_bar_span(is_first, is_last);
            let (prefix, icon_name, is_spinner, spinner_col_idx, is_yanked, is_cut) = get_prefix!(row, is_selected, 0, item, columns, is_first, is_last);

            total_height += remaining_height;

            if as_cols {
                if self._hr() + remaining_height != h {
                    for (_, t) in row.iter_mut().enumerate().filter(|(i, _)| widths[*i] != 0) {
                        clip_text_lines(t, remaining_height, !self.reverse());
                    }
                }

                let last_visible = widths
                    .iter()
                    .enumerate()
                    .rev()
                    .find_map(|(i, w)| (*w != 0).then_some(i));

                let mut row_texts: Vec<_> = row
                    .iter()
                    .take(last_visible.map(|x| x + 1).unwrap_or(0))
                    .cloned()
                    .enumerate()
                    .map(|(x, mut t)| {
                        t = style_text(t, x, is_current_row);
                        if x == spinner_col_idx {
                            prefix_span(
                                &mut t,
                                prefix.clone(),
                                self.active_prefix_style(&icon_name, is_selected, is_spinner, &cwd),
                                self.inactive_prefix_style(
                                    &icon_name,
                                    is_selected,
                                    is_spinner,
                                    &cwd,
                                ),
                                is_current_row,
                                if !is_selected && !is_yanked && !is_cut {
                                    nav_bar_span.clone()
                                } else {
                                    None
                                },
                            );
                            if self.config.icons {
                                insert_icon_span(
                                    &mut t,
                                    &icon_name,
                                    !is_selected && !is_yanked && !is_cut && nav_bar_span.is_some(),
                                );
                            }
                            if self.config.symlink_target {
                                maybe_append_symlink_target(
                                    &mut t,
                                    &icon_name,
                                    self.config.symlink_target_style.into(),
                                    widths[spinner_col_idx],
                                );
                            }
                        }
                        t
                    })
                    .collect();

                if self.config.right_align_last && row_texts.len() > 1 {
                    row_texts.last_mut().unwrap().alignment = Some(Alignment::Right)
                }

                let row = Row::new(row_texts).height(remaining_height);
                let row = if is_selected && !is_current_row {
                    row.style(Style::from(self.config.selected_style))
                } else {
                    row
                };
                rows.push(row);
            } else {
                let col_count = row.len();
                let mut push = vec![];

                for (rev_i, mut col) in row.into_iter().rev().enumerate() {
                    let col_idx = col_count.saturating_sub(1 + rev_i);
                    let mut height = col.height() as u16;
                    if remaining_height == 0 {
                        break;
                    } else if remaining_height < height {
                        clip_text_lines(&mut col, remaining_height, !self.reverse());
                        height = remaining_height;
                    }
                    remaining_height -= height;

                    prefix_span(
                        &mut col,
                        prefix.clone(),
                        self.active_prefix_style(&icon_name, is_selected, is_spinner, &cwd),
                        self.inactive_prefix_style(&icon_name, is_selected, is_spinner, &cwd),
                        is_current_row,
                        if !is_selected && !is_yanked && !is_cut {
                            nav_bar_span.clone()
                        } else {
                            None
                        },
                    );
                    if self.config.icons && col_idx == 0 {
                        insert_icon_span(&mut col, &icon_name, !is_selected && !is_yanked && !is_cut && nav_bar_span.is_some());
                    }
                    if self.config.symlink_target && col_idx == 0 {
                        maybe_append_symlink_target(
                            col,
                            &icon_name,
                            self.config.symlink_target_style.into(),
                            self.width,
                        );
                    }

                    let row = Row::new(vec![col.clone()]).height(height);
                    let row = if is_selected && !is_current_row {
                        row.style(Style::from(self.config.selected_style))
                    } else {
                        row
                    };
                    push.push(row);
                }
                rows.extend(push.into_iter().rev());
            }
        }

        // topside padding is not self-correcting, and can only do its best to stay at #padding lines without obscuring cursor on cursor movement events.
        let mut remaining_height = self.height.saturating_sub(total_height);

        let mut i = self.bottom_clip.is_some() as usize;

        let mut drain_iter = results.drain(start_index as usize..).peekable();
        while let Some((group, mut row, item)) = drain_iter.next() {
            let is_current_row = self.is_current(i);
            // note that the index changes *next* frame
            if let Click::ResultPos(c) = click {
                let c = if self.reverse() {
                    self.height.saturating_sub(*c).saturating_sub(1)
                } else {
                    *c
                };
                if self.height - remaining_height > c {
                    let idx = self.bottom as u32 + i as u32 - 1;
                    log::debug!(
                        "Mapped click position to index: {c} -> {idx} with remaining {remaining_height}",
                    );
                    *click = Click::ResultIdx(idx);
                }
            }

            if self.is_current(i) {
                self.cursor_above = self.height - remaining_height;
            }

            // insert group header
            if let Some(group) = group {
                if remaining_height > 0 {
                    let group_style: Style = self.config.group_header_style.into();
                    let line = ratatui::text::Line::from(vec![
                        Span::raw(" "),
                        Span::styled(group.to_string(), group_style),
                    ]);
                    let row = if as_cols {
                        let last_visible = widths
                            .iter()
                            .enumerate()
                            .rev()
                            .find_map(|(i, w)| (*w != 0).then_some(i))
                            .unwrap_or(0);
                        let mut cells = vec![];
                        for i in 0..widths.len() {
                            if i == last_visible {
                                cells.push(ratatui::widgets::Cell::from(line.clone()));
                            } else {
                                cells.push(ratatui::widgets::Cell::from(""));
                            }
                        }
                        Row::new(cells).height(1)
                    } else {
                        Row::new(vec![line]).height(1)
                    };
                    rows.push(row);
                    remaining_height = remaining_height.saturating_sub(1);
                }
            }
            if remaining_height == 0 {
                break;
            }

            // insert hr
            if let Some(hr) = self.hr()
                && remaining_height > 0
            {
                rows.push(hr);
                remaining_height -= self._hr();
            }
            if remaining_height == 0 {
                break;
            }

            // determine prefix
            let is_selected = selector.contains(item);
            let is_first = rows.is_empty();
            let is_last_in_results = drain_iter.peek().is_none();
            let h = if as_cols {
                row.iter().map(|t| t.height() as u16).max().unwrap_or_default()
            } else {
                row.iter().map(|t| t.height() as u16).sum::<u16>()
            };
            let is_last = is_last_in_results || (remaining_height <= h);
            let nav_bar_span = get_nav_bar_span(is_first, is_last);
            let (prefix, icon_name_hz, is_spinner, spinner_col_idx, is_yanked, is_cut) = get_prefix!(row, is_selected, i, item, columns, is_first, is_last);

            if as_cols {
                // scroll down
                if self.is_current(i) && self.config.vscroll_current_only && self.vscroll > 0 {
                    for (x, t) in row.iter_mut().enumerate().filter(|(i, _)| widths[*i] != 0) {
                        if self.col.is_none() || self.col() == Some(x) {
                            let scroll = self.vscroll as usize;

                            if scroll < t.lines.len() {
                                t.lines = t.lines.split_off(scroll);
                            } else {
                                t.lines.clear();
                            }
                        }
                    }
                }

                let mut height = row
                    .iter()
                    .map(|t| t.height() as u16)
                    .max()
                    .unwrap_or_default();

                if remaining_height < height {
                    height = remaining_height;

                    for (_, t) in row.iter_mut().enumerate().filter(|(i, _)| widths[*i] != 0) {
                        clip_text_lines(t, height, self.reverse());
                    }
                }
                remaining_height -= height;

                // same as above
                let last_visible = widths
                    .iter()
                    .enumerate()
                    .rev()
                    .find_map(|(i, w)| (*w != 0).then_some(i));

                let mut row_texts: Vec<_> = row
                    .iter()
                    .take(last_visible.map(|x| x + 1).unwrap_or(0))
                    .cloned()
                    // highlight
                    .enumerate()
                    .map(|(x, mut t)| {
                        t = style_text(t, x, self.is_current(i));

                        // prefix after hscroll
                        if x == spinner_col_idx {
                            prefix_span(
                                &mut t,
                                prefix.clone(),
                                self.active_prefix_style(
                                    &icon_name_hz,
                                    is_selected,
                                    is_spinner,
                                    &cwd,
                                ),
                                self.inactive_prefix_style(
                                    &icon_name_hz,
                                    is_selected && !is_current_row,
                                    is_spinner,
                                    &cwd,
                                ),
                                is_current_row,
                                if !is_selected && !is_yanked && !is_cut {
                                    nav_bar_span.clone()
                                } else {
                                    None
                                },
                            );
                            if self.config.icons {
                                insert_icon_span(
                                    &mut t,
                                    &icon_name_hz,
                                    !is_selected && !is_yanked && !is_cut && nav_bar_span.is_some(),
                                );
                            }
                            if self.config.symlink_target {
                                maybe_append_symlink_target(
                                    &mut t,
                                    &icon_name_hz,
                                    self.config.symlink_target_style.into(),
                                    widths[spinner_col_idx],
                                );
                            }
                        };
                        t
                    })
                    .collect();

                if self.config.right_align_last && row_texts.len() > 1 {
                    row_texts.last_mut().unwrap().alignment = Some(Alignment::Right)
                }

                // push
                let mut row = Row::new(row_texts).height(height);

                if self.is_current(i) {
                    match self.config.row_connection {
                        RowConnectionStyle::Capped => {
                            row = row.style(self.config.inactive_current_style)
                        }
                        RowConnectionStyle::Full => row = row.style(self.config.current_style),
                        _ => {}
                    }
                } else if is_selected {
                    row = row.style(Style::from(self.config.selected_style));
                }

                rows.push(row);
            } else {
                let mut push = vec![];
                let mut vscroll_to_skip = if self.is_current(i) && self.config.vscroll_current_only
                {
                    self.vscroll as usize
                } else {
                    0
                };

                for (x, mut col) in row.into_iter().enumerate() {
                    if vscroll_to_skip > 0 {
                        let col_height = col.lines.len();
                        if vscroll_to_skip >= col_height {
                            vscroll_to_skip -= col_height;
                            continue;
                        } else {
                            col.lines = col.lines.split_off(vscroll_to_skip);
                            vscroll_to_skip = 0;
                        }
                    }

                    let mut height = col.height() as u16;

                    if remaining_height == 0 {
                        break;
                    } else if remaining_height < height {
                        height = remaining_height;
                        clip_text_lines(&mut col, remaining_height, self.reverse());
                    }
                    remaining_height -= height;

                    let is_current_row = self.is_current(i);
                    prefix_span(
                        &mut col,
                        prefix.clone(),
                        self.active_prefix_style(&icon_name_hz, is_selected, is_spinner, &cwd),
                        self.inactive_prefix_style(
                            &icon_name_hz,
                            is_selected && !is_current_row,
                            is_spinner,
                            &cwd,
                        ),
                        is_current_row,
                        if !is_selected && !is_yanked && !is_cut {
                            nav_bar_span.clone()
                        } else {
                            None
                        },
                    );
                    if self.config.icons && x == 0 {
                        insert_icon_span(
                            &mut col,
                            &icon_name_hz,
                            !is_selected && !is_yanked && !is_cut && nav_bar_span.is_some(),
                        );
                    }
                    if self.config.symlink_target && x == 0 {
                        maybe_append_symlink_target(
                            &mut col,
                            &icon_name_hz,
                            self.config.symlink_target_style.into(),
                            self.width,
                        );
                    }

                    let is_active_col = active_column == x;

                    match self.config.row_connection {
                        RowConnectionStyle::Disjoint => {
                            if is_active_col {
                                col = col.style(if is_current_row {
                                    self.config.current_style
                                } else {
                                    self.config.style
                                });
                            } else {
                                col = col.style(if is_current_row {
                                    self.config.inactive_current_style
                                } else {
                                    self.config.inactive_style
                                });
                            }
                        }
                        RowConnectionStyle::Capped => {
                            if is_active_col {
                                col = col.style(if is_current_row {
                                    self.config.current_style
                                } else {
                                    self.config.style
                                });
                            }
                        }
                        RowConnectionStyle::Full => {}
                    }

                    // push
                    let mut row = Row::new(vec![col]).height(height);
                    if is_current_row {
                        match self.config.row_connection {
                            RowConnectionStyle::Capped => {
                                row = row.style(self.config.inactive_current_style)
                            }
                            RowConnectionStyle::Full => row = row.style(self.config.current_style),
                            _ => {}
                        }
                    } else if is_selected {
                        row = row.style(Style::from(self.config.selected_style));
                    }
                    push.push(row);
                }
                rows.extend(push);
            }
            i += 1;
        }

        // doesn't loop back after results is exhausted so we have to set here
        if let Click::ResultPos(_c) = click {
            log::debug!("Mapped click to last row = {i}");
            *click = Click::ResultIdx(self.bottom as u32 + i as u32 - 1);
        }

        if self.reverse() {
            rows.reverse();
            if remaining_height > 0 {
                rows.insert(0, Row::new(vec![vec![]]).height(remaining_height));
            }
        }

        // ratatui column_spacing eats into the constraints
        let table_widths = if as_cols {
            // first 0 element after which all is 0
            let pos = widths.iter().rposition(|&x| x != 0);
            // column_spacing eats into the width
            let mut widths: Vec<_> = widths[..pos.map_or(0, |x| x + 1)].to_vec();

            let surplus = self.content_width().saturating_sub(widths.iter().sum());

            if surplus > 0 {
                // occupy full row
                if matches!(self.config.row_connection, RowConnectionStyle::Full)
                    || (matches!(self.config.row_connection, RowConnectionStyle::Disjoint)
                        && self.config.right_align_last)
                {
                    if let Some(s) = widths.last_mut() {
                        *s += surplus;
                    }
                }
            }

            // save actual widths of each column
            self.widths = widths.clone();

            widths
        } else {
            vec![self.width]
        };

        // log::trace!(
        //     "limits: {width_limits:?}, widths: {widths:?}, {:?}, medians {:?}",
        //     self.width,
        //     self.medians
        // );

        let mut table = Table::new(rows, table_widths).column_spacing(self.config.column_spacing.0);

        table = match self.config.row_connection {
            RowConnectionStyle::Full => table.style(self.config.style),
            RowConnectionStyle::Capped => table.style(self.config.inactive_style),
            _ => table,
        };

        // log::trace!("{table:?}");

        if !self.config.border.is_empty() {
            table = table.block(self.config.border.as_static_block());
        }
        table
    }
}

impl ResultsUI {
    pub fn make_status(&self, full_width: u16) -> Paragraph<'_> {
        let status_config = &self.status_config;
        let replacements = [
            ('r', self.index().to_string()),
            ('m', self.status.matched_count.to_string()),
            ('t', self.status.item_count.to_string()),
        ];

        // sub replacements into line
        let mut new_spans = Vec::new();

        if status_config.match_indent {
            new_spans.push(Span::raw(" ".repeat(self.indentation())));
        }

        for span in &self.status_template {
            let subbed = substitute_escaped(&span.content, &replacements);
            new_spans.push(Span::styled(subbed, span.style));
        }

        let substituted_line = Line::from(new_spans);

        // sub whitespace expansions
        let effective_width = match self.status_config.row_connection {
            RowConnectionStyle::Full => full_width,
            _ => self.width,
        } as usize;
        let expanded = expand_indents(substituted_line, r"\s", r"\S", effective_width)
            .style(status_config.style);

        Paragraph::new(expanded)
    }

    /// Returns just the substituted status spans as a `Line`, without width
    /// expansion or indentation — suitable for embedding inline in the input bar.
    pub fn status_line(&self) -> Line<'_> {
        let replacements = [
            ('r', self.index().to_string()),
            ('m', self.status.matched_count.to_string()),
            ('t', self.status.item_count.to_string()),
        ];

        let spans: Vec<Span<'_>> = self
            .status_template
            .iter()
            .map(|span| {
                let subbed = substitute_escaped(&span.content, &replacements);
                Span::styled(subbed, span.style)
            })
            .collect();

        Line::from(spans).style(self.status_config.style)
    }

    /// The style from the config overrides the Line style (but not the span styles).
    /// None restores the prompt defined in the config.
    pub fn set_status_line(&mut self, template: Option<Line<'static>>) {
        let status_config = &self.status_config;
        log::trace!("status line: {template:?}");

        self.status_template = template
            .unwrap_or(status_config.template.clone().into())
            .style(status_config.style)
            .into()
    }
}

// helpers
impl ResultsUI {
    fn default_prefix(&self, i: usize) -> String {
        let substituted = substitute_escaped(
            &self.config.unselected_prefix,
            &[
                ('d', &(i + 1).to_string()),                        // cursor index
                ('r', &(i + 1 + self.bottom as usize).to_string()), // absolute index
            ],
        );

        fit_width(&substituted, self.config.multi_prefix.width())
    }

    fn is_current(&self, i: usize) -> bool {
        !self.cursor_disabled && self.cursor == i as u16
    }

    fn hr(&self) -> Option<Row<'static>> {
        let sep = self.config.separator;

        if matches!(sep, HorizontalSeparator::None) {
            return None;
        }

        let unit = sep.as_str();
        let line = unit.repeat(self.width as usize);

        // todo: support non_stacked properly by doing a seperate rendering pass
        if !self.config.stacked_columns && self.widths.len() > 1 {
            // Some(Row::new(vec![vec![]]))
            Some(Row::new(vec![line; self.widths().len()]).style(self.config.separator_style))
        } else {
            Some(Row::new(vec![line]).style(self.config.separator_style))
        }
    }

    fn _hr(&self) -> u16 {
        !matches!(self.config.separator, HorizontalSeparator::None) as u16
    }
}

pub struct StatusUI {}

impl StatusUI {
    pub fn parse_template_to_status_line(s: &str) -> Line<'static> {
        let parts = match split_on_nesting(&s, ['{', '}']) {
            Ok(x) => x,
            Err(n) => {
                if n > 0 {
                    log::error!("Encountered {} unclosed parentheses", n)
                } else {
                    log::error!("Extra closing parenthesis at index {}", -n)
                }
                return Line::from(s.to_string());
            }
        };

        let mut spans = Vec::new();
        let mut in_nested = !s.starts_with('{');
        for part in parts {
            in_nested = !in_nested;
            let content = part.as_str();

            if in_nested {
                let inner = &content[1..content.len() - 1];

                // perform replacement fg:content
                spans.push(Self::span_from_template(inner));
            } else {
                spans.push(Span::raw(content.to_string()));
            }
        }

        Line::from(spans)
    }

    /// Converts a template string into a `Span` with colors and modifiers.
    ///
    /// The template string format is:
    /// ```text
    /// "style1,style2,...:text"
    /// ```
    /// - The **first valid color** token is used as foreground (fg).
    /// - The **second valid color** token is used as background (bg).
    /// - Remaining tokens are interpreted as **modifiers**: bold, dim, italic, underlined,
    ///   slow_blink, rapid_blink, reversed, hidden, crossed_out.
    /// - Empty tokens are ignored.
    /// - Unrecognized tokens are collected and logged once at the end.
    ///
    /// # Examples
    ///
    /// ```
    /// use matchmaker::ui::StatusUI;
    /// StatusUI::span_from_template("red,bg=blue,bold,italic:Hello");
    /// StatusUI::span_from_template("green,,underlined:World");
    /// StatusUI::span_from_template(",,dim:OnlyDim");
    /// ```
    ///
    /// Returns a `Span` with the specified styles applied to the text.
    pub fn span_from_template(inner: &str) -> Span<'static> {
        use std::str::FromStr;

        let (style_part, text) = inner.split_once(':').unwrap_or(("", inner));

        let mut style = Style::default();
        let mut fg_set = false;
        let mut bg_set = false;
        let mut unknown_tokens = Vec::new();

        for token in style_part.split(',') {
            let token = token.trim();
            if token.is_empty() {
                fg_set = true;
                continue;
            }

            if !fg_set {
                if let Ok(color) = Color::from_str(token) {
                    style = style.fg(color);
                    fg_set = true;
                    continue;
                }
            }

            if !bg_set {
                if let Ok(color) = Color::from_str(token) {
                    style = style.bg(color);
                    bg_set = true;
                    continue;
                }
            }

            match token.to_lowercase().as_str() {
                "bold" => {
                    style = style.add_modifier(Modifier::BOLD);
                }
                "dim" => {
                    style = style.add_modifier(Modifier::DIM);
                }
                "italic" => {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                "underlined" => {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                "slow_blink" => {
                    style = style.add_modifier(Modifier::SLOW_BLINK);
                }
                "rapid_blink" => {
                    style = style.add_modifier(Modifier::RAPID_BLINK);
                }
                "reversed" => {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                "hidden" => {
                    style = style.add_modifier(Modifier::HIDDEN);
                }
                "crossed_out" => {
                    style = style.add_modifier(Modifier::CROSSED_OUT);
                }
                _ => unknown_tokens.push(token.to_string()),
            };
        }

        if !unknown_tokens.is_empty() {
            log::warn!("Unknown style tokens: {:?}", unknown_tokens);
        }

        Span::styled(text.to_string(), style)
    }
}

// ---------- icon helpers ----------

/// Append a symlink-target annotation to the **first line** of `col`.
///
/// Reads the link target with `std::fs::read_link`. If the path is not a
/// symlink (or the read fails) the function is a no-op. The annotation is
/// rendered as `" \u{f061} <target>"` using `style`, truncated with `…` if
/// it would overflow `max_width`.
fn maybe_append_symlink_target(
    col: &mut ratatui::text::Text<'_>,
    name: &str,
    style: ratatui::style::Style,
    max_width: u16,
) {
    let path = std::path::Path::new(name.trim());
    if let Ok(target) = std::fs::read_link(path) {
        let target_str = target.to_string_lossy().into_owned();
        let arrow = " \u{f061} ";
        let arrow_width = arrow.width();

        // Measure how much width the first line already occupies.
        let used: usize = col
            .lines
            .first()
            .map(|l| l.spans.iter().map(|s| s.content.width()).sum())
            .unwrap_or(0);

        let remaining = (max_width as usize).saturating_sub(used);

        // Need at least space for the arrow + 1 char to show anything useful.
        if remaining < arrow_width + 1 {
            return;
        }

        let budget = remaining - arrow_width;
        let annotation = if target_str.width() <= budget {
            format!("{arrow}{target_str}")
        } else {
            // Truncate target to budget - 1 chars + "…"
            let mut truncated = String::new();
            let mut w = 0;
            for g in unicode_segmentation::UnicodeSegmentation::graphemes(target_str.as_str(), true)
            {
                let gw = g.width();
                if w + gw + 1 > budget {
                    break;
                }
                truncated.push_str(g);
                w += gw;
            }
            format!("{arrow}{truncated}…")
        };

        let span = ratatui::text::Span::styled(annotation, style);
        if let Some(line) = col.lines.first_mut() {
            line.spans.push(span);
        }
    }
}

/// Extract the plain-text content of the first line of a `Text` cell (before
/// any prefix span has been inserted). Used to determine which file-type icon
/// to display.
fn extract_col0_name(col: &ratatui::text::Text<'_>) -> String {
    col.lines
        .first()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .unwrap_or_default()
}

/// Insert a Nerd-Font icon span after the prefix in every line of `col`.
/// Callers must ensure `prefix_span` has already been called.
fn insert_icon_span(col: &mut ratatui::text::Text<'_>, name: &str, has_nav_bar: bool) {
    let (icon, color) = icon_for_name(name);
    let icon_span = ratatui::text::Span::styled(
        format!("{icon} "),
        ratatui::style::Style::default().fg(color),
    );
    let index = if has_nav_bar { 2 } else { 1 };
    for line in col.lines.iter_mut() {
        line.spans
            .insert(index.min(line.spans.len()), icon_span.clone());
    }
}

/// Return the Nerd-Font glyph and colour for a given file/directory name.
///
/// Lookup order: directory → symlink → known basename → file extension →
/// generic file fallback.
fn icon_for_name(name: &str) -> (char, Color) {
    use std::path::Path;
    let path = Path::new(name.trim());

    // Directory
    if std::fs::metadata(path).is_ok_and(|m| m.is_dir()) {
        return ('\u{f115}', Color::Blue); // nf-fa-folder_open
    }
    // Symlink
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return ('\u{f482}', Color::Cyan); // nf-mdi-link
    }

    let basename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(name.trim());

    match basename.to_lowercase().as_str() {
        "cargo.toml" | "cargo.lock" => return ('\u{e7a8}', Color::Red),
        "package.json" | "package-lock.json" | "yarn.lock" => return ('\u{e74e}', Color::Green),
        "makefile" | "gnumakefile" => return ('\u{e779}', Color::Yellow),
        "dockerfile" => return ('\u{e7b0}', Color::Cyan),
        ".gitignore" | ".gitmodules" | ".gitattributes" => return ('\u{e702}', Color::Red),
        "readme.md" | "readme.txt" | "readme" => return ('\u{e73e}', Color::Blue),
        "license" | "license.md" | "license.txt" => return ('\u{f02d}', Color::Yellow),
        _ => {}
    }

    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "rs" => ('\u{e7a8}', Color::Red),
        "toml" => ('\u{e6b2}', Color::Gray),
        "json" => ('\u{e60b}', Color::Yellow),
        "yaml" | "yml" => ('\u{e8eb}', Color::Yellow),
        "js" | "mjs" | "cjs" => ('\u{e74e}', Color::Yellow),
        "ts" | "mts" | "cts" => ('\u{e628}', Color::Blue),
        "jsx" | "tsx" => ('\u{e7ba}', Color::Cyan),
        "py" | "pyw" => ('\u{e73c}', Color::Yellow),
        "html" | "htm" => ('\u{e736}', Color::Red),
        "css" | "scss" | "sass" | "less" => ('\u{e749}', Color::Cyan),
        "sh" | "bash" | "zsh" | "fish" | "ksh" => ('\u{f489}', Color::Green),
        "md" | "mdx" | "markdown" => ('\u{e73e}', Color::Blue),
        "txt" | "text" => ('\u{f15c}', Color::Gray),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "ico" | "bmp" => {
            ('\u{f1c5}', Color::Magenta)
        }
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" => ('\u{f03d}', Color::Magenta),
        "mp3" | "flac" | "ogg" | "wav" | "aac" | "opus" => ('\u{f001}', Color::Magenta),
        "zip" | "tar" | "gz" | "xz" | "bz2" | "zst" | "7z" | "rar" => ('\u{f410}', Color::Yellow),
        "pdf" => ('\u{f1c1}', Color::Red),
        "c" | "h" => ('\u{e61e}', Color::Blue),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => ('\u{e61d}', Color::Blue),
        "go" => ('\u{e724}', Color::Cyan),
        "java" | "class" | "jar" => ('\u{e738}', Color::Red),
        "rb" => ('\u{e739}', Color::Red),
        "php" => ('\u{e73d}', Color::Magenta),
        "lua" => ('\u{e620}', Color::Blue),
        "vim" | "nvim" => ('\u{e7c5}', Color::Green),
        "lock" => ('\u{f023}', Color::Yellow),
        "env" | "envrc" => ('\u{f462}', Color::Yellow),
        "xml" => ('\u{e619}', Color::Yellow),
        "sql" => ('\u{e706}', Color::Gray),
        "nix" => ('\u{f313}', Color::Cyan),
        "swift" => ('\u{e755}', Color::Red),
        "kt" | "kts" => ('\u{e634}', Color::Magenta),
        "cs" => ('\u{f81a}', Color::Magenta),
        "ex" | "exs" => ('\u{e62d}', Color::Magenta),
        "hs" | "lhs" => ('\u{e61f}', Color::Magenta),
        "ml" | "mli" => ('\u{e67a}', Color::Yellow),
        "r" | "rmd" => ('\u{f25d}', Color::Blue),
        "tf" | "tfvars" => ('\u{e20f}', Color::Magenta),
        _ => ('\u{f15b}', Color::Gray), // nf-fa-file
    }
}

fn apply_dim_directory_path(col: &mut ratatui::text::Text<'_>, dim_style: ratatui::style::Style) {
    for line in &mut col.lines {
        let full_str: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        if let Some(last_slash_pos) = full_str.rfind(|c| c == '/' || c == '\\') {
            let dir_end_idx = last_slash_pos + 1;
            let mut curr_offset = 0;
            let mut new_spans = Vec::new();
            for span in line.spans.drain(..) {
                let span_len = span.content.len();
                let span_end = curr_offset + span_len;
                if span_end <= dir_end_idx {
                    let mut s = span;
                    s.style = s.style.patch(dim_style);
                    if let Some(fg) = dim_style.fg {
                        s.style.fg = Some(fg);
                    }
                    new_spans.push(s);
                } else if curr_offset >= dir_end_idx {
                    new_spans.push(span);
                } else {
                    let split_at = dir_end_idx - curr_offset;
                    let dir_part = span.content[..split_at].to_string();
                    let base_part = span.content[split_at..].to_string();

                    let mut dir_span = span.clone();
                    dir_span.content = dir_part.into();
                    dir_span.style = dir_span.style.patch(dim_style);
                    if let Some(fg) = dim_style.fg {
                        dir_span.style.fg = Some(fg);
                    }

                    let mut base_span = span;
                    base_span.content = base_part.into();

                    new_spans.push(dir_span);
                    new_spans.push(base_span);
                }
                curr_offset = span_end;
            }
            line.spans = new_spans;
        }
    }
}
