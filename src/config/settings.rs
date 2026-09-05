use super::custom_module::empty_string_as_none;
use crate::services::upower::PeripheralDeviceKind;
use log::warn;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum SettingsIndicator {
    IdleInhibitor,
    PowerProfile,
    Audio,
    Microphone,
    Network,
    Vpn,
    Bluetooth,
    Battery,
    PeripheralBattery,
    Brightness,
}

#[derive(Deserialize, Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum SettingsFormat {
    Icon,
    #[serde(alias = "Value")]
    Percentage,
    #[default]
    #[serde(alias = "IconAndValue")]
    IconAndPercentage,
    Time,
    IconAndTime,
    Name,
    IconAndName,
}

#[derive(Deserialize, Clone, Default, PartialEq, Eq, Debug)]
pub enum PeripheralIndicators {
    #[default]
    All,
    Specific(Vec<PeripheralDeviceKind>),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct SettingsModuleConfig {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub lock_cmd: Option<String>,
    pub shutdown_cmd: String,
    pub suspend_cmd: String,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub hibernate_cmd: Option<String>,
    pub reboot_cmd: String,
    pub logout_cmd: String,
    pub battery_format: SettingsFormat,
    pub battery_hide_when_full: bool,
    pub peripheral_indicators: PeripheralIndicators,
    pub peripheral_battery_format: SettingsFormat,
    pub peripheral_expanded_by_default: bool,
    pub volume_step: u8,
    pub max_volume: u8,
    pub audio_indicator_format: SettingsFormat,
    pub microphone_indicator_format: SettingsFormat,
    pub network_indicator_format: SettingsFormat,
    pub bluetooth_indicator_format: SettingsFormat,
    pub brightness_indicator_format: SettingsFormat,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub audio_sinks_more_cmd: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub audio_sources_more_cmd: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub wifi_more_cmd: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub vpn_more_cmd: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub bluetooth_more_cmd: Option<String>,
    pub remove_airplane_btn: bool,
    pub remove_idle_btn: bool,
    pub enable_tooltips: bool,
    pub indicators: Vec<SettingsIndicator>,
    #[serde(rename = "CustomButton")]
    pub custom_buttons: Vec<SettingsCustomButton>,
}

impl Default for SettingsModuleConfig {
    fn default() -> Self {
        Self {
            lock_cmd: Default::default(),
            shutdown_cmd: "shutdown now".to_string(),
            suspend_cmd: "systemctl suspend".to_string(),
            hibernate_cmd: Default::default(),
            reboot_cmd: "systemctl reboot".to_string(),
            logout_cmd: "loginctl kill-user $(whoami)".to_string(),
            battery_format: SettingsFormat::IconAndPercentage,
            battery_hide_when_full: false,
            peripheral_indicators: Default::default(),
            peripheral_battery_format: SettingsFormat::Icon,
            peripheral_expanded_by_default: false,
            volume_step: 5,
            max_volume: 100,
            audio_indicator_format: SettingsFormat::Icon,
            microphone_indicator_format: SettingsFormat::Icon,
            network_indicator_format: SettingsFormat::Icon,
            bluetooth_indicator_format: SettingsFormat::Icon,
            brightness_indicator_format: SettingsFormat::Icon,
            audio_sinks_more_cmd: Default::default(),
            audio_sources_more_cmd: Default::default(),
            wifi_more_cmd: Default::default(),
            vpn_more_cmd: Default::default(),
            bluetooth_more_cmd: Default::default(),
            remove_airplane_btn: Default::default(),
            remove_idle_btn: Default::default(),
            enable_tooltips: true,
            indicators: vec![
                SettingsIndicator::IdleInhibitor,
                SettingsIndicator::PowerProfile,
                SettingsIndicator::Audio,
                SettingsIndicator::Microphone,
                SettingsIndicator::Bluetooth,
                SettingsIndicator::Network,
                SettingsIndicator::Vpn,
                SettingsIndicator::Battery,
            ],
            custom_buttons: Default::default(),
        }
    }
}

impl SettingsModuleConfig {
    pub(super) fn validate(&mut self) {
        if self.max_volume == 0 || self.max_volume > 200 {
            warn!(
                "settings.max_volume ({}) out of range 1..=200, clamping to 100",
                self.max_volume
            );
            self.max_volume = 100;
        }
        if self.volume_step == 0 || self.volume_step > 50 {
            warn!(
                "settings.volume_step ({}) out of range 1..=50, clamping to 5",
                self.volume_step
            );
            self.volume_step = 5;
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct SettingsCustomButton {
    pub name: String,
    pub icon: String,
    pub command: String,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub status_command: Option<String>,
    pub tooltip: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_range_max_volume_is_clamped_to_100() {
        let mut config = SettingsModuleConfig {
            max_volume: 255,
            ..SettingsModuleConfig::default()
        };
        config.validate();
        assert_eq!(config.max_volume, 100);

        config.max_volume = 0;
        config.validate();
        assert_eq!(config.max_volume, 100);
    }

    #[test]
    fn out_of_range_volume_step_is_clamped_to_5() {
        let mut config = SettingsModuleConfig {
            volume_step: 51,
            ..SettingsModuleConfig::default()
        };
        config.validate();
        assert_eq!(config.volume_step, 5);
    }

    #[test]
    fn in_range_values_are_untouched() {
        let mut config = SettingsModuleConfig {
            max_volume: 150,
            volume_step: 10,
            ..SettingsModuleConfig::default()
        };
        config.validate();
        assert_eq!(config.max_volume, 150);
        assert_eq!(config.volume_step, 10);
    }
}
