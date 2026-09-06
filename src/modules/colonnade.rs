//! Column-grouped, niri-aware tab strip -- a native port of Colonnade
//! (github.com/yesvus/colonnade, MIT) onto its own toolkit-independent
//! `colonnade-core` crate.
//!
//! As of the Firefox-style rework: only the *active* workspace ever
//! renders (no bloom/collapse markers for the others -- dropped along
//! with `colonnade_core::{glyph, slice}`, which this module no longer
//! uses). Its tabs share a fixed-width strip equally (one tab per niri
//! column), shrinking together as more open -- exactly a browser's tab
//! strip, not proportional to the real niri tile size -- clamped between
//! `min_tab_width_px` and `max_tab_width_px`. See `firefox_tab_width`,
//! `bloomed_view`, and `edge_fade_stack` (the "more to scroll" affordance,
//! shown only on whichever side is actually still hidden).
//!
//! Holds its own `colonnade_core::niri::Niri` client and event stream,
//! independent of Lumen's own multi-compositor `services::compositor`
//! abstraction -- niri columns have no equivalent in that model, and
//! Colonnade is inherently niri-only, so there is nothing to share there.

use std::collections::HashMap;

use colonnade_core::{
    column::{self, Column},
    niri::{Niri, Snapshot, Window, WorkspaceInfo},
};
use iced::{
    Alignment, Background, Color, Element, Gradient, Length, Subscription, SurfaceId,
    widget::{
        Image, MouseArea, Row, Svg, button, container,
        scrollable::{self, Scrollable},
        text,
    },
};
use iced_anim::{AnimationBuilder, spring::Motion};
use log::warn;

use crate::{
    config::ColonnadeModuleConfig,
    outputs::Outputs,
    services::xdg_icons::{self, XdgIcon},
    theme::{LumenTheme, use_theme},
};

#[derive(Debug, Clone)]
pub enum Message {
    Snapshot(Snapshot),
    OutputWidths(HashMap<String, f64>),
    FocusWindow(u64),
    MaximizeWindow(u64),
    CloseWindow(u64),
    /// Mouse wheel over the tab strip: cycle niri's column focus left/right
    /// (not a plain pan -- see `bloomed_view`'s comment on why).
    ScrollLeft,
    ScrollRight,
    /// Trackpad smooth-scroll sub-threshold accumulator, same
    /// debounce shape `workspaces.rs` uses for its own scroll handling.
    ScrollAccumulator(f32),
    ConfigReloaded(ColonnadeModuleConfig),
}

pub struct Colonnade {
    config: ColonnadeModuleConfig,
    niri: Niri,
    snapshot: Snapshot,
    /// Logical width per niri output name -- fetched separately from the
    /// window/workspace event stream (which carries neither).
    output_widths: HashMap<String, f64>,
    /// Last time `output_widths` was (re)fetched -- see
    /// `OUTPUT_FETCH_MIN_INTERVAL`.
    last_output_fetch: Option<std::time::Instant>,
    scroll_accumulator: f32,
    /// One `scrollable::Id` per (bloomed) workspace -- needed to target a
    /// specific strip with `scrollable::scroll_to` from `update()`. Keyed
    /// by workspace id rather than output name: each output shows exactly
    /// one active workspace at a time, and this naturally gives each a
    /// distinct, stable id across renders without an `Option<String>` key.
    scroll_ids: std::cell::RefCell<HashMap<u64, iced::widget::Id>>,
    /// The last window id known to be focused on each (bloomed) workspace
    /// -- lets `update()` notice when focus moves to a column that isn't
    /// necessarily visible and scroll it into view, instead of leaving
    /// off-strip columns permanently unreachable (wheel-scroll here moves
    /// niri's focus, it doesn't pan the strip by itself -- see
    /// `bloomed_view`'s comment on why).
    last_focused: HashMap<u64, u64>,
    /// This module's own record of each (bloomed) workspace's current
    /// scroll-x, since the `Scrollable` can't hand it back -- see
    /// `scroll_focused_tabs_into_view`'s doc comment for why.
    scroll_offsets: HashMap<u64, f32>,
}

