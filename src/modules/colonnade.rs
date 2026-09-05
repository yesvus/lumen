//! Column-grouped, niri-aware tab strip -- a native port of Colonnade
//! (github.com/yesvus/colonnade, MIT) onto its own toolkit-independent
//! `colonnade-core` crate. See that repo's BEHAVIOR.md for the
//! interaction spec this implements: fused workspace markers + tabs, one
//! tab per niri column, single/double/middle click semantics, and a
//! pixel-budget visible slice with overflow glyph ticks.
//!
//! Holds its own `colonnade_core::niri::Niri` client and event stream,
//! independent of Lumen's own multi-compositor `services::compositor`
//! abstraction -- niri columns have no equivalent in that model, and
//! Colonnade is inherently niri-only, so there is nothing to share there.

use std::collections::HashMap;

use colonnade_core::{
    column::{self, Column},
    glyph,
    niri::{Niri, Snapshot, Window, WorkspaceInfo},
    slice,
};
use iced::{
    Alignment, Color, Element, Length, Subscription, SurfaceId,
    widget::{Image, MouseArea, Row, Space, Svg, button, container, text},
};
use iced_anim::{AnimationBuilder, transition::Easing};
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
    FocusWorkspace(u64),
    ScrollLeft,
    ScrollRight,
    ScrollAccumulator(f32),
    ConfigReloaded(ColonnadeModuleConfig),
}

pub struct Colonnade {
    config: ColonnadeModuleConfig,
    niri: Niri,
    snapshot: Snapshot,
    /// Logical width per niri output name -- fetched separately from the
    /// window/workspace event stream (which carries neither), refreshed
    /// on each snapshot.
    output_widths: HashMap<String, f64>,
    /// The visible slice's left anchor per bloomed workspace -- see
    /// `colonnade_core::slice`'s doc comment on why this persists across
    /// renders instead of being recomputed from scratch each time. A
    /// `RefCell` because `view()` takes `&self` but still needs to write
    /// the anchor `slice::compute` just handed back, for the next render.
    anchors: std::cell::RefCell<HashMap<u64, u64>>,
    /// Trackpad smooth-scroll accumulator, same threshold-based debounce
    /// `workspaces.rs` uses for its own scroll handling.
    scroll_accumulator: f32,
    /// Per-window last-committed tab width, fed to each tab's
    /// `AnimationBuilder` instead of the raw `target_width_px` niri hands
    /// back on every `Snapshot`. niri's own per-column width fraction
    /// fluctuates by a few px between consecutive snapshots even when
    /// nothing meaningfully changed (geometry rounding noise), so feeding
    /// the raw value straight into the animation handed it a new target
    /// virtually every frame and it never actually converged -- see
    /// lumen#23. Only updated when the real value moves by more than
    /// `WIDTH_STABILITY_THRESHOLD_PX`, so sub-threshold noise is ignored
    /// and the animation settles like the tree-position anchors above.
    stable_widths: std::cell::RefCell<HashMap<u64, f32>>,
    /// Per-workspace last-rendered `box_width` (see `bloomed_view`) --
    /// smoothed towards its new raw target each render rather than
    /// snapping straight to it, so the tabs box's own width doesn't run
    /// ahead of the individual tabs still easing towards their new widths
    /// via `AnimationBuilder`. Before this, `box_width` was recomputed
    /// fresh (and applied instantly) every render while the tabs inside it
    /// took ~100ms to catch up, which is what produced the "box already at
    /// its new size, last visible tab still mid-animation and narrower
    /// than it should be, gap before the overflow dashes" look -- see
    /// lumen#23. `AnimationBuilder` isn't used here for the same effect
    /// because nesting it inside each tab's own `AnimationBuilder` hits
    /// iced_anim's documented nested-animation limitation (the inner
    /// property would skip straight to its final value instead of
    /// animating), so this exponential-smoothing rolls its own instead.
    box_widths: std::cell::RefCell<HashMap<u64, f32>>,
}

/// Below this, a change in a column's `target_width_px` between snapshots
/// is treated as niri's own geometry-rounding noise rather than a real
/// resize worth re-animating towards -- see lumen#23.
const WIDTH_STABILITY_THRESHOLD_PX: f32 = 3.0;

impl Colonnade {
    pub fn new(config: ColonnadeModuleConfig) -> Self {
        Self {
            config,
            niri: Niri::new(),
            snapshot: Snapshot::default(),
            output_widths: HashMap::new(),
            anchors: std::cell::RefCell::new(HashMap::new()),
            scroll_accumulator: 0.0,
            stable_widths: std::cell::RefCell::new(HashMap::new()),
            box_widths: std::cell::RefCell::new(HashMap::new()),
        }
    }

