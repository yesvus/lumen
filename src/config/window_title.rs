use serde::Deserialize;

#[derive(Deserialize, Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum WindowTitleMode {
    #[default]
    Title,
    Class,
    InitialTitle,
    InitialClass,
}

#[derive(Deserialize, Copy, Clone, Debug)]
#[serde(default)]
pub struct WindowTitleConfig {
    pub mode: WindowTitleMode,
    pub truncate_title_after_length: u32,
}

impl Default for WindowTitleConfig {
    fn default() -> Self {
        Self {
            mode: Default::default(),
            truncate_title_after_length: 150,
        }
    }
}