/// How long a tab's width spring animation takes to respond --
/// `Motion::with_duration`, see `tab_view`.
///
/// 100ms here first shipped as visually "bouncing" endlessly whenever a
/// tab opened/closed and every sibling retargeted at once (a
/// critically-damped spring's stiffness scales with `1/duration`, and
/// 100ms was stiff enough to fight fixed-timestep integration at ~60Hz).
/// 260ms fixed it. Set to 150ms now to match niri's own
/// `window-movement`/`horizontal-view-movement` animation duration
/// (`~/.config/niri/config.kdl`'s `animations` block) -- if the bounce
/// reappears at this stiffness, dial back towards 200-260ms rather than
/// go lower.
const TAB_ANIMATION: std::time::Duration = std::time::Duration::from_millis(150);

/// Minimum time between `niri.outputs()` IPC round-trips. Output geometry
/// only changes on monitor plug/unplug/mode-change -- effectively never --
/// but `Message::Snapshot` fires on every `WindowLayoutsChanged` event,
/// which niri emits on *every compositor frame* while it eases a column
/// resize (e.g. the mod+R preset-width cycle). Without this throttle, a
/// single resize burst opened a fresh Unix socket and did a blocking
/// request/reply to niri once per frame just to refetch numbers that
/// never moved -- adding real per-frame latency on top of Colonnade's own
/// width-smoothing, which is what made tab text and overflow dashes look
/// like they were catching up in slow motion during a resize.
const OUTPUT_FETCH_MIN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

impl Colonnade {
    pub fn new(config: ColonnadeModuleConfig) -> Self {
        Self {
            config,
            niri: Niri::new(),
            snapshot: Snapshot::default(),
            output_widths: HashMap::new(),
            last_output_fetch: None,
            scroll_accumulator: 0.0,
            scroll_ids: std::cell::RefCell::new(HashMap::new()),
            last_focused: HashMap::new(),
            scroll_offsets: HashMap::new(),
        }
    }

    /// The active workspace on `output`, if any -- the only one this
    /// module ever renders now.
    fn active_workspace(&self, output: Option<&str>) -> Option<&WorkspaceInfo> {
        self.snapshot
            .workspaces
            .iter()
            .filter(|ws| output.is_none_or(|o| ws.output.as_deref() == Some(o)))
            .find(|ws| ws.is_active)
    }

    /// The stable `scrollable::Id` for `workspace_id`'s tab strip -- same
    /// id every render, created on first use. See `Self::scroll_ids`.
    fn scroll_id_for(&self, workspace_id: u64) -> iced::widget::Id {
        self.scroll_ids
            .borrow_mut()
            .entry(workspace_id)
            .or_insert_with(iced::widget::Id::unique)
            .clone()
    }

    /// The focused column's left edge, its width, and the strip's full
    /// (unclipped) content width, for `workspace` -- the geometry
    /// `update()` needs to scroll the focused column into view. `None`
    /// if there's nothing to scroll to (empty workspace).
    fn focused_tab_geometry(
        &self,
        workspace: &WorkspaceInfo,
        output_width: f64,
        strip_width: f32,
    ) -> Option<(u64, f32, f32, f32)> {
        let windows: Vec<Window> = self
            .snapshot
            .windows
            .iter()
            .filter(|w| w.workspace_id == Some(workspace.id))
            .cloned()
            .collect();
        let columns = column::group(
            &windows,
            output_width,
            self.config.tab_width_scale_px,
            self.config.min_tab_width_px,
            self.config.dynamic_tab_width,
        );
        if columns.is_empty() {
            return None;
        }
        let gap = use_theme(|t| t.space.xxs);
        let tab_width = firefox_tab_width(
            columns.len(),
            strip_width,
            gap,
            self.config.min_tab_width_px as f32,
            self.config.max_tab_width_px as f32,
        );
        let n = columns.len() as f32;
        let content_width = n * tab_width + gap * (n - 1.0).max(0.0);
        let focus_idx = columns
            .iter()
            .position(|c| c.window.is_focused || Some(c.window.id) == workspace.active_window_id)
            .unwrap_or(0);
        let left_edge = focus_idx as f32 * (tab_width + gap);
        Some((
            columns[focus_idx].window.id,
            left_edge,
            tab_width,
            content_width,
        ))
    }

