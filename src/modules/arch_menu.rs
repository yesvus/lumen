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

#[derive(Default)]
pub struct ArchMenu;

impl ArchMenu {
    pub fn update(&mut self, message: Message) {
        let command = match message {
            Message::AboutThisPc => term_hold("fastfetch"),
            Message::SystemSettings => "xfce4-settings-manager".to_string(),
            Message::CheckForUpdates => {
                term_hold(r#"checkupdates || echo "System is up to date.""#)
            }
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