    /// Moves the cached box width for `workspace_id` a fraction of the way
    /// towards `raw_target` each call rather than snapping straight to it
    /// -- see `Self::box_widths`'s doc comment / lumen#23. Snaps once
    /// within half a pixel so it actually settles instead of approaching
    /// forever.
    fn smoothed_box_width(&self, workspace_id: u64, raw_target: f32) -> f32 {
        let mut widths = self.box_widths.borrow_mut();
        let current = widths.entry(workspace_id).or_insert(raw_target);
        *current += (raw_target - *current) * 0.35;
        if (*current - raw_target).abs() < 0.5 {
            *current = raw_target;
        }
        *current
    }

    /// Debounces `raw_width_px` against the last committed width for
    /// `window_id` (see `Self::stable_widths`'s doc comment / lumen#23): a
    /// change smaller than `WIDTH_STABILITY_THRESHOLD_PX` is ignored so
    /// per-snapshot rounding noise doesn't perpetually restart the tab's
    /// width animation.
    fn stable_width(&self, window_id: u64, raw_width_px: f32) -> f32 {
        let mut widths = self.stable_widths.borrow_mut();
        let stable = widths.entry(window_id).or_insert(raw_width_px);
        if (raw_width_px - *stable).abs() > WIDTH_STABILITY_THRESHOLD_PX {
            *stable = raw_width_px;
        }
        *stable
    }

    pub fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            Message::Snapshot(snapshot) => {
                self.snapshot = snapshot;
                let niri = self.niri;
                return iced::Task::perform(
                    async move { tokio::task::spawn_blocking(move || niri.outputs()).await },
                    |result| match result {
                        Ok(Ok(outputs)) => Message::OutputWidths(
                            outputs
                                .into_iter()
                                .map(|(name, output)| {
                                    let width =
                                        output.logical.map(|l| l.width as f64).unwrap_or_default();
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
                );
            }
            Message::OutputWidths(widths) => {
                if !widths.is_empty() {
                    self.output_widths = widths;
                }
            }
            Message::FocusWindow(id) => self.run(move |niri| niri.activate_window(id)),
            Message::MaximizeWindow(id) => self.run(move |niri| niri.maximize_window(id)),
            Message::CloseWindow(id) => self.run(move |niri| niri.close_window(id)),
            Message::FocusWorkspace(id) => self.run(move |niri| niri.focus_workspace(id)),
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

        let workspaces: Vec<&WorkspaceInfo> = self
            .snapshot
            .workspaces
            .iter()
            .filter(|ws| monitor_name.is_none_or(|n| ws.output.as_deref() == Some(n)))
            .collect();

        let bloomed_id = workspaces.iter().find(|ws| ws.is_active).map(|ws| ws.id);

        let space = use_theme(|t| t.space);
        // Same `space.xxs` gap `bloomed_view` uses between the tab group
        // and its overflow-glyph text, so the workspace-number-to-tab gap
        // and the last-tab-to-dashes gap read as one consistent rhythm
        // instead of two different values (this used to be `space.xs`,
        // visibly looser than the `xxs` gaps inside the group).
        let mut row = Row::new().align_y(Alignment::Center).spacing(space.xxs);

        for ws in &workspaces {
            let ws_windows: Vec<Window> = self
                .snapshot
                .windows
                .iter()
                .filter(|w| w.workspace_id == Some(ws.id))
                .cloned()
                .collect();
            let is_bloomed = Some(ws.id) == bloomed_id;

            // Empty + not bloomed: hidden entirely (BEHAVIOR.md).
            if ws_windows.is_empty() && !is_bloomed {
                continue;
            }

            row = row.push(self.workspace_number(ws, is_bloomed));

            if is_bloomed {
                row = row.push(self.bloomed_view(ws, &ws_windows, output_width));
            } else {
                row = row.push(self.collapsed_view(ws, &ws_windows));
            }
        }

        row.into()
    }

    fn workspace_number<'a>(
        &self,
        workspace: &WorkspaceInfo,
        focused: bool,
    ) -> Element<'a, Message> {
        let label = workspace
            .name
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| workspace.idx.to_string());
        let font_size = use_theme(|t| t.font_size.sm);
        let color = use_theme(|t| {
            if focused {
                t.palette.text
            } else {
                let mut c = t.palette.text;
                c.a *= 0.5;
                c
            }
        });
        let id = workspace.id;
        MouseArea::new(text(label).size(font_size).color(color))
            .on_press(Message::FocusWorkspace(id))
            .into()
    }

