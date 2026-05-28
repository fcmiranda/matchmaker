mod dynamic;
mod state;

use cba::bait::ResultExt;
use crossterm::event::{MouseButton, MouseEventKind};
pub use dynamic::*;
pub use state::*;
// ------------------------------

use std::io::Write;

use log::{debug, info, warn};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use tokio::sync::mpsc;

#[cfg(feature = "bracketed-paste")]
use crate::PasteHandler;
use crate::action::{Action, ActionExt, NullActionExt};
use crate::config::{CursorSetting, ExitConfig, RowConnectionStyle};
use crate::event::{BindSender, EventSender};
use crate::message::{BindDirective, Event, Interrupt, RenderCommand};
use crate::tui::Tui;
use crate::ui::{DisplayUI, OverlayUI, PickerUI, PreviewUI, QueryUI, ResultsUI, UI};
use crate::{ActionAliaser, ActionExtHandler, Initializer, MatchError, SSS, Selection};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph};

fn action_from_null<A: ActionExt>(action: Action<NullActionExt>) -> Option<Action<A>> {
    Some(match action {
        Action::Select => Action::Select,
        Action::Deselect => Action::Deselect,
        Action::Toggle => Action::Toggle,
        Action::CycleAll => Action::CycleAll,
        Action::ClearSelections => Action::ClearSelections,
        Action::Accept => Action::Accept,
        Action::Quit(x) => Action::Quit(x),
        Action::ToggleWrap => Action::ToggleWrap,
        Action::ToggleActionBox => Action::ToggleActionBox,
        Action::ToggleFocus => Action::ToggleFocus,
        Action::Up(x) => Action::Up(x),
        Action::Down(x) => Action::Down(x),
        Action::Pos(x) => Action::Pos(x),
        Action::HalfPageDown => Action::HalfPageDown,
        Action::HalfPageUp => Action::HalfPageUp,
        Action::HScroll(x) => Action::HScroll(x),
        Action::VScroll(x) => Action::VScroll(x),
        Action::CyclePreview => Action::CyclePreview,
        Action::Preview(x) => Action::Preview(x),
        Action::Help(x) => Action::Help(x),
        Action::SetPreview(x) => Action::SetPreview(x),
        Action::SwitchPreview(x) => Action::SwitchPreview(x),
        Action::TogglePreviewWrap => Action::TogglePreviewWrap,
        Action::PreviewUp(x) => Action::PreviewUp(x),
        Action::PreviewDown(x) => Action::PreviewDown(x),
        Action::ExpandPreview(x) => Action::ExpandPreview(x),
        Action::ShrinkPreview(x) => Action::ShrinkPreview(x),
        Action::PreviewHalfPageUp => Action::PreviewHalfPageUp,
        Action::PreviewHalfPageDown => Action::PreviewHalfPageDown,
        Action::PreviewHScroll(x) => Action::PreviewHScroll(x),
        Action::PreviewScroll(x) => Action::PreviewScroll(x),
        Action::PreviewJump => Action::PreviewJump,
        Action::NextColumn => Action::NextColumn,
        Action::PrevColumn => Action::PrevColumn,
        Action::SwitchColumn(x) => Action::SwitchColumn(x),
        Action::ToggleColumn(x) => Action::ToggleColumn(x),
        Action::ShowColumn(x) => Action::ShowColumn(x),
        Action::Execute(x) => Action::Execute(x),
        Action::ExecuteAsync(x) => Action::ExecuteAsync(x),
        Action::ExecuteThen(x) => Action::ExecuteThen(x),
        Action::ExecuteSilent(x) => Action::ExecuteSilent(x),
        Action::Become(x) => Action::Become(x),
        Action::BecomeSilent(x) => Action::BecomeSilent(x),
        Action::Reload(x) => Action::Reload(x),
        Action::ChDir(x) => Action::ChDir(x),
        Action::Print(x) => Action::Print(x),
        Action::PrintKey => Action::PrintKey,
        Action::Store(x) => Action::Store(x),
        Action::ForwardChar => Action::ForwardChar,
        Action::BackwardChar => Action::BackwardChar,
        Action::ForwardWord => Action::ForwardWord,
        Action::BackwardWord => Action::BackwardWord,
        Action::DeleteChar => Action::DeleteChar,
        Action::DeleteWord => Action::DeleteWord,
        Action::DeleteLineStart => Action::DeleteLineStart,
        Action::DeleteLineEnd => Action::DeleteLineEnd,
        Action::Cancel => Action::Cancel,
        Action::SetQuery(x) => Action::SetQuery(x),
        Action::QueryPos(x) => Action::QueryPos(x),
        Action::Char(x) => Action::Char(x),
        Action::Redraw => Action::Redraw,
        Action::Custom(_) => return None,
        Action::Overlay(x) => Action::Overlay(x),
        Action::Semantic(x) => Action::Semantic(x),
        Action::Trace(x) => Action::Trace(x),
    })
}

/// Pre-process `buffer` for navigation mode: simulate `ToggleFocus` events encountered in the
/// batch and expand `Action::Char` events into `nav_binds` actions while focus is on results.
fn apply_focus_binds<A: ActionExt>(
    buffer: &mut Vec<RenderCommand<A>>,
    initial_focus: Focus,
    focus_binds: &std::collections::HashMap<String, crate::action::Actions<NullActionExt>>,
    overlay_active: bool,
) {
    if overlay_active {
        return;
    }

    let mut out = Vec::with_capacity(buffer.len());
    let mut sim_focus = initial_focus;

    for cmd in buffer.drain(..) {
        match cmd {
            RenderCommand::Action(Action::ToggleFocus) => {
                sim_focus = match sim_focus {
                    Focus::Input => Focus::Results,
                    Focus::Results => Focus::Input,
                };
                out.push(RenderCommand::Action(Action::ToggleFocus));
            }
            RenderCommand::Action(Action::Char(c)) if sim_focus == Focus::Results => {
                let key = c.to_string();
                if let Some(actions) = focus_binds.get(&key) {
                    for action in actions.iter().cloned() {
                        if let Some(action) = action_from_null::<A>(action) {
                            out.push(RenderCommand::Action(action));
                        }
                    }
                }
            }
            other => out.push(other),
        }
    }

    *buffer = out;
}

