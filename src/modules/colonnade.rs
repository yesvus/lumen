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
use iced_anim::{
    AnimationBuilder,
    transition::{Curve, Easing},
};
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
    /// window/workspace event stream (which carries neither).
    output_widths: HashMap<String, f64>,
    /// Last time `output_widths` was (re)fetched -- see
    /// `OUTPUT_FETCH_MIN_INTERVAL`.
    last_output_fetch: Option<std::time::Instant>,
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
    /// Per-workspace in-flight `box_width` transition (see `bloomed_view`)
    /// -- eased towards its new raw target over `TAB_ANIMATION` rather
    /// than snapping straight to it, so the tabs box's own width doesn't
    /// run ahead of the individual tabs still easing towards their new
    /// widths via `AnimationBuilder`. Before this, `box_width` was
    /// recomputed fresh (and applied instantly) every render while the
    /// tabs inside it took ~100ms to catch up, which is what produced the
    /// "box already at its new size, last visible tab still mid-animation
    /// and narrower than it should be, gap before the overflow dashes"
    /// look -- see lumen#23. `AnimationBuilder` isn't used here for the
    /// same effect because nesting it inside each tab's own
    /// `AnimationBuilder` hits iced_anim's documented nested-animation
    /// limitation (the inner property would skip straight to its final
    /// value instead of animating), so this rolls its own easing instead.
    box_widths: std::cell::RefCell<HashMap<u64, BoxWidthTransition>>,
}

/// One workspace's in-flight `box_width` easing -- see
/// `Colonnade::box_widths` and `Colonnade::smoothed_box_width`.
#[derive(Clone, Copy)]
struct BoxWidthTransition {
    /// Width this transition started from.
    from: f32,
    /// Width it is easing towards.
    to: f32,
    /// When it started, for the time-based progress in
    /// `smoothed_box_width`.
    started: std::time::Instant,
}

/// How long a tab's width `AnimationBuilder` takes (`Easing::very_quick`,
/// see `tab_view`). `box_width`'s own easing deliberately shares it: the
/// box has no animation clock of its own and is only advanced when
/// something *else* schedules a redraw, and during a resize the only
/// thing doing that is the tabs' own `AnimationBuilder`. Finishing in the
/// same window means the box lands exactly when the last redraw arrives,
/// instead of being stranded partway -- see `smoothed_box_width`.
const TAB_ANIMATION: std::time::Duration = std::time::Duration::from_millis(100);

/// Below this, a change in a column's `target_width_px` between snapshots
/// is treated as niri's own geometry-rounding noise rather than a real
/// resize worth re-animating towards -- see lumen#23.
const WIDTH_STABILITY_THRESHOLD_PX: f32 = 3.0;