    fn collapsed_view<'a>(
        &self,
        workspace: &WorkspaceInfo,
        windows: &[Window],
    ) -> Element<'a, Message> {
        let glyphs = glyph::marker_text(workspace, windows, self.config.max_overflow_glyphs);
        let glyphs = if glyphs.is_empty() {
            "\u{b7}".to_string()
        } else {
            glyphs
        };
        let font_size = use_theme(|t| t.font_size.sm);
        let mut color = use_theme(|t| t.palette.text);
        color.a *= 0.5;
        let id = workspace.id;
        // Forced to a monospace font: block-drawing glyphs (`\u{2588}` FULL
        // BLOCK vs. `|`/`\u{258C}`/`\u{A6}`) are only guaranteed to fill
        // their advance width in fonts that account for box-drawing metrics
        // deliberately. Left to the proportional UI font, the full block
        // rendered left-leaning/off-center relative to its thinner
        // siblings -- see lumen#25.
        MouseArea::new(
            text(glyphs)
                .font(iced::Font::MONOSPACE)
                .size(font_size)
                .color(color),
        )
        .on_press(Message::FocusWorkspace(id))
        .into()
    }

    fn bloomed_view<'a>(
        &self,
        workspace: &WorkspaceInfo,
        windows: &[Window],
        output_width: f64,
    ) -> Element<'a, Message> {
        let space = use_theme(|t| t.space);
        let fixed_width = self.config.max_group_width_px as f32;
        let columns = column::group(
            windows,
            output_width,
            self.config.tab_width_scale_px,
            self.config.min_tab_width_px,
            self.config.dynamic_tab_width,
        );

        if columns.is_empty() {
            return container(Row::new())
                .width(Length::Fixed(fixed_width))
                .into();
        }

        let current_idx = columns
            .iter()
            .position(|c| c.window.is_focused || Some(c.window.id) == workspace.active_window_id)
            .unwrap_or(0);

        let slice::BloomedSlice { start, end, anchor } = slice::compute(
            &columns,
            current_idx,
            self.anchors.borrow().get(&workspace.id).copied(),
            self.config.max_group_width_px,
        );
        self.anchors.borrow_mut().insert(workspace.id, anchor);

        let font_size = use_theme(|t| t.font_size.sm);
        let mut dim = use_theme(|t| t.palette.text);
        dim.a *= 0.5;

        // Not `glyph::capped`: that keeps the *first* glyphs and trails
        // `…` at the end, which reads right for the right-side overflow
        // (nearest-to-visible glyphs first, `…` trailing off further
        // away) but is backwards on the left -- it put `…` right next to
        // the visible tabs and pushed the far-away glyphs to the outer
        // edge. The left side wants the mirror: keep the glyphs *nearest*
        // the visible slice and lead with `…` at the far edge, e.g.
        // `…|||||` rather than `|||||…`. See lumen#23 follow-up.
        let left_text = capped_from_end(
            columns[..start]
                .iter()
                .map(|c| glyph::glyph_for(workspace, c.window)),
            self.config.max_overflow_glyphs,
        );
        let right_text = glyph::capped(
            columns[end..]
                .iter()
                .map(|c| glyph::glyph_for(workspace, c.window)),
            self.config.max_overflow_glyphs,
        );

        // Every column stays in the tree, in stable order, across every
        // render -- only the *target* width fed to each tab's
        // AnimationBuilder toggles between its real width and 0 depending
        // on whether the visible-slice window currently covers it. That
        // way a tab crossing the slice boundary eases its width down to
        // nothing (and its sibling eases up to fill the gap) instead of
        // being removed from the widget tree outright, which is what was
        // producing the instant, chunky pop in and out of view.
        //
        // No `Row::spacing()` here on purpose (unlike the left/right
        // overflow row below, which has none to worry about): `spacing`
        // inserts a fixed gap between *every* pair of children regardless
        // of their own width, including the collapsed (0-width) columns
        // outside the visible slice. With many columns collapsed, all
        // those un-rendered gaps still added up to real width that
        // `box_width` below (sized only for the *visible* tabs' widths and
        // gaps) never accounted for -- silently pushing the visible tabs
        // rightward until `tabs_box`'s `.clip(true)` cut off part of the
        // last one. A plain `Space` between each pair of columns instead,
        // sized to the gap only when *both* neighbours are actually
        // visible, keeps collapsed columns truly zero-width including
        // their spacing -- see lumen#23.
        let mut tabs_row = Row::new().align_y(Alignment::Center);
        let last_idx = columns.len().saturating_sub(1);
        for (i, col) in columns.iter().enumerate() {
            let is_visible = i >= start && i < end;
            let target_width = if is_visible {
                self.stable_width(col.window.id, col.target_width_px as f32)
            } else {
                0.0
            };
            tabs_row = tabs_row.push(self.tab_view(col, font_size, target_width));
            if i != last_idx {
                let gap = if is_visible && i + 1 < end {
                    space.xxs
                } else {
                    0.0
                };
                tabs_row = tabs_row.push(Space::new().width(Length::Fixed(gap)));
            }
        }

        // Sized to the *visible slice's* own content, not the full
        // `max_group_width_px` budget every time: that budget is a cap on
        // how wide the box is allowed to grow, not a promise that it's
        // always that wide. Reserving the full budget regardless of
        // content left a huge empty gap after a short slice (one short
        // tab in a 700px box) and, once the overflow glyphs moved outside
        // this box, put that same empty gap between the last visible tab
        // and the glyphs. Recomputed from each column's *target* width
        // (not its live animated width), so the box itself doesn't resize
        // continuously mid-animation -- only in the discrete steps where
        // the visible slice itself changes.
        //
        // Not clamped to `fixed_width`: `slice::compute`'s own budget math
        // (`expand_right` in colonnade-core) only sums raw tab widths, not
        // the inter-tab spacing added here, so a full slice's real content
        // width is `fixed_width` plus a bit of spacing. Clamping down to
        // exactly `fixed_width` was silently clipping that last bit off
        // the rightmost visible tab whenever the slice was full.
        let visible_count = end - start;
        let raw_box_width: f32 = columns[start..end]
            .iter()
            .map(|c| self.stable_width(c.window.id, c.target_width_px as f32))
            .sum::<f32>()
            + space.xxs * visible_count.saturating_sub(1) as f32;
        let box_width = if use_theme(|t| t.animations_enabled) {
            self.smoothed_box_width(workspace.id, raw_box_width)
        } else {
            raw_box_width
        };

        let scroll = |dir: i32| {
            if dir < 0 {
                Message::ScrollLeft
            } else {
                Message::ScrollRight
            }
        };
        let tabs_box = MouseArea::new(
            container(tabs_row)
                .width(Length::Fixed(box_width))
                .clip(true),
        )
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
        });

        // `left_text`/`right_text` are always pushed, even when empty,
        // rather than conditionally -- an empty `text` collapses to zero
        // width so it's visually inert, but conditionally including it
        // would shift every sibling after it by one tree position exactly
        // when overflow appears/disappears. iced's widget-tree diffing
        // matches children positionally per parent, so that shift was
        // silently discarding (and restarting from scratch) the entire
        // tabs_box subtree's internal state -- including every tab's
        // in-flight AnimationBuilder progress -- which is what made the
        // width-collapse animation look inconsistent (fine most renders,
        // reset mid-transition whenever the left overflow indicator's
        // presence toggled).
        //
        // No `Row::spacing()` here on purpose: a uniform gap would add a
        // visible space before/after an empty (invisible) overflow text
        // too, widening the gap to the workspace number whenever there's
        // nothing to show on the left. The gap is applied as padding on
        // each text element instead, so it only exists when that element
        // actually has something in it.
        let left_gap = if left_text.is_empty() { 0.0 } else { space.xxs };
        let right_gap = if right_text.is_empty() {
            0.0
        } else {
            space.xxs
        };
        Row::new()
            .align_y(Alignment::Center)
            .push(
                // Monospace for the same box-drawing-metrics reason as
                // `collapsed_view`'s marker text -- see lumen#25.
                container(
                    text(left_text)
                        .font(iced::Font::MONOSPACE)
                        .size(font_size)
                        .color(dim),
                )
                .padding(iced::Padding {
                    right: left_gap,
                    ..iced::Padding::ZERO
                }),
            )
            .push(tabs_box)
            .push(
                container(
                    text(right_text)
                        .font(iced::Font::MONOSPACE)
                        .size(font_size)
                        .color(dim),
                )
                .padding(iced::Padding {
                    left: right_gap,
                    ..iced::Padding::ZERO
                }),
            )
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
            AnimationBuilder::new(target_width, build)
                .animates_layout(true)
                .animation(Easing::EASE.very_quick())
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

/// Mirror of `colonnade_core::glyph::capped` for the left-side overflow
/// indicator: keeps the glyphs nearest the visible tab slice (the *last*
/// `max` of them, since `glyphs` runs left-to-right away from the slice)
/// and leads with `…` for the far-away remainder, instead of `capped`'s
/// trailing `…`. See its call site in `bloomed_view` / lumen#23 follow-up.
fn capped_from_end(glyphs: impl Iterator<Item = char>, max: usize) -> String {
    let glyphs: Vec<char> = glyphs.collect();
    if glyphs.len() <= max {
        glyphs.into_iter().collect()
    } else {
        let keep = max.saturating_sub(1);
        let tail_start = glyphs.len() - keep;
        format!("…{}", glyphs[tail_start..].iter().collect::<String>())
    }
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
