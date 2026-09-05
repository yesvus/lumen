use crate::{
    components::icons::{DynamicIcon, StaticIcon, icon},
    config::CustomModuleDef,
    theme::use_theme,
    utils::launcher::execute_command,
};
use iced::widget::canvas;
use iced::{
    Alignment, Element, Length, Subscription, Theme,
    stream::channel,
    widget::{Space, Stack, row, text},
};
use iced::{
    mouse::Cursor,
    widget::{
        canvas::{Cache, Geometry, Path, Program},
        container,
    },
};
use log::{error, info, warn};
use serde::Deserialize;
use std::process::Stdio;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

#[derive(Debug, Clone)]
pub struct Custom {
    pub config: CustomModuleDef,
    data: CustomListenData,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CustomListenData {
    pub alt: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    LaunchCommand,
    LaunchRightClickCommand,
    LaunchMiddleClickCommand,
    LaunchScrollUpCommand,
    LaunchScrollDownCommand,
    Update(CustomListenData),
}

// Define a struct for the canvas program
#[derive(Debug, Clone, Copy, Default)]
struct AlertIndicator;

impl<Message> Program<Message> for AlertIndicator {
    type State = Cache;

    fn draw(
        &self,
        cache: &Self::State,
        renderer: &iced::Renderer,
        theme: &Theme,
        bounds: iced::Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let geometry = cache.draw(renderer, bounds.size(), |frame| {
            let center = frame.center();
            // Use a smaller radius so the circle doesn't touch the canvas edges
            let radius = 2.0; // Creates a 4px diameter circle
            let circle = Path::circle(center, radius);
            frame.fill(&circle, theme.palette().danger);
        });

        vec![geometry]
    }
}

impl Custom {
    pub fn new(config: CustomModuleDef) -> Self {
        Self {
            config,
            data: CustomListenData::default(),
        }
    }

    /// Rebuilds `self` for a reloaded config while keeping whatever
    /// `listen_cmd` last reported. A hot-reload otherwise recreates every
    /// `Custom` from scratch, and a one-shot `listen_cmd` (`printf ...;
    /// sleep infinity`, used for static text labels) never fires again to
    /// repopulate it -- its subscription is keyed on the command string,
    /// which hasn't changed, so iced treats it as the same subscription
    /// and never re-spawns the process.
    pub fn reload(&mut self, config: CustomModuleDef) {
        self.config = config;
    }

    pub fn module_type(&self) -> crate::config::CustomModuleType {
        self.config.r#type
    }

    pub fn update(&mut self, msg: Message) {
        match msg {
            Message::LaunchCommand => {
                if let Some(cmd) = &self.config.command {
                    execute_command(cmd);
                }
            }
            Message::LaunchRightClickCommand => {
                if let Some(cmd) = &self.config.on_right_click {
                    execute_command(cmd);
                }
            }
            Message::LaunchMiddleClickCommand => {
                if let Some(cmd) = &self.config.on_middle_click {
                    execute_command(cmd);
                }
            }
            Message::LaunchScrollUpCommand => {
                if let Some(cmd) = &self.config.on_scroll_up {
                    execute_command(cmd);
                }
            }
            Message::LaunchScrollDownCommand => {
                if let Some(cmd) = &self.config.on_scroll_down {
                    execute_command(cmd);
                }
            }
            Message::Update(data) => {
                self.data = data;
            }
        }
    }

