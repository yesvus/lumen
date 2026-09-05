mod appearance;
mod colonnade;
mod custom_module;
mod keyboard_layout;
mod layout;
mod loader;
mod logging;
mod media_player;
mod notifications;
mod osd;
mod settings;
mod system_info;
mod tempo;
mod tray;
mod updates;
mod window_title;
mod workspaces;

pub use appearance::*;
pub use colonnade::*;
pub use custom_module::{CustomModuleDef, CustomModuleType, RegexCfg};
pub use keyboard_layout::*;
pub use layout::*;
pub use loader::{get_config, read_logging_config, subscription};
pub use logging::*;
pub use media_player::*;
pub use notifications::*;
pub use osd::*;
pub use settings::*;
pub use system_info::*;
pub use tempo::*;
pub use tray::*;
pub use updates::*;
pub use window_title::*;
pub use workspaces::*;

use regex::Regex;
use serde::Deserialize;

pub const DEFAULT_CONFIG_FILE_PATH: &str = "~/.config/lumen/config.toml";

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub logging: LoggingConfig,
    pub language: Option<String>,
    pub region: Option<String>,
    pub position: Position,
    pub layer: Layer,
    pub outputs: Outputs,
    pub modules: Modules,
    #[serde(rename = "CustomModule")]
    pub custom_modules: Vec<CustomModuleDef>,
    pub updates: Option<UpdatesModuleConfig>,
    pub workspaces: WorkspacesModuleConfig,
    pub colonnade: ColonnadeModuleConfig,
    pub window_title: WindowTitleConfig,
    pub system_info: SystemInfoModuleConfig,
    pub notifications: NotificationsModuleConfig,
    pub tray: TrayModuleConfig,
    pub tempo: TempoModuleConfig,
    pub settings: SettingsModuleConfig,
    pub appearance: Appearance,
    pub media_player: MediaPlayerModuleConfig,
    pub keyboard_layout: KeyboardLayoutModuleConfig,
    pub animations: AnimationsConfig,
    pub enable_esc_key: bool,
    pub osd: OsdConfig,
}

/// Lumen's own baseline, not upstream ashell's: if the config file is
/// missing or fails to parse, this is what should come up -- the bar this
/// fork actually runs, not a stranger's stock layout. Keep in sync with
/// `~/.config/lumen/config.toml` (the reference install) when that
/// changes.
impl Default for Config {
    fn default() -> Self {
        Self {
            logging: LoggingConfig::default(),
            language: None,
            region: Some("en-GB".to_string()),
            position: Position::default(),
            layer: Layer::Top,
            outputs: Outputs::default(),
            modules: Modules::default(),
            custom_modules: vec![
                CustomModuleDef {
                    name: "Applications".to_string(),
                    command: Some("fuzzel".to_string()),
                    icon: None,
                    listen_cmd: Some(
                        "printf '{\"text\":\"Applications\",\"alt\":\"\"}\\n'; sleep infinity"
                            .to_string(),
                    ),
                    icons: None,
                    alert: None,
                    r#type: CustomModuleType::Button,
                    on_right_click: None,
                    on_middle_click: None,
                    on_scroll_up: None,
                    on_scroll_down: None,
                },
                CustomModuleDef {
                    name: "Overview".to_string(),
                    command: Some("niri msg action toggle-overview".to_string()),
                    icon: None,
                    listen_cmd: Some(
                        "printf '{\"text\":\"Workspaces\",\"alt\":\"\"}\\n'; sleep infinity"
                            .to_string(),
                    ),
                    icons: None,
                    alert: None,
                    r#type: CustomModuleType::Button,
                    on_right_click: None,
                    on_middle_click: None,
                    on_scroll_up: Some("niri msg action focus-workspace-up".to_string()),
                    on_scroll_down: Some("niri msg action focus-workspace-down".to_string()),
                },
                CustomModuleDef {
                    name: "Claude5h".to_string(),
                    command: Some("alacritty --working-directory $HOME -e claude".to_string()),
                    icon: None,
                    listen_cmd: Some("~/.config/lumen/bin/claude-limits 5h".to_string()),
                    icons: None,
                    alert: Some(RegexCfg(Regex::new("critical").expect("valid regex"))),
                    r#type: CustomModuleType::Button,
                    on_right_click: None,
                    on_middle_click: None,
                    on_scroll_up: None,
                    on_scroll_down: None,
                },
                CustomModuleDef {
                    name: "Claude7d".to_string(),
                    command: Some("alacritty --working-directory $HOME -e claude".to_string()),
                    icon: None,
                    listen_cmd: Some("~/.config/lumen/bin/claude-limits 7d".to_string()),
                    icons: None,
                    alert: Some(RegexCfg(Regex::new("critical").expect("valid regex"))),
                    r#type: CustomModuleType::Button,
                    on_right_click: None,
                    on_middle_click: None,
                    on_scroll_up: None,
                    on_scroll_down: None,
                },
            ],
            updates: None,
            workspaces: WorkspacesModuleConfig::default(),
            colonnade: ColonnadeModuleConfig::default(),
            window_title: WindowTitleConfig::default(),
            system_info: SystemInfoModuleConfig::default(),
            notifications: NotificationsModuleConfig::default(),
            tray: TrayModuleConfig::default(),
            tempo: TempoModuleConfig::default(),
            settings: SettingsModuleConfig::default(),
            appearance: Appearance::default(),
            media_player: MediaPlayerModuleConfig::default(),
            keyboard_layout: KeyboardLayoutModuleConfig::default(),
            animations: AnimationsConfig::default(),
            enable_esc_key: false,
            osd: OsdConfig::default(),
        }
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct AnimationsConfig {
    pub enabled: bool,
}

impl Config {
    fn validate(&mut self) {
        if let Some(ref mut updates) = self.updates {
            updates.validate();
        }
        self.system_info.validate();
        self.tempo.validate();
        self.settings.validate();
        self.media_player.validate();
    }
}