/// `transition`'s current width at `now`: its `from`/`to` endpoints
/// interpolated by elapsed-time progress through `TAB_ANIMATION`, shaped
/// by literally the same `Curve::Ease` the tabs' `Easing::EASE` uses, so
/// the box and its contents move on matching velocity curves rather than
/// one gliding while the other moves linearly. Calling into iced_anim's
/// own curve rather than approximating it in closed form keeps the two
/// exact, including if the crate ever retunes its control points.
/// Progress saturates at 1.0, so once the window has passed this returns
/// exactly `to` -- the box always lands precisely on target no matter
/// when it is next rendered.
fn eased(transition: &BoxWidthTransition, now: std::time::Instant) -> f32 {
    let elapsed = now.duration_since(transition.started).as_secs_f32();
    let progress = (elapsed / TAB_ANIMATION.as_secs_f32()).clamp(0.0, 1.0);
    let t = Curve::Ease.value(progress);
    transition.from + (transition.to - transition.from) * t
}

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
            anchors: std::cell::RefCell::new(HashMap::new()),
            scroll_accumulator: 0.0,
            stable_widths: std::cell::RefCell::new(HashMap::new()),
            box_widths: std::cell::RefCell::new(HashMap::new()),
        }
    }

    /// Eases the box width for `workspace_id` towards `raw_target` over
    /// `TAB_ANIMATION`, as a function of *elapsed time* rather than of how
    /// many times this happened to be called -- see `Self::box_widths` /
    /// lumen#23.
    ///
    /// The previous version moved a fixed 35% of the remaining distance
    /// per render, which is what left the "shrunk last tab + gap" visible
    /// *after* a resize settled rather than only during it. Two things
    /// compounded:
    ///
    /// 1. Per-render decay only converges as fast as renders arrive, and
    ///    nothing here drives renders on its own. During a resize the only
    ///    thing requesting them is each tab's `AnimationBuilder`, which
    ///    stops as soon as its own 100ms easing completes
    ///    (`is_animating()` gates `request_redraw`). 35% per render needs
    ///    ~12 renders to close a 100px delta to within half a pixel, but
    ///    the tabs stop asking for redraws after ~6 -- stranding the box
    ///    ~7px short of its target, with no further render scheduled to
    ///    finish the job. That leftover is exactly the phantom gap: the box
    ///    is `.clip(true)`, so a box narrower than its content visibly cuts
    ///    the last tab short.
    /// 2. Being render-counted rather than timed also made the speed
    ///    depend on frame rate, so the same resize settled differently
    ///    under load than when idle.
    ///
    /// Easing on a real clock fixes both: progress depends only on elapsed
    /// time, so it reaches exactly 1.0 after `TAB_ANIMATION` regardless of
    /// how many renders happened to land in between, and it shares the
    /// tabs' own duration so the box lands on the same frame they do.
    fn smoothed_box_width(&self, workspace_id: u64, raw_target: f32) -> f32 {
        let mut widths = self.box_widths.borrow_mut();
        let now = std::time::Instant::now();
        let transition = widths.entry(workspace_id).or_insert(BoxWidthTransition {
            from: raw_target,
            to: raw_target,
            started: now,
        });

        // Sub-pixel target drift isn't worth restarting the easing for --
        // same reasoning as `WIDTH_STABILITY_THRESHOLD_PX` for tabs, and
        // without it a target wobbling by rounding noise would keep
        // resetting `started` and never reach progress 1.0.
        if (raw_target - transition.to).abs() > 0.5 {
            transition.from = eased(transition, now);
            transition.to = raw_target;
            transition.started = now;
        }

        eased(transition, now)
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

    /// Drops cached per-window and per-workspace animation state for
    /// windows/workspaces that no longer exist in the latest snapshot.
    ///
    /// `stable_widths` is keyed by niri window id, `box_widths`/`anchors`
    /// by workspace id, and all three only ever inserted -- so without
    /// this every window and workspace ever *seen* kept an entry for the
    /// life of the process. niri hands out monotonically increasing ids
    /// and never reuses them, so these maps grew unboundedly across a
    /// long uptime: on a bar that runs for days, every terminal, browser
    /// tab-tearoff and short-lived dialog leaked an entry apiece, with
    /// nothing to ever reclaim them.
    ///
    /// Pruning against the snapshot is safe because a window absent from
    /// it is closed (or moved to another output), and if it ever comes
    /// back it simply re-seeds from its current width -- the same thing
    /// that happens the first time a window is seen.
    fn forget_stale_cache_entries(&mut self) {
        let live_windows: std::collections::HashSet<u64> =
            self.snapshot.windows.iter().map(|w| w.id).collect();
        let live_workspaces: std::collections::HashSet<u64> =
            self.snapshot.workspaces.iter().map(|ws| ws.id).collect();

        self.stable_widths
            .borrow_mut()
            .retain(|id, _| live_windows.contains(id));
        self.box_widths
            .borrow_mut()
            .retain(|id, _| live_workspaces.contains(id));
        self.anchors
            .borrow_mut()
            .retain(|id, _| live_workspaces.contains(id));
    }

    pub fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            Message::Snapshot(snapshot) => {
                self.snapshot = snapshot;
                self.forget_stale_cache_entries();
                let due = self
                    .last_output_fetch
                    .is_none_or(|t| t.elapsed() >= OUTPUT_FETCH_MIN_INTERVAL);
                if !due {
                    return iced::Task::none();
                }
                self.last_output_fetch = Some(std::time::Instant::now());
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
                // Same duration `smoothed_box_width` eases the containing
                // box over -- see `TAB_ANIMATION`. Kept as one constant so
                // the two can't drift apart: a box that finishes later than
                // its tabs is stranded mid-easing when the tabs stop
                // driving redraws (lumen#23).
                .animation(Easing::EASE.with_duration(TAB_ANIMATION))
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

#[cfg(test)]
mod tests {
    use super::*;
    use colonnade_core::niri::{Snapshot, Window, WorkspaceInfo};
    use std::time::{Duration, Instant};

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

    /// The regression behind lumen#23's leftover gap: the box width must
    /// land *exactly* on target within `TAB_ANIMATION`, because the tabs'
    /// own `AnimationBuilder` stops driving redraws at that point and
    /// nothing else will schedule the frame that would finish the job.
    /// The old per-render 35% decay was still ~7px short here.
    #[test]
    fn box_width_lands_exactly_on_target_within_the_animation_window() {
        let start = Instant::now();
        let transition = BoxWidthTransition {
            from: 100.0,
            to: 200.0,
            started: start,
        };

        assert_eq!(eased(&transition, start), 100.0, "starts at `from`");

        let at_end = eased(&transition, start + TAB_ANIMATION);
        assert_eq!(at_end, 200.0, "must land exactly on target, not near it");

        // And stays there rather than drifting once the window has passed.
        let after = eased(&transition, start + TAB_ANIMATION * 3);
        assert_eq!(after, 200.0);
    }

    /// Progress must depend on elapsed time only, so a slow frame or a
    /// missed render can't change where the box ends up -- the old
    /// render-counted decay made the settle speed frame-rate dependent.
    #[test]
    fn box_width_is_time_based_not_render_count_based() {
        let start = Instant::now();
        let transition = BoxWidthTransition {
            from: 0.0,
            to: 100.0,
            started: start,
        };

        let halfway = start + TAB_ANIMATION / 2;
        // Sampling repeatedly at the same instant must be idempotent: the
        // value is a function of the clock, not of how often it's polled.
        let a = eased(&transition, halfway);
        let b = eased(&transition, halfway);
        assert_eq!(a, b);
        assert!(a > 0.0 && a < 100.0, "mid-flight, got {a}");
    }

    /// Monotonic and bounded: the box must never overshoot its target and
    /// then come back, which would read as a visible wobble at the end of
    /// a resize.
    #[test]
    fn box_width_never_overshoots() {
        let start = Instant::now();
        let transition = BoxWidthTransition {
            from: 50.0,
            to: 150.0,
            started: start,
        };

        let mut previous = f32::MIN;
        for step in 0..=20 {
            let now = start + Duration::from_millis(step * 10);
            let value = eased(&transition, now);
            assert!(
                (50.0..=150.0).contains(&value),
                "out of bounds at step {step}: {value}"
            );
            assert!(value >= previous, "went backwards at step {step}");
            previous = value;
        }
    }

    /// Drives the real public entry point (`Colonnade::update` with a
    /// `Message::Snapshot`, exactly as the live niri subscription does)
    /// and asserts the per-window/per-workspace caches don't accumulate
    /// entries for windows that have since closed.
    ///
    /// Before `forget_stale_cache_entries`, these maps were insert-only.
    /// niri hands out monotonically increasing window ids and never
    /// reuses them, so on a bar running for days every terminal, dialog
    /// and short-lived window leaked an entry that nothing ever
    /// reclaimed.
    #[test]
    fn closed_windows_do_not_accumulate_in_the_width_cache() {
        let ws = 1;
        let mut colonnade = colonnade_with(
            vec![window(10, 1, 600.0, ws), window(11, 2, 600.0, ws)],
            vec![workspace(ws, Some(10))],
        );

        // Seed the width cache the way a render does.
        colonnade.stable_width(10, 300.0);
        colonnade.stable_width(11, 300.0);
        assert_eq!(colonnade.stable_widths.borrow().len(), 2);

        // Churn: those two windows close, many new ones open over time.
        for id in 12..40 {
            colonnade.update(Message::Snapshot(Snapshot {
                windows: vec![window(id, 1, 600.0, ws)],
                workspaces: vec![workspace(ws, Some(id))],
            }));
            colonnade.stable_width(id, 300.0);
        }

        let cached = colonnade.stable_widths.borrow().len();
        assert!(
            cached <= 2,
            "width cache grew to {cached} entries across 28 window \
             lifetimes; it must only retain currently-live windows"
        );
        assert!(
            !colonnade.stable_widths.borrow().contains_key(&10),
            "closed window 10 still cached"
        );
    }

    /// Same, for the per-workspace caches keyed by workspace id.
    #[test]
    fn removed_workspaces_do_not_accumulate() {
        let mut colonnade = colonnade_with(
            vec![window(1, 1, 600.0, 100)],
            vec![workspace(100, Some(1))],
        );
        colonnade.smoothed_box_width(100, 250.0);
        assert_eq!(colonnade.box_widths.borrow().len(), 1);

        for ws in 101..120 {
            colonnade.update(Message::Snapshot(Snapshot {
                windows: vec![window(1, 1, 600.0, ws)],
                workspaces: vec![workspace(ws, Some(1))],
            }));
            colonnade.smoothed_box_width(ws, 250.0);
        }

        let cached = colonnade.box_widths.borrow().len();
        assert!(
            cached <= 1,
            "box-width cache grew to {cached} entries across 19 workspaces"
        );
        assert!(
            !colonnade.box_widths.borrow().contains_key(&100),
            "stale workspace 100 still cached"
        );
    }

    /// A window that closes and later reappears must re-seed cleanly from
    /// its current width rather than resurrecting a stale cached one --
    /// the correctness condition that makes pruning safe.
    #[test]
    fn a_returning_window_reseeds_at_its_current_width() {
        let ws = 1;
        let mut colonnade =
            colonnade_with(vec![window(7, 1, 600.0, ws)], vec![workspace(ws, Some(7))]);
        assert_eq!(colonnade.stable_width(7, 300.0), 300.0);

        // Window closes.
        colonnade.update(Message::Snapshot(Snapshot {
            windows: vec![],
            workspaces: vec![workspace(ws, None)],
        }));

        assert!(
            !colonnade.stable_widths.borrow().contains_key(&7),
            "cache entry must be dropped while the window is gone"
        );

        // Comes back at a width within the noise threshold of the old
        // cached one. This is the case that distinguishes a real re-seed
        // from a survivor: had the stale 300.0 persisted, the sub-
        // threshold debounce would pin the width at 300.0 forever and the
        // tab would render at the wrong size.
        colonnade.update(Message::Snapshot(Snapshot {
            windows: vec![window(7, 1, 200.0, ws)],
            workspaces: vec![workspace(ws, Some(7))],
        }));
        assert_eq!(
            colonnade.stable_width(7, 298.0),
            298.0,
            "returning window must re-seed at its real current width, not \
             stay pinned to a stale cached value by the noise threshold"
        );
    }

    /// Exercises `bloomed_view` -- the real layout entry point that
    /// computes `box_width` and the visible slice -- with real `Window`
    /// values, confirming it produces an element without panicking for
    /// the shapes that previously misbehaved (single tab, many tabs
    /// overflowing the budget, and an empty workspace).
    #[test]
    fn bloomed_view_builds_for_representative_layouts() {
        for count in [0_u64, 1, 2, 12] {
            let ws = 1;
            let windows: Vec<Window> = (0..count)
                .map(|i| window(i + 1, (i as usize) + 1, 600.0, ws))
                .collect();
            let colonnade = colonnade_with(windows.clone(), vec![workspace(ws, Some(1))]);
            let _element = colonnade.bloomed_view(&workspace(ws, Some(1)), &windows, 1920.0);
        }
    }

    /// Shrinking must behave symmetrically -- the gap bug showed up on
    /// collapse, not just growth.
    #[test]
    fn box_width_lands_exactly_when_shrinking() {
        let start = Instant::now();
        let transition = BoxWidthTransition {
            from: 700.0,
            to: 120.0,
            started: start,
        };
        assert_eq!(eased(&transition, start + TAB_ANIMATION), 120.0);
    }
}
