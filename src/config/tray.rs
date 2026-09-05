use super::custom_module::RegexCfg;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum TrayClickAction {
    Open,
    Menu,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct TrayModuleConfig {
    pub blocklist: Vec<RegexCfg>,
    pub right_click: Option<TrayClickAction>,
}
