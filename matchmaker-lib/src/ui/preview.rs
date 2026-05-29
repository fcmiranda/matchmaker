use log::error;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::{
    config::{
        BorderSetting, PreviewConfig, PreviewInitialSetting, PreviewSetting, ShowCondition, Side,
    },
    preview::Preview,
    utils::text::wrapped_line_height,
};

#[derive(Debug)]
pub struct PreviewUI {
    pub view: Preview,
    pub config: PreviewConfig,
    layout_idx: usize,
    /// content area
    pub(crate) area: Rect,
    pub scroll: [u16; 2],
    offset: usize,
    target: Option<usize>,
    attained_target: bool,
    #[cfg(feature = "partial")]
    initial: PreviewInitialSetting,

    pub last_count: usize,

    pub jump: (bool, usize), // end, initial

    show: bool,

    pub current_dimension: Option<u16>,

    /// Dynamic title set from the current item's first column; shown in the
    /// preview border when the border is configured.
    title: Option<String>,

    picker: Option<ratatui_image::picker::Picker>,
    pub zoom: f32,
}

impl PreviewUI {
    fn active_border(&self) -> Option<&BorderSetting> {
        if let Some(layout_border) = self.setting().and_then(|s| s.border.as_ref()) {
            if !layout_border.is_empty() {
                return Some(layout_border);
            }
        }

        (!self.config.border.is_empty()).then_some(&self.config.border)
    }

    fn initial(&self) -> &PreviewInitialSetting {
        #[cfg(feature = "partial")]
        {
            &self.initial
        }
        #[cfg(not(feature = "partial"))]
        {
            &self.config.initial
        }
    }

    pub fn new(view: Preview, mut config: PreviewConfig, [ui_width, ui_height]: [u16; 2]) -> Self {
        for x in &mut config.layout {
            if let Some(b) = &mut x.border
                && b.sides.is_none()
                && !b.is_empty()
            {
                b.sides = Some(x.layout.side.opposite())
            }
        }

        let show = match config.show {
            ShowCondition::Free(x) => {
                if let Some(l) = config.layout.first() {
                    match l.layout.side {
                        Side::Bottom | Side::Top => ui_height >= x,
                        _ => ui_width >= x,
                    }
                } else {
                    false
                }
            }
            ShowCondition::Bool(x) => {
                x && if let Some(l) = config.layout.first() {
                    (match l.layout.side {
                        Side::Bottom | Side::Top => ui_height,
                        _ => ui_width,
                    }) > 5 + (l.layout.min.max(0) as u16)
                } else {
                    false
                }
            }
        };

        // enforce invariant of valid index
        if config.layout.is_empty() {
            let mut s = PreviewSetting::default();
            s.layout.max = 0;
            config.layout.push(s);
        }

        let mut picker = None;
        if config.media {
            let p = ratatui_image::picker::Picker::from_query_stdio();
            if let Ok(p) = p {
                picker = Some(p);
            }
        }

        Self {
            view,
            #[cfg(feature = "partial")]
            initial: config.initial.clone(),
            config,
            area: Rect::default(),
            layout_idx: 0,
            scroll: [0, 0],
            offset: 0,
            target: None,
            attained_target: false,
            last_count: 0,
            jump: (false, 0),
            show,
            current_dimension: None,
            title: None,
            picker,
            zoom: 1.0,
        }
    }

    pub fn update_dimensions(&mut self, area: &Rect) {
        let (border_h, border_w) = self
            .active_border()
            .map(|b| (b.height(), b.width()))
            .unwrap_or((0, 0));
        let mut height = area.height;
        height -= border_h.min(height);
        self.area.height = height;

        let mut width = area.width;
        width -= border_w.min(width);
        self.area.width = width;
    }

    pub fn reevaluate_show_condition(&mut self, [ui_width, ui_height]: [u16; 2], hide: bool) {
        match self.config.show {
            ShowCondition::Free(x) => {
                if let Some(setting) = self.setting() {
                    let l = &setting.layout;

                    let show = match l.side {
                        Side::Bottom | Side::Top => ui_height >= x,
                        _ => ui_width >= x,
                    };
                    log::debug!(
                        "Evaluated ShowCondition(Free({x})) against {ui_width}x{ui_height} => {show}"
                    );
                    if !hide && !show {
                        return;
                    }

                    self.show(show);
                };
            }
            ShowCondition::Bool(mut show) => {
                if !hide && !show {
                    return;
                };
                show = show
                    && if let Some(l) = self.config.layout.first() {
                        (match l.layout.side {
                            Side::Bottom | Side::Top => ui_height,
                            _ => ui_width,
                        }) > 5 + (l.layout.min.max(0) as u16)
                    } else {
                        false
                    };
                self.show(show);
            }
        };
    }