    /// Scrolls each bloomed workspace's strip to keep its focused column
    /// visible, for whichever workspaces' focus actually changed since the
    /// last snapshot -- see `Self::last_focused`.
    ///
    /// Minimal scroll, not centering: if the focused tab is already fully
    /// within the visible range, the viewport doesn't move at all; if it's
    /// off the left edge, the viewport moves just enough to bring its left
    /// edge into view (symmetrically for the right edge). This is the
    /// standard "scroll a list item into view" behavior -- centering every
    /// time was visually restless, moving the whole strip even when the
    /// newly-focused tab was already on-screen.
    ///
    /// Tracks the resulting offset itself in `Self::scroll_offsets` rather
    /// than reading it back from the `Scrollable`: programmatic
    /// `scroll_to` (unlike a user drag/wheel) updates the widget's
    /// internal state through `Widget::operate`, which has no `Shell` to
    /// publish `on_scroll`'s callback through -- there is no event to read
    /// the new position back from. Self-tracking is safe here because nothing
    /// else ever moves this strip: wheel input over it is captured by the
    /// wrapping `MouseArea` for niri's column focus (see `bloomed_view`)
    /// before the `Scrollable` underneath ever sees it, and its scrollbar
    /// is hidden (0-width), so this method is the *only* thing that moves it.
    fn scroll_focused_tabs_into_view(&mut self) -> iced::Task<Message> {
        let strip_width = self.config.max_group_width_px as f32;
        let workspaces: Vec<WorkspaceInfo> = self
            .snapshot
            .workspaces
            .iter()
            .filter(|ws| ws.is_active)
            .cloned()
            .collect();

        let mut tasks = Vec::new();
        for ws in workspaces {
            let output_width = ws
                .output
                .as_deref()
                .and_then(|name| self.output_widths.get(name))
                .copied()
                .unwrap_or_default();
            let Some((focused_id, left_edge, tab_width, content_width)) =
                self.focused_tab_geometry(&ws, output_width, strip_width)
            else {
                continue;
            };
            if self.last_focused.get(&ws.id) == Some(&focused_id) {
                continue;
            }
            self.last_focused.insert(ws.id, focused_id);

            let max_offset = (content_width - strip_width).max(0.0);
            let current_offset = self.scroll_offsets.get(&ws.id).copied().unwrap_or(0.0);
            let tab_end = left_edge + tab_width;
            let visible_end = current_offset + strip_width;

            let target_x = if left_edge < current_offset {
                left_edge
            } else if tab_end > visible_end {
                tab_end - strip_width
            } else {
                current_offset
            }
            .clamp(0.0, max_offset);

            if (target_x - current_offset).abs() < 0.5 {
                continue;
            }
            self.scroll_offsets.insert(ws.id, target_x);

            let task: iced::Task<Message> = iced_runtime::widget::operation::scroll_to(
                self.scroll_id_for(ws.id),
                scrollable::AbsoluteOffset {
                    x: target_x,
                    y: 0.0,
                },
            )
            .into();
            tasks.push(task);
        }
        iced::Task::batch(tasks)
    }

