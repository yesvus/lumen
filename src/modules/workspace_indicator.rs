//! The active workspace's number, e.g. "3 ›" -- deliberately its own
//! module, separate from `Colonnade`'s tab strip. Kept apart so:
//!
//! - the bar's own module-gap layout (`modules_section`, `module_item.rs`)
//!   handles the spacing between this and `Colonnade` the same way it
//!   spaces every other pair of modules, instead of either module
//!   hand-tuning a gap that's really the bar's concern;
//! - editing one doesn't force touching (or rebuilding confidence in) the
//!   other -- they don't share state, a struct, or a match arm beyond the
//!   `ModuleName` dispatch every module goes through.
//!
//! Holds its own niri client and snapshot subscription, same
//! self-contained pattern `Colonnade` uses (see its module doc comment for
//! why nothing is shared between niri-native modules here) -- this is not
//! `modules::workspaces`, which is ashell's generic multi-compositor
//! workspace pager; this one is niri-only and only ever shows the single
//! active workspace's number, not a full pager.

use colonnade_core::niri::{Niri, Snapshot, WorkspaceInfo};
use iced::{
    Element, Length, Subscription, SurfaceId,
    widget::{Space, text},
};
use log::warn;

use crate::{outputs::Outputs, theme::use_theme, utils};

#[derive(Debug, Clone)]
pub enum Message {
    Snapshot(Snapshot),
    /// Click: opens niri's overview (`niri msg action toggle-overview`),
    /// the actual "show me the workspaces" action -- `FocusWorkspace` on
    /// the already-active workspace would be a no-op.
    OpenOverview,
    /// Mouse wheel over the indicator: switch to the previous/next
    /// workspace on this output (sign of the `i32` is direction).
    Scroll(i32),
}

pub struct WorkspaceIndicator {
    niri: Niri,
    snapshot: Snapshot,
}

impl Default for WorkspaceIndicator {
    fn default() -> Self {
        Self {
            niri: Niri::new(),
            snapshot: Snapshot::default(),
        }
    }
}

impl WorkspaceIndicator {
    pub fn new() -> Self {
        Self::default()
    }

    fn active(&self, output: Option<&str>) -> Option<&WorkspaceInfo> {
        self.snapshot
            .workspaces
            .iter()
            .filter(|ws| output.is_none_or(|o| ws.output.as_deref() == Some(o)))
            .find(|ws| ws.is_active)
    }

    pub fn update(&mut self, message: Message) -> iced::Task<Message> {
        match message {
            Message::Snapshot(snapshot) => self.snapshot = snapshot,
            Message::OpenOverview => {
                utils::launcher::execute_command("niri msg action toggle-overview")
            }
            Message::Scroll(direction) => {
                if let Some(target) = self.workspace_after(direction) {
                    self.run(move |niri| niri.focus_workspace(target));
                }
            }
        }
        iced::Task::none()
    }

    /// The id of the workspace `direction` steps from the active one, on
    /// the same output, wrapping around -- `None` if there's no active
    /// workspace to step from at all (e.g. no snapshot yet).
    fn workspace_after(&self, direction: i32) -> Option<u64> {
        let active = self.active(None)?;
        let mut sorted: Vec<&WorkspaceInfo> = self
            .snapshot
            .workspaces
            .iter()
            .filter(|ws| ws.output == active.output)
            .collect();
        sorted.sort_by_key(|ws| ws.idx);
        let pos = sorted.iter().position(|ws| ws.id == active.id)?;
        let len = sorted.len() as i32;
        let next = (pos as i32 + direction).rem_euclid(len.max(1)) as usize;
        sorted.get(next).map(|ws| ws.id)
    }

    /// Fires a niri action on a blocking thread, fire-and-forget -- same
    /// shape as `Colonnade::run`.
    fn run(
        &self,
        action: impl FnOnce(&Niri) -> Result<(), colonnade_core::error::Error> + Send + 'static,
    ) {
        let niri = self.niri;
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || action(&niri)).await;
            match result {
                Ok(Err(e)) => warn!("workspace_indicator: niri action failed: {e}"),
                Err(e) => warn!("workspace_indicator: niri action task panicked: {e}"),
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
        let Some(ws) = self.active(monitor_name) else {
            return Space::new().width(Length::Shrink).into();
        };
        let label = ws
            .name
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| ws.idx.to_string());
        let font_size = use_theme(|t| t.font_size.sm);
        let color = use_theme(|t| t.palette.text);

        // Just the label -- press/scroll handling and the hover-pill
        // styling both come from the module-slot wrapper now
        // (`OnModulePress::CustomAction` in `modules::get_module_view`,
        // which routes through `ModuleItem`'s `position_button` +
        // `module_button_style()`, see `components::module_item`). That's
        // the same shared mechanism every other bar button uses, which is
        // what makes the hover highlight span the bar's full height
        // instead of just this text's own -- building a separate button
        // here (as an earlier version of this did) only gets a pill sized
        // to its own content, not the bar.
        text(format!("{label} \u{203a}"))
            .size(font_size)
            .color(color)
            .into()
    }
}