fn apply_aliases<T: SSS, S: Selection, A: ActionExt>(
    buffer: &mut Vec<RenderCommand<A>>,
    aliaser: &mut ActionAliaser<T, S, A>,
    dispatcher: &mut MMState<'_, '_, T, S>,
) {
    let mut out = Vec::new();

    for cmd in buffer.drain(..) {
        match cmd {
            RenderCommand::Action(a) => out.extend(
                aliaser(a, dispatcher)
                    .into_iter()
                    .map(RenderCommand::Action),
            ),
            other => out.push(other),
        }
    }

    *buffer = out;
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn render_loop<'a, W: Write, T: SSS, S: Selection, A: ActionExt>(
    mut ui: UI,
    mut picker_ui: PickerUI<'a, T, S>,
    mut footer_ui: DisplayUI,
    mut preview_ui: Option<PreviewUI>,
    mut tui: Tui<W>,

    mut overlay_ui: Option<OverlayUI<A>>,
    exit_config: ExitConfig,

    mut render_rx: mpsc::UnboundedReceiver<RenderCommand<A>>,
    controller_tx: EventSender,
    bind_tx: BindSender<A>,

    mut dynamic_handlers: DynamicHandlers<T, S>,
    mut ext_handler: Option<ActionExtHandler<T, S, A>>,
    mut ext_aliaser: Option<ActionAliaser<T, S, A>>,
    initializer: Option<Initializer<T, S>>,
    #[cfg(feature = "bracketed-paste")] //
    mut paste_handler: Option<PasteHandler<T, S>>,
) -> Result<Vec<S>, MatchError> {
    let mut state = State::new();

    if let Some(handler) = initializer {
        handler(&mut state.dispatcher(
            &mut ui,
            &mut picker_ui,
            &mut footer_ui,
            &mut preview_ui,
            &controller_tx,
        ));
    }

    let mut click = Click::None;
    // Tracks the last known mouse position for gap hover highlighting.
    let mut mouse_hover: Option<Position> = None;

    // place the initial command in the state where the preview listener can access
    if let Some(ref p) = preview_ui {
        state.update_preview_payload(p.get_initial_command());
    }

    let mut buffer = Vec::with_capacity(256);

    while render_rx.recv_many(&mut buffer, 256).await > 0 {
        if state.iterations == 0 {
            log::debug!("Render loop started");
        }
        let (mut did_pause, mut did_reload, mut did_exit, mut did_resize, mut did_cursor_wrap) =
            (false, false, None, false, false);

        if let Some(aliaser) = &mut ext_aliaser {
            apply_aliases(
                &mut buffer,
                aliaser,
                &mut state.dispatcher(
                    &mut ui,
                    &mut picker_ui,
                    &mut footer_ui,
                    &mut preview_ui,
                    &controller_tx,
                ),
            )
        };

        if ui.config.nav_mode {
            apply_focus_binds(
                &mut buffer,
                state.focus,
                &ui.config.nav_binds,
                picker_ui.action_visible
                    || overlay_ui.as_ref().map_or(false, |o| o.index().is_some()),
            );

            if let Some(aliaser) = &mut ext_aliaser {
                apply_aliases(
                    &mut buffer,
                    aliaser,
                    &mut state.dispatcher(
                        &mut ui,
                        &mut picker_ui,
                        &mut footer_ui,
                        &mut preview_ui,
                        &controller_tx,
                    ),
                )
            };
        }

        if state.should_quit {
            log::debug!("Exiting due to should_quit");
            return if picker_ui.selector.is_disabled()
                && let Some((_, item)) = get_current(&picker_ui)
            {
                Ok(vec![item])
            } else {
                Ok(picker_ui.selector.output().collect())
            };
        } else if state.should_quit_nomatch {
            log::debug!("Exiting due to should_quit_nomatch");
            return Err(MatchError::NoMatch);
        }

        let mut events = buffer.drain(..);
        while let Some(event) = events.next() {
            state.clear_interrupt();

            if !matches!(event, RenderCommand::Tick) {
                info!("Received {event:?}");
            } else {
                // log::trace!("Recieved {event:?}");
            }

            match event {
                #[cfg(feature = "bracketed-paste")]
                RenderCommand::Paste(content) => {
                    if let Some(handler) = &mut paste_handler {
                        let content = {
                            handler(
                                content,
                                &state.dispatcher(
                                    &mut ui,
                                    &mut picker_ui,
                                    &mut footer_ui,
                                    &mut preview_ui,
                                    &controller_tx,
                                ),
                            )
                        };
                        if !content.is_empty() {
                            if let Some(x) = overlay_ui.as_mut()
                                && x.index().is_some()
                            {
                                for c in content.chars() {
                                    x.handle_input(c);
                                }
                            } else {
                                picker_ui.query.push_str(&content);
                            }
                        }
                    }
                }
                RenderCommand::Resize(area) => {
                    tui.resize(area);
                    ui.update_dimensions(area);
                }
                RenderCommand::Refresh => {
                    picker_ui.header.init();
                    footer_ui.init();
                    picker_ui.query.set_prompt(None);
                    picker_ui.results.set_status_line(None);
                    tui.redraw();
                }
                RenderCommand::HeaderTable(columns) => {
                    picker_ui.header.header_table(columns);
                }
                RenderCommand::Mouse(mouse) => {
                    use crate::config::Side;
                    // we could also impl this in the aliasing step
                    let pos = Position::from((mouse.column, mouse.row));
                    let layout = state.layout;

                    match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            if let Some(p) = preview_ui.as_mut()
                                && p.visible()
                            {
                                let gap_rect = layout.gap;
                                let is_in_drag_area = if !gap_rect.is_empty() {
                                    // Dedicated gap area: click anywhere in the gap to start drag.
                                    gap_rect.contains(pos)
                                } else {
                                    // Fallback: use the legacy drag_width border-edge approach.
                                    let drag_width = p.drag_width();
                                    if drag_width > 0 {
                                        let side = p
                                            .setting()
                                            .map(|s| &s.layout.side)
                                            .unwrap_or(&Side::Right);
                                        match side {
                                            Side::Right => {
                                                let drag_area = Rect {
                                                    x: layout.preview.x,
                                                    y: layout.preview.y,
                                                    width: drag_width,
                                                    height: layout.preview.height,
                                                };
                                                drag_area.contains(pos)
                                            }
                                            Side::Left => {
                                                let drag_area = Rect {
                                                    x: layout.preview.x
                                                        + layout
                                                            .preview
                                                            .width
                                                            .saturating_sub(drag_width),
                                                    y: layout.preview.y,
                                                    width: drag_width,
                                                    height: layout.preview.height,
                                                };
                                                drag_area.contains(pos)
                                            }
                                            Side::Bottom => {
                                                let drag_area = Rect {
                                                    x: layout.preview.x,
                                                    y: layout.preview.y,
                                                    width: layout.preview.width,
                                                    height: drag_width,
                                                };
                                                drag_area.contains(pos)
                                            }
                                            Side::Top => {
                                                let drag_area = Rect {
                                                    x: layout.preview.x,
                                                    y: layout.preview.y
                                                        + layout
                                                            .preview
                                                            .height
                                                            .saturating_sub(drag_width),
                                                    width: layout.preview.width,
                                                    height: drag_width,
                                                };
                                                drag_area.contains(pos)
                                            }
                                        }
                                    } else {
                                        false
                                    }
                                };

                                if is_in_drag_area {
                                    state.dragging = Some(pos);
                                    continue;
                                }
                            }

                            if layout.results.contains(pos) {
                                let y = mouse.row - layout.results.top();
                                debug!("Results clicked at: {y}");
                                click = Click::ResultPos(y);
                            } else if layout.input.contains(pos) {
                                // The X offset of the start of the visible text relative to the terminal
                                let text_start_x = layout.input.x + picker_ui.query.left();

                                if pos.x >= text_start_x {
                                    let visual_offset = pos.x - text_start_x;
                                    picker_ui.query.set_at_visual_offset(visual_offset);
                                } else {
                                    picker_ui.query.set(None, 0);
                                }
                            } else if layout.status.contains(pos) {
                                let x = pos.x.saturating_sub(layout.status.x);
                                debug!("Status clicked at x: {x}");
                                if let Some(action) = find_interaction(
                                    &picker_ui.results.status_config.interactions,
                                    x,
                                ) {
                                    click = Click::Semantic(action);
                                }
                            } else if layout.header.contains(pos) {
                                let rel_x = pos.x.saturating_sub(layout.header.x);
                                let rel_y = pos.y.saturating_sub(layout.header.y);
                                debug!("Header clicked at x: {rel_x}, y: {rel_y}");

                                if let Some(setting) =
                                    picker_ui.header.config.interactions.get(rel_y as usize)
                                    && let Some(action) = find_interaction(setting, rel_x)
                                {
                                    click = Click::Semantic(action);
                                }
                            } else if layout.footer.contains(pos) {
                                let rel_x = pos.x.saturating_sub(layout.footer.x);
                                let rel_y = pos.y.saturating_sub(layout.footer.y);
                                debug!("Footer clicked at x: {rel_x}, y: {rel_y}");

                                if let Some(setting) =
                                    footer_ui.config.interactions.get(rel_y as usize)
                                    && let Some(action) = find_interaction(setting, rel_x)
                                {
                                    click = Click::Semantic(action);
                                }
                            }
                        }
                        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                            if layout.preview.contains(pos) {
                                if let Some(p) = preview_ui.as_mut() {
                                    if matches!(mouse.kind, MouseEventKind::ScrollDown) {
                                        p.down(1)
                                    } else {
                                        p.up(1)
                                    }
                                }
                            } else {
                                let next = matches!(mouse.kind, MouseEventKind::ScrollDown)
                                    ^ picker_ui.results.reverse();
                                did_cursor_wrap = if next {
                                    picker_ui.results.cursor_next()
                                } else {
                                    picker_ui.results.cursor_prev()
                                };
                            }
                        }
                        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
                            let left = matches!(mouse.kind, MouseEventKind::ScrollLeft);
                            if layout.preview.contains(pos) {
                                if let Some(p) = preview_ui.as_mut() {
                                    p.scroll(true, if left { -1 } else { 1 })
                                }
                            } else {
                                if !left
                                    || picker_ui.results.hscroll > 0
                                    || !picker_ui.query.input.is_empty()
                                {
                                    picker_ui
                                        .results
                                        .current_scroll(if left { -1 } else { 1 }, true);
                                }
                            }
                        }
                        MouseEventKind::Drag(MouseButton::Left) => {
                            if let Some(start_pos) = state.dragging {
                                if let Some(p) = preview_ui.as_mut() {
                                    let side =
                                        p.setting().map(|s| &s.layout.side).unwrap_or(&Side::Right);
                                    match side {
                                        Side::Right => {
                                            if pos.x < start_pos.x {
                                                p.expand(start_pos.x - pos.x);
                                            } else if pos.x > start_pos.x {
                                                p.shrink(pos.x - start_pos.x);
                                            }
                                        }
                                        Side::Left => {
                                            if pos.x > start_pos.x {
                                                p.expand(pos.x - start_pos.x);
                                            } else if pos.x < start_pos.x {
                                                p.shrink(start_pos.x - pos.x);
                                            }
                                        }
                                        Side::Bottom => {
                                            if pos.y < start_pos.y {
                                                p.expand(start_pos.y - pos.y);
                                            } else if pos.y > start_pos.y {
                                                p.shrink(pos.y - start_pos.y);
                                            }
                                        }
                                        Side::Top => {
                                            if pos.y > start_pos.y {
                                                p.expand(pos.y - start_pos.y);
                                            } else if pos.y < start_pos.y {
                                                p.shrink(start_pos.y - pos.y);
                                            }
                                        }
                                    }
                                    state.dragging = Some(pos);
                                }
                            }
                        }
                        MouseEventKind::Up(MouseButton::Left) => {
                            state.dragging = None;
                        }
                        MouseEventKind::Moved => {
                            mouse_hover = Some(pos);
                        }
                        _ => {}
                    }
                }
                RenderCommand::NoMatch => {
                    return Err(MatchError::NoMatch);
                }
                RenderCommand::Empty => {
                    return Ok(vec![]);
                }
                RenderCommand::Action(action) => {
                    if let Some(x) = overlay_ui.as_mut() {
                        if match action {
                            Action::Char(c) => x.handle_input(c),
                            _ => x.handle_action(&action),
                        } {
                            continue;
                        }
                    }
                    let PickerUI {
                        query,
                        action: action_input,
                        action_visible,
                        results,
                        worker,
                        selector: selections,
                        ..
                    } = &mut picker_ui;
                    match action {
                        Action::Select => {
                            if let Some(item) = worker.get_nth(results.index()) {
                                selections.sel(item);
                            }
                        }
                        Action::Deselect => {
                            if let Some(item) = worker.get_nth(results.index()) {
                                selections.desel(item);
                            }
                        }
                        Action::Toggle => {
                            if let Some(item) = worker.get_nth(results.index()) {
                                selections.toggle(item);
                                // Advance cursor so repeated presses select consecutive items.
                                results.cursor_next();
                            }
                        }
                        Action::CycleAll => {
                            selections.cycle_all_bg(worker.raw_results());
                        }
                        Action::ClearSelections => {
                            selections.clear();
                        }
                        Action::Accept => {
                            let ret = if selections.is_empty() {
                                if let Some(item) = get_current(&picker_ui) {
                                    vec![item.1]
                                } else if exit_config.allow_empty {
                                    vec![]
                                } else {
                                    continue;
                                }
                            } else {
                                selections.output().collect::<Vec<S>>()
                            };
                            return Ok(ret);
                        }
                        Action::Quit(code) => {
                            if picker_ui.action_visible {
                                picker_ui.action_visible = false;
                                picker_ui.action.cancel();
                            } else {
                                return Err(MatchError::Abort(code));
                            }
                        }

                        // Results
                        Action::ToggleWrap => {
                            results.wrap(!results.is_wrap());
                        }
                        Action::ToggleActionBox => {
                            picker_ui.action_visible = !picker_ui.action_visible;
                        }
                        Action::Up(x) | Action::Down(x) => {
                            let next = matches!(action, Action::Down(_)) ^ results.reverse();
                            for _ in 0..x.into() {
                                did_cursor_wrap = if next {
                                    results.cursor_next()
                                } else {
                                    results.cursor_prev()
                                };
                            }
                        }
                        Action::Pos(pos) => {
                            let pos = if pos >= 0 {
                                pos as u32
                            } else {
                                results.status.matched_count.saturating_sub((-pos) as u32)
                            };
                            results.cursor_jump(pos);
                        }
                        Action::QueryPos(pos) => {
                            let pos = if pos >= 0 {
                                pos as u16
                            } else {
                                (query.len() as u16).saturating_sub((-pos) as u16)
                            };
                            query.set(None, pos);
                        }
                        Action::HScroll(n) | Action::VScroll(n) => {
                            if let Some(p) = &mut preview_ui
                                && !p.config.wrap
                                && false
                            // track mouse location?
                            {
                                p.scroll(true, n);
                            } else if !matches!(action, Action::HScroll(_))
                                || n >= 0
                                || results.hscroll > 0
                                || !query.input.is_empty()
                            {
                                results.current_scroll(n, matches!(action, Action::HScroll(_)));
                            }
                        }
                        Action::HalfPageDown | Action::HalfPageUp => {
                            let x = (results.height() + 1) / 2;
                            let next = matches!(action, Action::HalfPageDown) ^ results.reverse();
                            for _ in 0..x.into() {
                                did_cursor_wrap = if next {
                                    results.cursor_next()
                                } else {
                                    results.cursor_prev()
                                };
                            }
                        }

                        // Preview Navigation
                        Action::PreviewUp(n) => {
                            if let Some(p) = preview_ui.as_mut() {
                                p.up(n)
                            }
                        }
                        Action::PreviewDown(n) => {
                            if let Some(p) = preview_ui.as_mut() {
                                p.down(n)
                            }
                        }
                        Action::ExpandPreview(n) => {
                            if let Some(p) = preview_ui.as_mut() {
                                p.expand(n)
                            }
                        }
                        Action::ShrinkPreview(n) => {
                            if let Some(p) = preview_ui.as_mut() {
                                p.shrink(n)
                            }
                        }
                        Action::PreviewHalfPageUp | Action::PreviewHalfPageDown => {
                            if let Some(p) = preview_ui.as_mut() {
                                let n = (p.area.height + 1) / 2;

                                if matches!(action, Action::PreviewHalfPageUp) {
                                    p.up(n)
                                } else {
                                    p.down(n)
                                }
                            }
                        }

                        Action::PreviewHScroll(x) | Action::PreviewScroll(x) => {
                            if let Some(p) = preview_ui.as_mut() {
                                p.scroll(matches!(action, Action::PreviewHScroll(_)), x);
                            }
                        }
                        Action::PreviewJump => {
                            if let Some(p) = preview_ui.as_mut() {
                                p.jump()
                            }
                        }

                        // Preview
                        Action::CyclePreview => {
                            if let Some(p) = preview_ui.as_mut() {
                                p.cycle_layout();
                                if !p.command().is_empty() {
                                    state.update_preview_payload(p.command());
                                }
                            }
                        }

                        Action::Preview(context) => {
                            if let Some(p) = preview_ui.as_mut() {
                                if !state.update_preview_payload(context.as_str()) {
                                    p.toggle_show()
                                } else {
                                    p.show(true);
                                }
                            };
                        }
                        Action::Help(context) => {
                            if let Some(p) = preview_ui.as_mut() {
                                // empty payload signifies help
                                if !state.update_preview_set(Err(context.into())) {
                                    state.update_preview_unset()
                                } else {
                                    p.show(true);
                                }
                            };
                        }
                        Action::SetPreview(idx) => {
                            if let Some(p) = preview_ui.as_mut() {
                                if let Some(idx) = idx {
                                    p.set_layout(idx);
                                } else {
                                    state.update_preview_payload(p.command());
                                }
                            }
                        }
                        Action::SwitchPreview(idx) => {
                            if let Some(p) = preview_ui.as_mut() {
                                if let Some(idx) = idx {
                                    if !p.set_layout(idx)
                                        && !state.update_preview_payload(p.command())
                                    {
                                        p.toggle_show();
                                    }
                                } else {
                                    p.toggle_show()
                                }
                            }
                        }
                        Action::TogglePreviewWrap => {
                            if let Some(p) = preview_ui.as_mut() {
                                p.wrap(!p.is_wrap());
                            }
                        }

                        // Programmable
                        Action::Execute(payload) => {
                            state.set_interrupt(Interrupt::Execute, payload);
                        }
                        Action::CopyAsync(ref payload) => {
                            state.set_interrupt(Interrupt::ExecuteAsync, payload.clone());
                            state.discriminant_payload = Some(if tui.config.osc52 { 1 } else { 0 });
                        }
                        Action::Copy(ref payload) => {
                            state.set_interrupt(Interrupt::ExecuteSilent, payload.clone());
                            state.discriminant_payload = Some(if tui.config.osc52 { 3 } else { 2 });
                        }
                        Action::ExecuteAsync(ref payload) | Action::ExecuteThen(ref payload) => {
                            let is_async = matches!(action, Action::ExecuteAsync(_));
                            let payload = payload.clone();

                            let mut remainder = crate::action::Actions::default();
                            for cmd in events.by_ref() {
                                if let RenderCommand::Action(a) = cmd {
                                    remainder.push(a);
                                }
                            }

                            if let Some(id) = state.stash_actions(remainder, bind_tx.clone()) {
                                state.set_interrupt(Interrupt::ExecuteAsync, payload);
                                state.discriminant_payload =
                                    Some(2 * id + (if is_async { 1 } else { 0 }));
                            } else {
                                log::error!("No free slots left: remaining actions dropped");
                            }
                        }
                        Action::ExecuteSilent(payload) => {
                            state.set_interrupt(Interrupt::ExecuteSilent, payload);
                        }
                        Action::Store(payload) => {
                            state.envs.set("MM_STORE", payload);
                        }
                        Action::Become(payload) => {
                            state.set_interrupt(Interrupt::Become, payload);
                        }
                        Action::BecomeSilent(payload) => {
                            state.set_interrupt(Interrupt::BecomeSilent, payload);
                        }
                        Action::Reload(payload) => {
                            state.set_interrupt(Interrupt::Reload, payload);
                        }
                        Action::ChDir(payload) => {
                            state.set_interrupt(Interrupt::ChDir, payload);
                        }
                        Action::Print(payload) => {
                            state.set_interrupt(Interrupt::Print, payload);
                        }

                        // Columns
                        Action::SwitchColumn(col_name) => {
                            if worker.query.active_column_name(query.str_at_cursor()) != col_name
                                && worker.columns.iter().any(|c| *c.name == col_name)
                            {
                                query.prepare_column_change();
                                query.push_str(&format!("%{} ", col_name));
                            } else {
                                log::warn!("Column {} not found in worker columns", col_name);
                            }
                        }
                        Action::NextColumn | Action::PrevColumn => {
                            let cursor_byte = query.byte_index(query.cursor() as usize);
                            let active_idx = worker.query.active_column_index(cursor_byte);

                            let num_columns = worker.columns.len();
                            if num_columns > 0 {
                                query.prepare_column_change();

                                let mut next_idx = match action {
                                    Action::NextColumn => active_idx + 1,
                                    Action::PrevColumn => {
                                        active_idx + num_columns - 1 % num_columns
                                    }
                                    _ => unreachable!(),
                                } % num_columns;

                                loop {
                                    if next_idx < results.hidden_columns.len()
                                        && results.hidden_columns[next_idx]
                                    {
                                        next_idx = match action {
                                            Action::NextColumn => (next_idx + 1) % num_columns,
                                            Action::PrevColumn => {
                                                (next_idx + num_columns - 1) % num_columns
                                            }
                                            _ => unreachable!(),
                                        };
                                    } else {
                                        break;
                                    }
                                }

                                let col_name = &worker.columns[next_idx].name;
                                query.push_str(&format!("%{} ", col_name));
                            }
                        }

                        Action::ToggleColumn(col_name) => {
                            let index = if let Some(name) = col_name {
                                worker.columns.iter().position(|c| *c.name == name)
                            } else {
                                let cursor_byte = query.byte_index(query.cursor() as usize);
                                Some(worker.query.active_column_index(cursor_byte))
                            };

                            if let Some(idx) = index {
                                if idx >= results.hidden_columns.len() {
                                    results.hidden_columns.resize(idx + 1, false);
                                }
                                results.hidden_columns[idx] = !results.hidden_columns[idx];
                            }
                        }

                        Action::ShowColumn(col_name) => {
                            if let Some(name) = col_name {
                                if let Some(idx) =
                                    worker.columns.iter().position(|c| *c.name == name)
                                {
                                    if idx < results.hidden_columns.len() {
                                        results.hidden_columns[idx] = false;
                                    }
                                }
                            } else {
                                for val in results.hidden_columns.iter_mut() {
                                    *val = false;
                                }
                            }
                        }

                        // Edit
                        Action::SetQuery(context) => {
                            if *action_visible {
                                action_input.set(context, u16::MAX);
                            } else {
                                query.set(context, u16::MAX);
                            }
                        }
                        Action::ForwardChar => {
                            if *action_visible {
                                action_input.forward_char()
                            } else {
                                query.forward_char()
                            }
                        }
                        Action::BackwardChar => {
                            if *action_visible {
                                action_input.backward_char()
                            } else {
                                query.backward_char()
                            }
                        }
                        Action::ForwardWord => {
                            if *action_visible {
                                action_input.forward_word()
                            } else {
                                query.forward_word()
                            }
                        }
                        Action::BackwardWord => {
                            if *action_visible {
                                action_input.backward_word()
                            } else {
                                query.backward_word()
                            }
                        }
                        Action::DeleteChar => {
                            if *action_visible {
                                action_input.delete()
                            } else {
                                query.delete()
                            }
                        }
                        Action::DeleteWord => {
                            if *action_visible {
                                action_input.delete_word()
                            } else {
                                query.delete_word()
                            }
                        }
                        Action::DeleteLineStart => {
                            if *action_visible {
                                action_input.delete_line_start()
                            } else {
                                query.delete_line_start()
                            }
                        }
                        Action::DeleteLineEnd => {
                            if *action_visible {
                                action_input.delete_line_end()
                            } else {
                                query.delete_line_end()
                            }
                        }
                        Action::Cancel => {
                            if *action_visible {
                                *action_visible = false;
                                action_input.cancel();
                            } else {
                                query.cancel()
                            }
                        }

                        // Other
                        Action::Redraw => {
                            tui.redraw();
                        }
                        Action::ToggleFocus => {
                            if ui.config.nav_mode {
                                state.focus = match state.focus {
                                    Focus::Input => Focus::Results,
                                    Focus::Results => Focus::Input,
                                };
                                state.focus_blink = true;
                                state.focus_tick = 0;

                                let prompt = &ui.config.nav_prompt;
                                if !prompt.is_empty() {
                                    let prompt = match state.focus {
                                        Focus::Results => {
                                            Some(ratatui::text::Line::raw(prompt.clone()))
                                        }
                                        Focus::Input => None,
                                    };
                                    picker_ui.query.set_prompt(prompt);
                                }
                            }
                        }
                        Action::Overlay(index) => {
                            if let Some(x) = overlay_ui.as_mut() {
                                x.enable(index, &ui.area());
                                tui.redraw();
                            };
                        }
                        Action::Custom(e) => {
                            if let Some(handler) = &mut ext_handler {
                                handler(
                                    e,
                                    &mut state.dispatcher(
                                        &mut ui,
                                        &mut picker_ui,
                                        &mut footer_ui,
                                        &mut preview_ui,
                                        &controller_tx,
                                    ),
                                );
                            }
                        }
                        Action::Char(c) => {
                            if *action_visible {
                                action_input.push_char(c)
                            } else if !(ui.config.nav_mode && state.focus == Focus::Results) {
                                query.push_char(c)
                            }
                        }
                        Action::SetMode(s) => {
                            if let Ok(mut m) = crate::MODE.lock() {
                                *m = s;
                            }
                        }

                        // unreachable
                        Action::PrintKey => {}
                        Action::Semantic(_) => {}
                        Action::Trace(_) => {}
                    }
                }
                _ => {}
            }

            let interrupt = state.interrupt();

            match interrupt {
                Interrupt::None => continue,
                Interrupt::Execute => {
                    // because of this, we don't want to send controller events until after resuming at batch end
                    if controller_tx.send(Event::Pause).is_err() {
                        break;
                    }
                    tui.enter_execute();
                    if did_exit.is_none() {
                        did_exit = Some(true);
                    }
                    did_pause = true;
                }
                Interrupt::Reload => {
                    picker_ui.worker.restart(false);
                    state.synced = [false; 2];
                    did_reload = true;
                }
                Interrupt::Become => {
                    tui.exit(None);
                }
                Interrupt::BecomeSilent => {
                    tui.exit_lite();
                }
                _ => {}
            }
            // Apply interrupt effect
            {
                let mut dispatcher = state.dispatcher(
                    &mut ui,
                    &mut picker_ui,
                    &mut footer_ui,
                    &mut preview_ui,
                    &controller_tx,
                );
                for h in dynamic_handlers.1.get_mut(interrupt) {
                    h(&mut dispatcher);
                }

                if matches!(interrupt, Interrupt::Become) {
                    return Err(MatchError::Become(state.payload().clone()));
                }
            }

            if state.should_quit {
                log::debug!("Exiting due to should_quit");
                return if picker_ui.selector.is_disabled()
                    && let Some((_, item)) = get_current(&picker_ui)
                {
                    Ok(vec![item])
                } else {
                    Ok(picker_ui.selector.output().collect())
                };
            } else if state.should_quit_nomatch {
                log::debug!("Exiting due to should_quit_nomatch");
                return Err(MatchError::NoMatch);
            }
        }

        // debug!("{state:?}");

        // ------------- update state + render ------------------------
        if state.filtering {
            picker_ui.update();
        } else {
            // nothing
        }
        if did_cursor_wrap {
            log::trace!("cursor wrapped"); // todo: event handler?
        }

        // process exit conditions
        if exit_config.select_1
            && picker_ui.results.status.matched_count == 1
            && let Some((_, item)) = get_current(&picker_ui)
        {
            return Ok(vec![item]);
        }

        // resume tui
        if let Some(clear) = did_exit {
            tui.return_execute(clear)
                .map_err(|e| MatchError::TUIError(e.to_string()))?;
            tui.redraw();
        }

        if ui.config.nav_mode {
            let blink_ticks = ui.config.nav_blink_rate.ticks();
            state.focus_tick = state.focus_tick.wrapping_add(1);
            if state.focus_tick >= blink_ticks {
                state.focus_tick = 0;
                state.focus_blink = !state.focus_blink;
            }
        }

        let mut overlay_ui_ref = overlay_ui.as_mut();
        let mut cursor_y_offset = 0;

        tui.terminal
            .draw(|frame| {
                let mut area = frame.area();

                // mutates area!
                render_ui(frame, &mut area, &ui);

                let mut _area = area;

                let full_width_footer = footer_ui.is_single_column()
                    && footer_ui.config.row_connection == RowConnectionStyle::Full;

                let mut footer =
                    if full_width_footer || preview_ui.as_ref().is_none_or(|p| !p.visible()) {
                        split(&mut _area, footer_ui.height(), picker_ui.reverse())
                    } else {
                        Rect::default()
                    };

                let [preview, picker_area, footer, gap_area] = if let Some(preview_ui) =
                    preview_ui.as_mut()
                    && preview_ui.visible()
                {
                    let [preview, mut picker_area, gap_area] = preview_ui.split(_area);

                    if state.iterations == 0 && picker_area.width <= 5 {
                        warn!("UI too narrow, hiding preview");
                        preview_ui.show(false);

                        [Rect::default(), _area, footer, Rect::default()]
                    } else {
                        if !full_width_footer {
                            footer =
                                split(&mut picker_area, footer_ui.height(), picker_ui.reverse());
                        }

                        [preview, picker_area, footer, gap_area]
                    }
                } else {
                    [Rect::default(), _area, footer, Rect::default()]
                };

                let [action, input, status, header, results] = picker_ui.layout(picker_area);

                // save dimensions and check if dimensions changed
                did_resize = state.update_layout(Layout {
                    preview,
                    action,
                    input,
                    status,
                    header,
                    results,
                    footer,
                    gap: gap_area,
                    pane: _area,
                });

                if did_resize {
                    picker_ui.results.update_dimensions(&results);
                    picker_ui.action.update_width(action.width);
                    picker_ui.query.update_width(input.width);
                    footer_ui.update_width(
                        if footer_ui.config.row_connection == RowConnectionStyle::Capped {
                            area.width
                        } else {
                            footer.width
                        },
                    );
                    picker_ui.header.update_width(header.width);
                    // although these only want update when the whole ui change
                    ui.update_dimensions(area);
                    if let Some(x) = overlay_ui_ref.as_deref_mut() {
                        x.update_dimensions(&area);
                    }
                    if let Some(preview_ui) = preview_ui.as_mut() {
                        preview_ui.update_dimensions(&preview);
                    }
                };

                let status_inline_label: Option<Line<'_>> = if picker_ui.query.config.status_inline
                {
                    Some(picker_ui.results.status_line())
                } else {
                    None
                };

                let nav_color = ui.config.nav_color;
                let nav_mode = ui.config.nav_mode;
                let nav_do_blink = ui.config.nav_blink;
                let nav_bold = ui.config.nav_bold;
                let nav_bar = ui.config.nav_bar;
                let nav_marker = ui.config.nav_marker.clone();
                let input_focus_info = nav_mode.then_some(FocusInfo {
                    focused: state.focus == Focus::Input,
                    blink_phase: state.focus_blink,
                    color: nav_color,
                    do_blink: nav_do_blink,
                    bold: nav_bold,
                    bar: nav_bar,
                    marker: String::new(),
                });
                let results_focus_info = nav_mode.then_some(FocusInfo {
                    focused: state.focus == Focus::Results,
                    blink_phase: state.focus_blink,
                    color: nav_color,
                    do_blink: nav_do_blink,
                    bold: nav_bold,
                    bar: nav_bar,
                    marker: nav_marker,
                });

                if picker_ui.action_visible {
                    let cfg = &picker_ui.action_config;
                    // Apply width percentage — centered within the allocated row(s).
                    let action_w = cfg.width_pct.compute_clamped(action.width, 1, 0);
                    let x_pad = action.width.saturating_sub(action_w) / 2;
                    let action_full_rect = Rect {
                        x: action.x + x_pad,
                        y: action.y,
                        width: action_w,
                        height: action.height,
                    };
                    // Render the separator border (default: bottom line) over the full area.
                    // It only draws at the edges of `action_full_rect`, so the input and
                    // preview content rendered afterwards is not obscured.
                    if !cfg.border.is_empty() {
                        frame.render_widget(cfg.border.as_static_block(), action_full_rect);
                    }

                    let action_input_rect = Rect {
                        x: action_full_rect.x,
                        y: action_full_rect.y,
                        width: action_w,
                        height: action_full_rect.height.min(1),
                    };
                    render_input(frame, action_input_rect, &mut picker_ui.action, None, None);

                    // Render preview area below if preview_height > 0.
                    if cfg.preview_height > 0 && action.height > 2 {
                        let preview_rect = Rect {
                            x: action_full_rect.x,
                            y: action_full_rect.y + 1,
                            width: action_w,
                            height: cfg.preview_height,
                        };
                        let block = ratatui::widgets::Block::bordered();
                        frame.render_widget(block, preview_rect);
                    }
                }
                cursor_y_offset = render_input(
                    frame,
                    input,
                    &mut picker_ui.query,
                    status_inline_label,
                    input_focus_info,
                )
                .y;
                // When status_inline is active, skip the separate status row.
                if !picker_ui.query.config.status_inline {
                    render_status(frame, status, &picker_ui.results, ui.area().width);
                }
                render_results(
                    frame,
                    results,
                    &mut picker_ui,
                    &mut click,
                    results_focus_info,
                );
                render_display(frame, header, &mut picker_ui.header, &picker_ui.results);
                render_display(frame, footer, &mut footer_ui, &picker_ui.results);
                if let Some(preview_ui) = preview_ui.as_mut() {
                    state.update_preview_visible(preview_ui);
                    if preview_ui.visible() {
                        // Set the dynamic title from the first column of the current item.
                        let item_title = picker_ui
                            .worker
                            .get_nth(picker_ui.results.index())
                            .map(|item| picker_ui.worker.columns[0].raw(item).into_owned());
                        preview_ui.set_title(item_title);
                        render_preview(frame, preview, preview_ui);

                        // Highlight the gap area when the mouse is hovering over it.
                        if !gap_area.is_empty() {
                            let is_hovered = mouse_hover.is_some_and(|p| gap_area.contains(p));
                            let is_dragging = state.dragging.is_some();
                            if is_hovered || is_dragging {
                                use ratatui::style::Color as C;
                                use ratatui::widgets::Block;
                                let gap_block = Block::default()
                                    .style(ratatui::style::Style::default().bg(C::DarkGray));
                                frame.render_widget(gap_block, gap_area);
                            }
                        }
                    }
                }
                if let Some(x) = overlay_ui_ref {
                    x.draw(frame);
                }
            })
            .map_err(|e| MatchError::TUIError(e.to_string()))?;

        if did_resize {
            // useful to clear artifacts
            if tui.config.redraw_on_resize && did_exit.is_none() {
                tui.redraw();
            }
        }

        drop(events);
        buffer.clear();

        // note: the remainder could be scoped by a conditional on having run?
        // ====== Event handling ==========
        state.update(&picker_ui, &overlay_ui);
        let events = state.events();

        // ---- Invoke handlers -------
        let mut dispatcher = state.dispatcher(
            &mut ui,
            &mut picker_ui,
            &mut footer_ui,
            &mut preview_ui,
            &controller_tx,
        );
        // if let Some((signal, handler)) = signal_handler &&
        // let s = signal.load(std::sync::atomic::Ordering::Acquire) &&
        // s > 0
        // {
        //     handler(s, &mut dispatcher);
        //     signal.store(0, std::sync::atomic::Ordering::Release);
        // };

        // ping handlers with events
        for e in events.iter() {
            for h in dynamic_handlers.0.get(e) {
                h(&mut dispatcher, &e)
            }
        }
        state.reset();

        // ------------------------------
        // send events into controller
        for e in events.iter() {
            controller_tx.send(e)._elog();
        }
        // =================================

        if did_pause {
            log::debug!("Waiting for ack response to pause");
            if controller_tx.send(Event::Resume).is_err() {
                break;
            };
            // due to control flow, this does nothing, but is anyhow a useful safeguard to guarantee the pause
            while let Some(msg) = render_rx.recv().await {
                if matches!(msg, RenderCommand::Ack) {
                    log::debug!("Received ack response to pause");
                    break;
                }
            }
        }
        if did_reload {
            controller_tx.send(Event::Reloaded)._elog();
        }

        click.process(&mut buffer, &bind_tx);
    }

    Err(MatchError::EventLoopClosed)
}

