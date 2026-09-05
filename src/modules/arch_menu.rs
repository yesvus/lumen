//! Native port of the Waybar GtkMenu (menu-file: arch-menu.xml) that used to
//! back `custom/archmenu` there, and more recently `~/.config/lumen/bin/arch-menu`
//! (fuzzel in dmenu mode) as a Lumen `CustomModule` shim. A `CustomModule` runs
//! one command per click and can't own a popup, so this module draws the menu
//! itself through the same menu-layer machinery the settings panel uses.

use crate::{
    components::{
        IconPosition, MenuSize, divider,
        icons::{StaticIcon, icon},
        styled_button,
    },
    theme::use_theme,
    utils,
};
use iced::{Element, Length, widget::column};

#[derive(Debug, Clone, Copy)]
pub enum Message {
    AboutThisPc,
    SystemSettings,
    CheckForUpdates,
    ReloadLumen,
    LockScreen,
    LogOut,
    Sleep,
    Restart,
    ShutDown,
}

/// The same throwaway floating terminal the Waybar menu used for its two
/// read-only entries: run the command, then wait for a keypress so the
/// output stays up.
fn term_hold(cmd: &str) -> String {
    format!(
        "alacritty --class arch-about-float \
         -o window.dimensions.columns=100 -o window.dimensions.lines=40 \
         -e sh -c '{cmd} ; echo; echo \"Press any key to close...\"; read -n1 -s'"
    )
}

/// Relaunches the current `lumen` binary (same args this process was
/// started with, e.g. `--config-path`) and then exits this process.
///
/// A real restart, not just re-reading config: some module state has no
/// in-process recovery path once it wedges -- e.g. Colonnade's niri event
/// stream just stops producing snapshots forever if the underlying IPC
/// socket read ever errors (a stray/unrecognized event during something
/// like a Firefox tab drag-detach can trigger this), silently freezing the
/// tab strip at its last-known state until the process restarts. This is
/// the user-facing "turn it off and on again" for that whole class of bug,
/// not a fix for any specific one.
fn reload_lumen() {
    let Ok(exe) = std::env::current_exe() else {
        log::error!("arch_menu: failed to resolve current_exe, cannot reload");
        return;
    };
    let args: Vec<String> = std::env::args().skip(1).collect();

    match std::process::Command::new(&exe).args(&args).spawn() {
        Ok(_) => std::process::exit(0),
        Err(e) => log::error!("arch_menu: failed to respawn lumen for reload: {e}"),
    }
}

#[derive(Default)]
pub struct ArchMenu;

impl ArchMenu {
    pub fn update(&mut self, message: Message) {
        if matches!(message, Message::ReloadLumen) {
            reload_lumen();
            return;
        }

        let command = match message {
            Message::AboutThisPc => term_hold("fastfetch"),
            Message::SystemSettings => "xfce4-settings-manager".to_string(),
            Message::CheckForUpdates => {
                term_hold(r#"checkupdates || echo "System is up to date.""#)
            }
            Message::ReloadLumen => unreachable!("handled above"),
            Message::LockScreen => "hyprlock".to_string(),
            Message::LogOut => "niri msg action quit".to_string(),
            Message::Sleep => "systemctl suspend".to_string(),
            Message::Restart => "systemctl reboot".to_string(),
            Message::ShutDown => "systemctl poweroff".to_string(),
        };
        utils::launcher::execute_command(&command);
    }

    pub fn view(&self) -> Element<'_, Message> {
        icon(StaticIcon::ArchLinux).into()
    }

    pub fn menu_view(&self) -> Element<'_, Message> {
        let space = use_theme(|t| t.space);
        column!(
            styled_button("About This PC")
                .icon(StaticIcon::Info, IconPosition::Before)
                .on_press(Message::AboutThisPc)
                .width(Length::Fill),
            styled_button("System Settings")
                .icon(StaticIcon::Settings, IconPosition::Before)
                .on_press(Message::SystemSettings)
                .width(Length::Fill),
            styled_button("Check for Updates")
                .icon(StaticIcon::Refresh, IconPosition::Before)
                .on_press(Message::CheckForUpdates)
                .width(Length::Fill),
            styled_button("Reload Lumen")
                .icon(StaticIcon::Refresh, IconPosition::Before)
                .on_press(Message::ReloadLumen)
                .width(Length::Fill),
            divider(),
            styled_button("Lock Screen")
                .icon(StaticIcon::Lock, IconPosition::Before)
                .on_press(Message::LockScreen)
                .width(Length::Fill),
            styled_button("Log Out...")
                .icon(StaticIcon::Logout, IconPosition::Before)
                .on_press(Message::LogOut)
                .width(Length::Fill),
            styled_button("Sleep")
                .icon(StaticIcon::Suspend, IconPosition::Before)
                .on_press(Message::Sleep)
                .width(Length::Fill),
            styled_button("Restart...")
                .icon(StaticIcon::Reboot, IconPosition::Before)
                .on_press(Message::Restart)
                .width(Length::Fill),
            styled_button("Shut Down...")
                .icon(StaticIcon::Power, IconPosition::Before)
                .on_press(Message::ShutDown)
                .width(Length::Fill),
        )
        .padding(space.xs)
        .width(MenuSize::Small)
        .spacing(space.xs)
        .into()
    }
}