    // -------- Setting getters -----------
    /// Set the dynamic item title shown in the preview border.
    pub fn set_title(&mut self, title: Option<String>) {
        self.title = title;
    }

    /// None if not show OR if max = 0 (disabled layour)
    pub fn setting(&self) -> Option<&PreviewSetting> {
        // if let Some(ret) = self.config.layout.get(self.layout_idx)
        if let ret = &self.config.layout[self.layout_idx]
            && ret.layout.max != 0
        {
            Some(&ret)
        } else {
            None
        }
    }

    pub fn setting_mut(&mut self) -> Option<&mut PreviewSetting> {
        if let Some(ret) = self.config.layout.get_mut(self.layout_idx)
            && ret.layout.max != 0
        {
            Some(ret)
        } else {
            None
        }
    }

    pub fn visible(&self) -> bool {
        self.setting().is_some() && self.show
    }

    pub fn command(&self) -> &str {
        self.setting().map(|x| x.command.as_str()).unwrap_or("")
    }

    pub fn border(&self) -> &BorderSetting {
        self.setting()
            .and_then(|s| s.border.as_ref())
            .unwrap_or(&self.config.border)
    }

    pub fn get_initial_command(&self) -> &str {
        let x = self.command();
        if !x.is_empty() {
            return x;
        }

        self.config
            .layout
            .iter()
            .map(|l| l.command.as_str())
            .find(|cmd| !cmd.is_empty())
            .unwrap_or("")
    }

    // -------- Layout -----------
    pub fn cycle_layout(&mut self) {
        let len = self.config.layout.len();

        for _ in 0..len {
            self.layout_idx = (self.layout_idx + 1) % len;

            if self.config.layout[self.layout_idx].layout.max > 0 {
                self.reinit();
                return;
            }
        }
    }
    pub fn set_layout(&mut self, idx: u8) -> bool {
        let idx = idx as usize;
        if idx < self.config.layout.len() {
            let changed = self.layout_idx != idx;
            self.layout_idx = idx;
            self.reinit();
            changed
        } else {
            error!("Layout idx {idx} out of bounds, ignoring.");
            false
        }
    }
    pub fn reinit(&mut self) {
        #[cfg(feature = "partial")]
        {
            use matchmaker_partial::Apply;
            if let Some(s) = self.setting() {
                let mut new = self.config.initial.clone();
                new.apply(s.initial.clone());
                log::trace!("Applied: {:?} -> {:?}", s.initial, new);
                self.initial = new;
            }
        }
        self.current_dimension = None;
    }

    // ----- config && getters ---------

    pub fn show(&mut self, show: bool) -> bool {
        log::trace!("toggle preview with: {show}");
        let changed = self.show != show;
        self.show = show;
        changed
    }

    pub fn toggle_show(&mut self) {
        self.show = !self.show;
    }

    pub fn wrap(&mut self, wrap: bool) {
        self.config.wrap = wrap;
    }
    pub fn is_wrap(&self) -> bool {
        self.config.wrap
    }
    pub fn offset(&self) -> usize {
        self.initial().header_lines + self.offset
    }
    pub fn target_line(&self) -> Option<usize> {
        self.target
    }

    // ----- actions --------
    pub fn up(&mut self, n: u16) {
        let total_lines = self.view.len();
        let n = n as usize;

        if self.offset >= n {
            self.offset -= n;
        } else if self.config.scroll_wrap {
            self.offset = total_lines.saturating_sub(n - self.offset);
        } else {
            self.offset = 0;
        }
    }
    pub fn down(&mut self, n: u16) {
        let total_lines = self.view.len();
        let n = n as usize;

        if self.offset + n > total_lines {
            if self.config.scroll_wrap {
                self.offset = 0;
            } else {
                self.offset = total_lines;
            }
        } else {
            self.offset += n;
        }
    }

    pub fn scroll(&mut self, horizontal: bool, val: i8) {
        let a = &mut self.scroll[horizontal as usize];

        if val == 0 {
            *a = 0;
        } else {
            let new = (*a as i8 + val).clamp(0, u16::MAX as i8);
            *a = new as u16;
        }
    }