// ------------------------- HELPERS ----------------------------

struct FocusInfo {
    focused: bool,
    blink_phase: bool,
    color: Color,
    do_blink: bool,
    bold: bool,
    bar: Option<ratatui::widgets::BorderType>,
    marker: String,
}

impl FocusInfo {
    fn indicator_color(&self) -> Option<Color> {
        if self.focused {
            Some(if self.do_blink && !self.blink_phase {
                Color::DarkGray
            } else {
                self.color
            })
        } else {
            None
        }
    }
}

pub enum Click {
    None,
    ResultPos(u16),
    ResultIdx(u32),
    Semantic(String),
}

impl Click {
    fn process<A: ActionExt>(
        &mut self,
        buffer: &mut Vec<RenderCommand<A>>,
        bind_tx: &BindSender<A>,
    ) {
        match self {
            Self::ResultIdx(u) => {
                buffer.push(RenderCommand::Action(Action::Pos(*u as i32)));
            }
            Self::Semantic(s) => {
                bind_tx
                    .send(BindDirective::Action(Action::Semantic(s.clone())))
                    ._elog();
                log::debug!("Click triggered: @{s}");
            }
            _ => {}
        }
        *self = Click::None
    }
}

fn find_interaction(setting: &crate::config::InteractionRegionSetting, x: u16) -> Option<String> {
    setting
        .iter()
        .rev()
        .find(|(start, _)| x >= *start as u16)
        .map(|(_, action)| action.clone())
        .filter(|a| !a.is_empty())
}