    pub fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            Message::Snapshot(snapshot) => {
                self.snapshot = snapshot;
                let scroll_task = self.scroll_focused_tabs_into_view();

                let due = self
                    .last_output_fetch
                    .is_none_or(|t| t.elapsed() >= OUTPUT_FETCH_MIN_INTERVAL);
                if !due {
                    return scroll_task;
                }
                self.last_output_fetch = Some(std::time::Instant::now());
                let niri = self.niri;
                return iced::Task::batch([
                    scroll_task,
                    iced::Task::perform(
                        async move { tokio::task::spawn_blocking(move || niri.outputs()).await },
                        |result| match result {
                            Ok(Ok(outputs)) => Message::OutputWidths(
                                outputs
                                    .into_iter()
                                    .map(|(name, output)| {
                                        let width = output
                                            .logical
                                            .map(|l| l.width as f64)
                                            .unwrap_or_default();
                                        (name, width)
                                    })
                                    .collect(),
                            ),
                            Ok(Err(e)) => {
                                warn!("colonnade: error fetching niri outputs: {e}");
                                Message::OutputWidths(HashMap::new())
                            }
                            Err(e) => {
                                warn!("colonnade: output-fetch task panicked: {e}");
                                Message::OutputWidths(HashMap::new())
                            }
                        },
                    ),
                ]);
            }
            Message::OutputWidths(widths) => {
                if !widths.is_empty() {
                    self.output_widths = widths;
                }
            }
            Message::FocusWindow(id) => self.run(move |niri| niri.activate_window(id)),
            Message::MaximizeWindow(id) => self.run(move |niri| niri.maximize_window(id)),
            Message::CloseWindow(id) => self.run(move |niri| niri.close_window(id)),
            Message::ScrollLeft => {
                self.scroll_accumulator = 0.0;
                self.run(|niri| niri.focus_column_left());
            }
            Message::ScrollRight => {
                self.scroll_accumulator = 0.0;
                self.run(|niri| niri.focus_column_right());
            }
            Message::ScrollAccumulator(value) => {
                if self.scroll_accumulator.signum() != value.signum() {
                    self.scroll_accumulator = 0.0;
                }
                self.scroll_accumulator += value;
            }
            Message::ConfigReloaded(config) => self.config = config,
        }
        iced::Task::none()
    }

    /// Fires a niri action on a blocking thread, fire-and-forget -- the
    /// next `Snapshot` naturally reflects whatever changed, so there is
    /// nothing to feed back into a `Message` on success. Matches
    /// `utils::launcher::execute_command`'s fire-and-forget shape.
    fn run(
        &self,
        action: impl FnOnce(&Niri) -> Result<(), colonnade_core::error::Error> + Send + 'static,
    ) {
        let niri = self.niri;
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || action(&niri)).await;
            match result {
                Ok(Err(e)) => warn!("colonnade: niri action failed: {e}"),
                Err(e) => warn!("colonnade: niri action task panicked: {e}"),
                Ok(Ok(())) => {}
            }
        });
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::run(|| {
            iced::stream::channel(10, async move |mut output| {
                let niri = Niri::new();
                let stream = niri.window_stream();
                while let Some(snapshot) = stream.next().await {
                    if output.try_send(Message::Snapshot(snapshot)).is_err() {
                        break;
                    }
                }
            })
        })
    }

    pub fn view<'a>(&'a self, id: SurfaceId, outputs: &Outputs) -> Element<'a, Message> {
        let monitor_name = outputs.get_monitor_name(id);
        let output_width = monitor_name
            .and_then(|name| self.output_widths.get(name))
            .copied()
            .unwrap_or_default();

        let Some(workspace) = self.active_workspace(monitor_name) else {
            return Row::new().into();
        };

        let windows: Vec<Window> = self
            .snapshot
            .windows
            .iter()
            .filter(|w| w.workspace_id == Some(workspace.id))
            .cloned()
            .collect();

        self.bloomed_view(workspace.id, &windows, output_width)
    }

    /// Renders the active workspace's tabs only -- no workspace number
    /// here any more (`workspace_indicator.rs`'s job now) and no markers
    /// for other workspaces (dropped entirely; this module only ever
    /// shows the active one). Every tab shares `strip_width` equally,
    /// Firefox-style, shrinking together as more open down to
    /// `min_tab_width_px`, below which the strip scrolls instead of
    /// shrinking further -- see `firefox_tab_width`.
    fn bloomed_view<'a>(
        &'a self,
        workspace_id: u64,
        windows: &[Window],
        output_width: f64,
    ) -> Element<'a, Message> {
        let space = use_theme(|t| t.space);
        let strip_width = self.config.max_group_width_px as f32;

        // `column::group` also computes a per-column pixel width from the
        // real niri tile size (`.target_width_px`, `.width_fraction`) --
        // deliberately unused below. Firefox-style equal-share sizing
        // (`firefox_tab_width`) replaces it; `group` is still the right
        // call for the grouping/ordering/dedup it does (one `Column` per
        // niri column, sorted, one-window-per-column invariant enforced).
        let columns = column::group(
            windows,
            output_width,
            self.config.tab_width_scale_px,
            self.config.min_tab_width_px,
            self.config.dynamic_tab_width,
        );

        if columns.is_empty() {
            return container(Row::new())
                .width(Length::Fixed(strip_width))
                .into();
        }

        let font_size = use_theme(|t| t.font_size.sm);
        let gap = space.xxs;
        let tab_width = firefox_tab_width(
            columns.len(),
            strip_width,
            gap,
            self.config.min_tab_width_px as f32,
            self.config.max_tab_width_px as f32,
        );

        let mut tabs_row = Row::new().align_y(Alignment::Center).spacing(gap);
        for col in &columns {
            tabs_row = tabs_row.push(self.tab_view(col, font_size, tab_width));
        }

        let n = columns.len() as f32;
        let content_width = n * tab_width + gap * (n - 1.0).max(0.0);
        let max_offset = (content_width - strip_width).max(0.0);
        let current_offset = self
            .scroll_offsets
            .get(&workspace_id)
            .copied()
            .unwrap_or(0.0);

        let scrollable = Scrollable::new(tabs_row)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new().width(0.0).scroller_width(0.0),
            ))
            .width(Length::Fixed(strip_width))
            .id(self.scroll_id_for(workspace_id));

        // Edge fade: shown only on the side(s) there's actually more to
        // reveal, using our own tracked `current_offset` (the `Scrollable`
        // itself can't be read back -- see `scroll_focused_tabs_into_view`).
        // This sits *inside* the `MouseArea` below, wrapping only the
        // `Scrollable` -- wheel input is captured by that outer `MouseArea`
        // regardless of what's stacked on top of the strip, so the fade's
        // non-interactive overlay layers have nothing to intercept.
        let strip: Element<'a, Message> = if max_offset < 0.5 {
            scrollable.into()
        } else {
            edge_fade_stack(
                scrollable.into(),
                strip_width,
                self.config.tab_height_px,
                current_offset > 0.5,
                current_offset < max_offset - 0.5,
            )
        };

        // Wheel over the strip cycles niri's column focus, not a plain pan
        // -- a bare `Scrollable`'s own wheel handling does nothing when the
        // strip isn't overflowing (the common case), which reads as
        // "scroll doesn't work" for anyone with a handful of tabs. The
        // strip still visually scrolls when focus moves off-screen, just
        // driven by `scroll_focused_tabs_into_view` in `update()`, not by
        // the wheel event directly.
        let scroll = |dir: i32| {
            if dir < 0 {
                Message::ScrollLeft
            } else {
                Message::ScrollRight
            }
        };
        MouseArea::new(strip)
            .on_scroll(move |delta| match delta {
                iced::mouse::ScrollDelta::Lines { y, .. } => {
                    if y.is_sign_positive() {
                        scroll(1)
                    } else {
                        scroll(-1)
                    }
                }
                iced::mouse::ScrollDelta::Pixels { y, .. } => {
                    let sensibility = 3.0;
                    if y.abs() < sensibility {
                        Message::ScrollAccumulator(y)
                    } else if y.is_sign_positive() {
                        scroll(1)
                    } else {
                        scroll(-1)
                    }
                }
            })
            .into()
    }

    fn tab_view<'a>(
        &self,
        column: &Column<'_>,
        font_size: f32,
        target_width: f32,
    ) -> Element<'a, Message> {
        let window = column.window;
        let id = window.id;
        let is_focused = window.is_focused;
        let title = window
            .title
            .clone()
            .or_else(|| window.app_id.clone())
            .unwrap_or_else(|| id.to_string());
        let icon: Option<XdgIcon> =
            window.app_id.as_deref().map(|a| a.to_lowercase()).map(|a| {
                xdg_icons::get_icon_from_name(&a).unwrap_or_else(xdg_icons::fallback_icon)
            });
        let height = self.config.tab_height_px;
        let monochrome = self.config.monochrome_tab_icons;
        let animations_enabled = use_theme(|t| t.animations_enabled);

        let build = move |width: f32| {
            let title = title.clone();
            let icon = icon.clone();
            use_theme(move |theme| {
                tab_pill(
                    theme,
                    id,
                    title.clone(),
                    icon.clone(),
                    font_size,
                    is_focused,
                    monochrome,
                    Length::Fixed(width),
                    height,
                )
            })
        };

        let pill: Element<'a, Message> = if animations_enabled {
            // Spring-driven, not duration+easing: `Motion::SMOOTH` is
            // critically damped (no overshoot), `with_duration` sets its
            // response time. Unlike the old `Easing`-based animation this
            // replaces, a `Spring` retargets cleanly mid-flight (e.g.
            // opening/closing tabs in quick succession doesn't restart
            // from zero or fight a competing animation).
            AnimationBuilder::new(target_width, build)
                .animates_layout(true)
                .animation(Motion::SMOOTH.with_duration(TAB_ANIMATION))
                .into()
        } else {
            build(target_width)
        };

        MouseArea::new(pill)
            .on_double_click(Message::MaximizeWindow(id))
            .on_middle_press(Message::CloseWindow(id))
            .into()
    }
}