    pub fn set_target(&mut self, target: Option<isize>) {
        if self.initial().tail {
            return;
        }

        let results = self.view.results().lines;
        let line_count = results.len();

        let Some(mut target) = target else {
            self.target = None;
            self.offset = 0;
            return;
        };

        target += self.initial().offset;

        self.target = Some(if target < 0 {
            line_count.saturating_sub(target.unsigned_abs())
        } else {
            target as usize
        });

        let index = self.target.unwrap();

        self.offset = if index >= results.len() {
            self.attained_target = false;
            results.len().saturating_sub(self.area.height as usize / 2)
        } else {
            self.attained_target = true;
            self.target_to_offset(index, &results)
        };

        log::trace!("Preview initial offset: {}, index: {}", self.offset, index);
    }

    pub fn jump(&mut self) {
        if self.initial().tail {
            if self.offset > 0 {
                // go to end
                self.jump = (false, self.offset);
                self.reset_scroll();
            } else {
                if !self.jump.0 {
                    // go to start

                    self.attained_target = true;
                    self.offset = 0;
                    self.jump.0 = true
                } else {
                    // go to saved
                    self.offset = self.jump.1;
                    self.attained_target = true;
                    self.jump = (false, 0)
                }
            }
        } else {
            match self.jump {
                (false, 0) => {
                    self.jump = (true, self.offset);
                    self.scroll_end();
                }
                (true, x) if x != 0 => {
                    self.jump.0 = false;
                    self.reset_scroll();
                }
                _ => {
                    self.offset = self.jump.1;
                    self.jump = (false, 0)
                }
            }
        }
    }
    pub fn reset_scroll(&mut self) {
        self.offset = 0;
        self.attained_target = false;
    }
    pub fn scroll_end(&mut self) {
        let results = self.view.results();
        let rl = results.lines.len();
        let height = self.area.height as usize;

        let header_count = self.initial().header_lines.min(height);
        let remaining_lines = rl.saturating_sub(header_count);

        self.offset = remaining_lines.saturating_sub(height);
    }

    fn target_to_offset(&self, mut target: usize, results: &Vec<Line>) -> usize {
        // decrement the index to put the target lower on the page.
        // The resulting height up to the top of target should >= p% of height.
        let mut lines_above =
            self.config
                .initial
                .percentage
                .complement()
                .compute_clamped(self.area.height, 0, 0);

        // shoddy approximation to how Paragraph wraps lines
        while target > 0 && lines_above > 0 {
            let prev = results
                .get(target)
                .map(|x| wrapped_line_height(x, self.area.width))
                .unwrap_or(1);
            if prev > lines_above {
                break;
            } else {
                target -= 1;
                lines_above -= prev;
            }
        }

        target
    }
    // --------------------------

    pub fn drag_width(&self) -> u16 {
        self.config.drag_width.unwrap_or_else(|| {
            let side = self
                .setting()
                .map(|s| &s.layout.side)
                .unwrap_or(&Side::Right);
            match side {
                Side::Left | Side::Right => self.active_border().map(|b| b.width()).unwrap_or(0),
                Side::Top | Side::Bottom => self.active_border().map(|b| b.height()).unwrap_or(0),
            }
        })
    }

    pub fn split(&self, area: Rect) -> [Rect; 3] {
        let Some(setting) = self.setting() else {
            return [Rect::default(), area, Rect::default()];
        };

        setting.layout.split(area, self.current_dimension)
    }

    pub fn expand(&mut self, n: u16) {
        if n == 0 {
            self.current_dimension = None;
            return;
        }
        let current = self.current_size();
        self.current_dimension = Some(current.saturating_add(n));
    }

    pub fn shrink(&mut self, n: u16) {
        if n == 0 {
            self.current_dimension = None;
            return;
        }

        let current = self.current_size();
        self.current_dimension = Some(current.saturating_sub(n));
    }

    fn current_size(&self) -> u16 {
        if let Some(dim) = self.current_dimension {
            dim
        } else {
            let setting = self.setting();
            let side = setting.map(|s| &s.layout.side).unwrap_or(&Side::Right);
            match side {
                Side::Left | Side::Right => {
                    self.area.width + self.active_border().map(|b| b.width()).unwrap_or(0)
                }
                Side::Top | Side::Bottom => {
                    self.area.height + self.active_border().map(|b| b.height()).unwrap_or(0)
                }
            }
        }
    }