fn render_preview(frame: &mut Frame, area: Rect, ui: &mut PreviewUI) {
    // if ui.view.changed() {
    // doesn't work, use resize
    //     frame.render_widget(Clear, area);
    // } else {
    //     let widget = ui.make_preview();
    //     frame.render_widget(widget, area);
    // }
    assert!(ui.visible()); // don't call if not visible.
    let widget = ui.make_preview();
    frame.render_widget(widget, area);
}

fn render_results<T: SSS, S: Selection>(
    frame: &mut Frame,
    mut area: Rect,
    ui: &mut PickerUI<T, S>,
    click: &mut Click,
    focus_info: Option<FocusInfo>,
) {
    let cap = matches!(ui.results.config.row_connection, RowConnectionStyle::Capped);
    let (widget, table_width) = ui.make_table(click);

    if cap {
        area.width = area.width.min(table_width);
    }

    frame.render_widget(widget, area);

    if let Some(fi) = focus_info {
        let bar_color = fi.indicator_color();

        if let (Some(border_type), Some(color)) = (fi.bar, bar_color) {
            let mut style = Style::default().fg(color);
            if fi.bold {
                style = style.add_modifier(Modifier::BOLD);
            }
            let indicator = Block::default()
                .borders(Borders::LEFT)
                .border_type(border_type)
                .border_style(style);

            let bar_height = (ui.results.status.matched_count as u16).min(area.height);
            let bar_area = if ui.results.reverse() {
                Rect {
                    y: area.y + area.height - bar_height,
                    height: bar_height,
                    ..area
                }
            } else {
                Rect {
                    height: bar_height,
                    ..area
                }
            };

            frame.render_widget(indicator, bar_area);
        }

        if !fi.marker.is_empty()
            && let Some(cursor_row) = ui.results.cursor_offset()
        {
            let marker_y = if ui.results.reverse() {
                area.y + area.height.saturating_sub(1 + cursor_row)
            } else {
                area.y + cursor_row
            };
            if marker_y >= area.y && marker_y < area.y + area.height {
                let marker_color = bar_color.unwrap_or(fi.color);
                let marker_w = unicode_width::UnicodeWidthStr::width(fi.marker.as_str()) as u16;
                let marker_rect = Rect {
                    x: area.x,
                    y: marker_y,
                    width: marker_w.min(area.width),
                    height: 1,
                };
                let span = Span::styled(fi.marker.clone(), Style::default().fg(marker_color));
                frame.render_widget(Paragraph::new(span), marker_rect);
            }
        }
    }
}