/// One tab's width when `n` tabs share `strip_width` equally, like a
/// browser's tab strip: shrink together as `n` grows, floor at `min`
/// (below which the strip scrolls instead of shrinking tabs further --
/// see `bloomed_view`'s `can_scroll`), capped at `max` so a single (or
/// nearly-empty) workspace doesn't stretch its one tab across the entire
/// strip -- a real regression a first pass of this had no upper bound on.
fn firefox_tab_width(n: usize, strip_width: f32, gap: f32, min: f32, max: f32) -> f32 {
    if n == 0 {
        return strip_width.min(max);
    }
    let n = n as f32;
    let share = (strip_width - gap * (n - 1.0).max(0.0)) / n;
    share.clamp(min, max)
}

/// Overlays a left and/or right gradient fade on `content` (a `strip_width`
/// x `height` `Scrollable`) -- the affordance for "there's more here,
/// scroll this way," gated on `show_left`/`show_right` so it only appears
/// on the side(s) actually still hidden. Uses the bar's own translucent
/// surface color (`palette.background`, which already carries the bar's
/// configured opacity -- see `theme::Paint::surface`'s doc comment) faded
/// to fully transparent, the same gradient-mask technique a real browser's
/// overflowed tab strip uses (not an actual blur).
///
/// Sits *inside* the `MouseArea` in `bloomed_view`, wrapping only the
/// `Scrollable` -- that outer `MouseArea` is what actually receives wheel
/// input (see its doc comment), so these purely-decorative, non-interactive
/// overlay layers have nothing to intercept.
fn edge_fade_stack<'a>(
    content: Element<'a, Message>,
    width: f32,
    height: f32,
    show_left: bool,
    show_right: bool,
) -> Element<'a, Message> {
    const FADE_WIDTH: f32 = 24.0;
    let bar_color = use_theme(|t| t.palette.background);

    let fade = |fade_to_transparent_on_the_right: bool| {
        let transparent = Color {
            a: 0.0,
            ..bar_color
        };
        let (start, end) = if fade_to_transparent_on_the_right {
            (bar_color, transparent)
        } else {
            (transparent, bar_color)
        };
        container(iced::widget::Space::new())
            .width(Length::Fixed(FADE_WIDTH))
            .height(Length::Fixed(height))
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(Background::Gradient(Gradient::Linear(
                    iced::gradient::Linear::new(std::f32::consts::FRAC_PI_2)
                        .add_stop(0.0, start)
                        .add_stop(1.0, end),
                ))),
                ..container::Style::default()
            })
    };

    let mut stack = iced::widget::Stack::new().push(content);
    if show_left {
        stack = stack.push(
            Row::new()
                .width(Length::Fixed(width))
                .push(fade(true))
                .push(iced::widget::Space::new().width(Length::Fill)),
        );
    }
    if show_right {
        stack = stack.push(
            Row::new()
                .width(Length::Fixed(width))
                .push(iced::widget::Space::new().width(Length::Fill))
                .push(fade(false)),
        );
    }
    stack.into()
}

