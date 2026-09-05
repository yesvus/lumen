use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct OsdConfig {
    pub enabled: bool,
    pub timeout: u64,
    pub show_volume_percentage: bool,
    pub show_brightness_percentage: bool,
}

impl Default for OsdConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout: 1500,
            show_volume_percentage: false,
            show_brightness_percentage: false,
        }
    }
}
