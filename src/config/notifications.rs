use super::custom_module::RegexCfg;
use serde::Deserialize;

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastPosition {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct NotificationsModuleConfig {
    pub format: String,
    pub show_timestamps: bool,
    pub show_bodies: bool,
    pub grouped: bool,
    pub toast: bool,
    pub toast_position: ToastPosition,
    pub toast_timeout: u64,
    pub toast_limit: usize,
    pub toast_max_height: u32,
    pub blocklist: Vec<RegexCfg>,
}
impl Default for NotificationsModuleConfig {
    fn default() -> Self {
        Self {
            format: "%H:%M".to_string(),
            show_timestamps: true,
            show_bodies: true,
            grouped: false,
            toast: true,
            toast_position: ToastPosition::default(),
            toast_timeout: 5000,
            toast_limit: 5,
            toast_max_height: 150,
            blocklist: vec![],
        }
    }
}