/// Returns the offset of the cursor against the drawing area
fn render_input(
    frame: &mut Frame,
    area: Rect,
    ui: &mut QueryUI,
    status: Option<Line<'_>>,
    focus_info: Option<FocusInfo>,
) -> Position {
    ui.scroll_to_cursor();
    let widget = if let Some(label) = status {
        ui.make_input_with_status(label, area.width)
    } else {
        ui.make_input()
    };
    let p = ui.cursor_offset(&area);

    let show_cursor = focus_info.as_ref().map_or(true, |f| f.focused);
    if let CursorSetting::Default = ui.config.cursor {
        if show_cursor {
            frame.set_cursor_position(p)
        }
    };

    frame.render_widget(widget, area);

    if let Some(fi) = focus_info {
        if let (Some(border_type), Some(color)) = (fi.bar, fi.indicator_color()) {
            let mut style = Style::default().fg(color);
            if fi.bold {
                style = style.add_modifier(Modifier::BOLD);
            }
            let indicator = Block::default()
                .borders(Borders::LEFT)
                .border_type(border_type)
                .border_style(style);
            frame.render_widget(indicator, area);
        }
    }

    p
}

fn render_status(frame: &mut Frame, area: Rect, ui: &ResultsUI, full_width: u16) {
    if ui.status_config.show {
        let widget = ui.make_status(full_width);
        frame.render_widget(widget, area);
    }
}