    pub fn make_image_preview<'a>(&'a mut self, area: Rect) -> Option<ratatui_image::protocol::Protocol> {
        if let Ok(guard) = self.view.image.lock() {
            if let Some(img) = &*guard {
                if let Some(picker) = &mut self.picker {
                    let mut display_img = img.clone();
                    if self.zoom != 1.0 {
                        let center_x = img.width() / 2;
                        let center_y = img.height() / 2;
                        let crop_w = (img.width() as f32 / self.zoom) as u32;
                        let crop_h = (img.height() as f32 / self.zoom) as u32;
                        let x = center_x.saturating_sub(crop_w / 2);
                        let y = center_y.saturating_sub(crop_h / 2);
                        display_img = img.crop_imm(x, y, crop_w, crop_h);
                    }
                    let size = ratatui::layout::Size { width: area.width, height: area.height };
                    if let Ok(protocol) = picker.new_protocol(display_img, size, ratatui_image::Resize::Fit(None)) {
                        return Some(protocol);
                    }
                }
            }
        }
        None
    }

    pub fn make_preview(&mut self) -> Paragraph<'_> {
        let results = self.view.results();
        let rl = results.lines.len();
        let height = self.area.height as usize;
        let mut offset = self.offset;

        // this only triggers on preview change but not guaranteed on every preview change -- attaching it to the event handler is worse
        if rl < self.last_count {
            self.offset = 0;
            self.attained_target = false;
            self.jump = (false, 0)
        }
        self.last_count = rl;

        if self.initial().tail && !self.attained_target {
            let header_count = self.initial().header_lines.min(height);
            let remaining_lines = rl.saturating_sub(header_count);
            let remaining_space = height.saturating_sub(header_count);

            // get current offset
            offset = remaining_lines.saturating_sub(remaining_space);
            // apply initial offset
            if self.initial().offset < 0 {
                offset = offset.saturating_sub((self.initial().offset).unsigned_abs());
            }

            // stop scrolling
            if self.offset != 0 {
                if self.offset > offset || self.offset + offset > rl {
                    self.offset = self.offset.saturating_sub(rl.saturating_sub(offset));
                } else {
                    self.offset += offset;
                }
                self.attained_target = true;
            }
            // log::trace!("{} {} {}", offset, self.offset, self.attained_target);
        } else if let Some(target) = self.target
            && !self.attained_target
            && target < rl
        {
            self.offset = self.target_to_offset(target, &results.lines);
            self.attained_target = true;
        };

        let mut results = results.into_iter();

        if height == 0 {
            return Paragraph::new(Vec::new());
        }

        let mut lines = Vec::with_capacity(height);

        for _ in 0..self.initial().header_lines.min(height) {
            if let Some(line) = results.next() {
                lines.push(line);
            } else {
                break;
            };
        }

        let mut results = results.skip(offset);

        for _ in self.initial().header_lines..height {
            if let Some(line) = results.next() {
                lines.push(line);
            }
        }

        let configured_title = self.setting().and_then(|s| s.title.as_deref());
        let dynamic = self.title.as_deref().unwrap_or_default();
        let title_text = match configured_title {
            None => Some(dynamic.to_string()),
            Some("") => None,
            Some("{item}") => Some(dynamic.to_string()),
            Some(t) if t.contains("{item}") => Some(t.replace("{item}", dynamic)),
            Some("$currentItemName") => Some(dynamic.to_string()),
            Some(t) if t.contains("$currentItemName") => {
                Some(t.replace("$currentItemName", dynamic))
            }
            Some(t) => Some(t.to_string()),
        };

        if self.active_border().is_none() {
            if let Some(title) = &title_text {
                let fg = if self.config.border.title_fg == ratatui::style::Color::Reset {
                    self.config.border.color
                } else {
                    self.config.border.title_fg
                };
                let title_line = Line::from(Span::styled(
                    title.clone(),
                    Style::default()
                        .fg(fg)
                        .add_modifier(self.config.border.title_modifier),
                ));
                lines.insert(0, title_line);
                lines.truncate(height);
            }
        }

        let mut preview = Paragraph::new(lines);
        if let Some(border) = self.active_border() {
            let mut block = border.as_block();
            if let Some(title) = title_text.clone() {
                let fg = if border.title_fg != ratatui::style::Color::Reset {
                    border.title_fg
                } else if self.config.border.title_fg != ratatui::style::Color::Reset {
                    self.config.border.title_fg
                } else if border.color != ratatui::style::Color::Reset {
                    border.color
                } else {
                    self.config.border.color
                };
                block = block.title(Span::styled(
                    title,
                    Style::default().fg(fg).add_modifier(border.title_modifier),
                ));
            }
            preview = preview.block(block);
        }
        if self.config.wrap {
            preview = preview
                .wrap(Wrap { trim: false })
                .scroll(self.scroll.into());
        }
        preview
    }
}