/// Per the reference screenshot: every tab gets a faint gray outline, the
/// focused tab's outline is a brighter version of the *same* gray (no
/// hue, no accent color -- just more contrast), slightly more rounded
/// than a plain rectangle but nowhere near `workspace_button_style`'s
/// full-pill radius.
fn tab_button_style(
    is_focused: bool,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + use<> {
    let radius = 6.0;
    let light_gray = iced::Color::from_rgb8(200, 200, 200);
    move |theme: &iced::Theme, status: button::Status| {
        let ext = theme.extended_palette();
        let highlight = crate::theme::Paint::surface(theme, ext.background.base.text);
        let background = match status {
            button::Status::Hovered => Some(
                highlight
                    .scale_alpha(if is_focused { 0.16 } else { 0.08 })
                    .into(),
            ),
            _ if is_focused => Some(highlight.scale_alpha(0.12).into()),
            _ => None,
        };
        let border_alpha = if is_focused { 0.55 } else { 0.15 };
        button::Style {
            background,
            border: iced::Border {
                width: 0.6,
                radius: radius.into(),
                color: crate::theme::Paint::opaque(light_gray)
                    .scale_alpha(border_alpha)
                    .color(),
            },
            text_color: dim_unless_focused(theme.palette().text, is_focused),
            ..button::Style::default()
        }
    }
}

/// Focused tab text/icon stays at full contrast; every other tab is dimmed
/// to half-alpha, matching how `workspace_number`/`collapsed_view` already
/// mute everything but the active workspace.
fn dim_unless_focused(mut color: Color, is_focused: bool) -> Color {
    if !is_focused {
        color.a *= 0.35;
    }
    color
}

/// GTK's original Colonnade truncates a tab's title with Pango's real
/// font-metrics `EllipsizeMode::End`. iced has no equivalent (no text
/// measurement API available here), so this estimates a character budget
/// from an average glyph-advance heuristic for proportional Latin fonts
/// instead -- close enough for "shows `…` when it doesn't fit," which is
/// the actual ask, without pulling in a text-shaping dependency for it.
/// `.clip(true)` on the container in `tab_pill` stays as a safety net for
/// whenever this estimate undershoots.
fn truncate_title(title: &str, max_width_px: f32, font_size: f32) -> String {
    let avg_char_px = font_size * 0.55;
    let max_chars = (max_width_px / avg_char_px).floor().max(1.0) as usize;
    if title.chars().count() <= max_chars {
        return title.to_string();
    }
    let keep = max_chars.saturating_sub(1).max(1);
    let truncated: String = title.chars().take(keep).collect();
    format!("{truncated}…")
}

/// Builds one tab's visual: icon + title, ellipsized to (an estimate of)
/// `width` via `truncate_title` so a long title reads as "cut off" rather
/// than silently missing text, matching the original GTK widget's
/// Pango-ellipsized tabs.
#[allow(clippy::too_many_arguments)]
fn tab_pill<'a>(
    theme: &LumenTheme,
    id: u64,
    title: String,
    icon: Option<XdgIcon>,
    font_size: f32,
    is_focused: bool,
    monochrome: bool,
    width: Length,
    height: f32,
) -> Element<'a, Message> {
    let icon_size = font_size + 2.0;
    let has_icon = icon.is_some();
    let tint = dim_unless_focused(theme.palette.text, is_focused);
    let mut content = Row::new().align_y(Alignment::Center).spacing(4.0);
    // `monochrome` is currently a no-op for `XdgIcon::Svg`: iced's SVG
    // color filter recolors *every* pixel of the rendered raster to a
    // flat tint, which only looks right for a true symbolic/mask icon
    // (alpha-only, no opaque fill). Most XDG app icons (this one
    // included) have solid opaque artwork, so the filter just painted the
    // icon's whole bounding shape as a solid block -- reported as
    // "complete white rectangle" -- see lumen#18 for a real fix
    // (luminance-based masking, or falling back to symbolic icon lookups
    // where the icon theme actually provides them).
    let _ = monochrome;
    if let Some(icon) = icon {
        let icon_element: Element<'a, Message> = match icon {
            XdgIcon::Svg(handle) => Svg::new(handle)
                .height(Length::Fixed(icon_size))
                .width(Length::Shrink)
                .into(),
            XdgIcon::Image(handle) => Image::new(handle)
                .height(Length::Fixed(icon_size))
                .width(Length::Shrink)
                .into(),
            XdgIcon::NerdFont(si) => crate::components::icons::icon(si)
                .size(icon_size)
                .color(tint)
                .into(),
        };
        content = content.push(icon_element);
    }
    let icon_reserved = if has_icon { icon_size + 4.0 } else { 0.0 };
    let text_budget_px = match width {
        Length::Fixed(w) => (w - 12.0 /* horizontal padding */ - icon_reserved).max(0.0),
        _ => f32::MAX,
    };
    content = content.push(
        text(truncate_title(&title, text_budget_px, font_size))
            .size(font_size)
            .color(tint)
            .wrapping(iced::widget::text::Wrapping::None)
            .width(Length::Fill),
    );

    let style = tab_button_style(is_focused);
    // `button`'s own layout never centers leftover vertical space -- it
    // places content flush at the padded top-left (iced_core::layout::
    // padded's `position` step just offsets by `(padding.left,
    // padding.top)`), so top/bottom padding only matched by accident
    // whenever content height happened to equal `height` minus padding.
    // Filling the inner container to the button's full height and
    // centering inside *it* fixes that regardless of content height.
    button(
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::Center)
            .clip(true),
    )
    .padding([2.0, 6.0])
    .width(width)
    .height(Length::Fixed(height))
    .style(style)
    .on_press(Message::FocusWindow(id))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use colonnade_core::niri::{Snapshot, Window, WorkspaceInfo};

    /// Builds a real `colonnade_core::niri::Window` -- the same type the
    /// live niri event stream produces -- so the tests below can drive
    /// the actual public entry points rather than private helpers.
    fn window(id: u64, column: usize, tile_width: f64, workspace_id: u64) -> Window {
        let inner: niri_ipc::Window = serde_json::from_value(serde_json::json!({
            "id": id,
            "title": format!("window {id}"),
            "app_id": "test-app",
            "pid": null,
            "workspace_id": workspace_id,
            "is_focused": false,
            "is_floating": false,
            "is_urgent": false,
            "layout": {
                "pos_in_scrolling_layout": [column, 1],
                "tile_size": [tile_width, 1000.0],
                "window_size": [tile_width as i32, 1000],
                "tile_pos_in_workspace_view": [0.0, 0.0],
                "window_offset_in_tile": [0.0, 0.0],
            },
            "focus_timestamp": null,
        }))
        .expect("valid niri_ipc::Window");
        Window::new(inner, Some("test-output".to_string()))
    }

    fn workspace(id: u64, active_window_id: Option<u64>) -> WorkspaceInfo {
        WorkspaceInfo {
            id,
            idx: 1,
            name: None,
            output: Some("test-output".to_string()),
            is_active: true,
            is_focused: true,
            active_window_id,
        }
    }

    fn colonnade_with(windows: Vec<Window>, workspaces: Vec<WorkspaceInfo>) -> Colonnade {
        let mut c = Colonnade::new(ColonnadeModuleConfig::default());
        c.update(Message::Snapshot(Snapshot {
            windows,
            workspaces,
        }));
        c
    }

    /// One open tab reads as full-strip-width -- up to the cap. A strip
    /// wider than `max` must not stretch a single tab across all of it.
    #[test]
    fn a_single_tab_fills_the_strip_up_to_the_cap() {
        assert_eq!(firefox_tab_width(1, 700.0, 4.0, 40.0, 260.0), 260.0);
    }

    /// Firefox's actual behavior: tabs divide the strip evenly and shrink
    /// together as more open.
    #[test]
    fn tabs_share_the_strip_equally() {
        // 700px strip, 4px gaps, 3 tabs -> (700 - 2*4) / 3 = 230.667
        let width = firefox_tab_width(3, 700.0, 4.0, 40.0, 260.0);
        assert!(
            (width - 230.666_67).abs() < 0.01,
            "expected ~230.67, got {width}"
        );
    }

    /// Below the floor, stop shrinking further -- this is the point the
    /// strip should scroll instead of continuing to compress tabs into
    /// unreadable widths.
    #[test]
    fn width_never_drops_below_the_floor() {
        let width = firefox_tab_width(50, 700.0, 4.0, 40.0, 260.0);
        assert_eq!(width, 40.0);
    }

    /// Zero tabs is a real, non-panicking case (empty workspace) --
    /// `bloomed_view` short-circuits on it separately, but the pure
    /// function itself must not divide by zero either.
    #[test]
    fn zero_tabs_does_not_panic() {
        assert_eq!(firefox_tab_width(0, 700.0, 4.0, 40.0, 260.0), 260.0);
    }

    /// Exercises `bloomed_view` -- the real layout entry point -- with
    /// real `Window` values, confirming it produces an element without
    /// panicking for the shapes that matter: no tabs, one tab (full
    /// width), a couple of tabs, and enough tabs to floor out and need to
    /// scroll.
    #[test]
    fn bloomed_view_builds_for_representative_layouts() {
        for count in [0_u64, 1, 2, 30] {
            let ws = 1;
            let windows: Vec<Window> = (0..count)
                .map(|i| window(i + 1, (i as usize) + 1, 600.0, ws))
                .collect();
            let colonnade = colonnade_with(windows.clone(), vec![workspace(ws, Some(1))]);
            let _element = colonnade.bloomed_view(ws, &windows, 1920.0);
        }
    }
}