fn render_display(frame: &mut Frame, area: Rect, ui: &mut DisplayUI, results_ui: &ResultsUI) {
    if !ui.show {
        return;
    }
    let widths = results_ui.widths().to_vec();

    let widget = ui.make_display(
        results_ui.indentation() as u16 + results_ui.config.border.left(),
        widths,
        results_ui.config.column_spacing.0,
    );

    frame.render_widget(widget, area);

    if ui.is_single_column() {
        let widget = ui.make_full_width_row(results_ui.indentation() as u16);
        frame.render_widget(widget, area);
    }
}

// a bit weird, do we want mutable, do we want &mut ui, whatever this is simplest
fn render_ui(frame: &mut Frame, area: &mut Rect, ui: &UI) {
    let widget = ui.make_ui();
    frame.render_widget(widget, *area);
    *area = ui.compute_area(area);
}

fn split(rect: &mut Rect, height: u16, cut_top: bool) -> Rect {
    let h = height.min(rect.height);

    if cut_top {
        let offshoot = Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: h,
        };

        rect.y += h;
        rect.height -= h;

        offshoot
    } else {
        let offshoot = Rect {
            x: rect.x,
            y: rect.y + rect.height - h,
            width: rect.width,
            height: h,
        };

        rect.height -= h;

        offshoot
    }
}

// -----------------------------------------------------------------------------------

#[cfg(test)]
mod test {}

// #[cfg(test)]
// async fn send_every_second(tx: mpsc::UnboundedSender<RenderCommand>) {
//     let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));

//     loop {
//         interval.tick().await;
//         if tx.send(RenderCommand::quit()).is_err() {
//             break;
//         }
//     }
// }