    pub fn view<'a>(&'a self) -> Element<'a, Message> {
        let (space, font_size) = use_theme(|theme| (theme.space, theme.font_size));
        match self.config.r#type {
            crate::config::CustomModuleType::Text => self
                .data
                .text
                .as_ref()
                .and_then(|text_content| {
                    if !text_content.is_empty() {
                        Some(text(text_content.clone()).size(font_size.sm).into())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| Space::new().width(Length::Shrink).into()),
            crate::config::CustomModuleType::Button => {
                // Resolve the icon to an Option first, and only build a
                // widget for it when there is one to draw. It used to fall
                // back to StaticIcon::None, which is empty but still real:
                // it occupied a padded container and earned the row spacing
                // next to it, so every text-only button carried a leading
                // gap nothing had asked for.
                let mut icon_str = self.config.icon.clone();

                if let Some(icons_map) = &self.config.icons {
                    for (re, icon_str_match) in icons_map {
                        if re.is_match(&self.data.alt) {
                            icon_str = Some(icon_str_match.clone());
                            break; // Use the first match
                        }
                    }
                }

                let show_alert = self
                    .config
                    .alert
                    .as_ref()
                    .is_some_and(|re| re.is_match(&self.data.alt));

                // Positions the alert dot at the top-right of whatever it is
                // stacked over.
                let with_alert = |base: Element<'a, Message>| -> Element<'a, Message> {
                    let alert_canvas = canvas(AlertIndicator)
                        .width(Length::Fixed(space.xs)) // Size of the dot
                        .height(Length::Fixed(space.xs));

                    let alert_indicator_container = container(alert_canvas)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .align_x(iced::alignment::Horizontal::Right)
                        .align_y(iced::alignment::Vertical::Top);

                    Stack::new()
                        .push(base)
                        .push(alert_indicator_container)
                        .into()
                };

                let icon_element = icon_str.map(|icon_str| {
                    // Wrap the icon in a container to apply padding
                    let padded: Element<'a, Message> = container(icon(DynamicIcon(icon_str)))
                        .padding([0, 1])
                        .into();
                    // The dot rides the icon when there is one, exactly as
                    // before, rather than the whole row.
                    if show_alert {
                        with_alert(padded)
                    } else {
                        padded
                    }
                });

                let text_element = self.data.text.as_ref().and_then(|text_content| {
                    if !text_content.is_empty() {
                        Some(text(text_content.clone()).size(font_size.sm))
                    } else {
                        None
                    }
                });

                // align_y(Center) is required here, not decorative: icon
                // and text have different line-heights, so this row
                // defaults to top-alignment without it. See ModuleItem's
                // doc comment (src/components/module_item.rs) for why the
                // bar's own centering can't fix this from the outside.
                match (icon_element, text_element) {
                    (Some(icon_element), Some(text_element)) => row![icon_element, text_element]
                        .spacing(space.xs)
                        .align_y(Alignment::Center)
                        .into(),
                    (Some(icon_element), None) => icon_element,
                    // Text with no icon: no leading container, no spacing.
                    // The dot has nothing else to ride, so it takes the text.
                    (None, Some(text_element)) => {
                        if show_alert {
                            with_alert(text_element.into())
                        } else {
                            text_element.into()
                        }
                    }
                    // Neither: keep an empty icon so the button still has a
                    // hit area to click.
                    (None, None) => icon(StaticIcon::None).into(),
                }
            }
        }
    }

    pub fn subscription(&self) -> Subscription<(String, Message)> {
        let name = self.config.name.clone();
        if let Some(listen_cmd) = self.config.listen_cmd.clone() {
            Subscription::run_with((name, listen_cmd), |data| {
                let (name, listen_cmd) = data.clone();
                channel(10, async move |mut output| {
                    let command = Command::new("bash")
                        .arg("-c")
                        .arg(&listen_cmd)
                        .stdout(Stdio::piped())
                        .spawn();

                    match command {
                        Ok(mut child) => {
                            if let Some(stdout) = child.stdout.take() {
                                let mut reader = BufReader::new(stdout).lines();
                                let mut buf = String::new();

                                // Ensure the child process is spawned in the runtime so it can
                                // make progress on its own while we await for any output.
                                tokio::spawn(async move {
                                    match child.wait().await {
                                        Ok(status) => info!("child status was: {status}"),
                                        Err(e) => warn!("child process encountered an error: {e}"),
                                    }
                                });

                                while let Some(line) = reader.next_line().await.ok().flatten() {
                                    buf.push_str(&line);
                                    buf.push('\n');
                                    match serde_json::from_str::<CustomListenData>(&buf) {
                                        Ok(event) => {
                                            buf.clear();
                                            if let Err(e) = output
                                                .try_send((name.clone(), Message::Update(event)))
                                            {
                                                error!(
                                                    "Failed to send update for custom module '{name}': {e}"
                                                );
                                            }
                                        }
                                        Err(e) if e.is_eof() => {
                                            if buf.len() > 1 << 20 {
                                                warn!(
                                                    "custom module '{name}': dropping {} bytes of unterminated JSON",
                                                    buf.len()
                                                );
                                                buf.clear();
                                            }
                                        }
                                        Err(e) => {
                                            error!(
                                                "Failed to parse JSON for custom module '{name}': {e} (payload: {buf})"
                                            );
                                            buf.clear();
                                        }
                                    }
                                }
                            } else {
                                error!("Failed to capture stdout for command: {listen_cmd}");
                            }
                        }
                        Err(error) => {
                            error!("Failed to execute command: {error}");
                        }
                    }
                })
            })
        } else {
            Subscription::none()
        }
    }
}
