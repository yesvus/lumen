use crate::i18n::{TemperatureUnit, unit_system};
use log::warn;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct SystemInfoCpu {
    pub warn_threshold: u32,
    pub alert_threshold: u32,

    pub format: CpuFormat,
}

pub(super) fn validate_thresholds<T: PartialOrd + Copy + std::fmt::Display>(
    warn: &mut T,
    alert: &mut T,
    name: &str,
) {
    if *warn >= *alert {
        warn!(
            "{name} warn_threshold ({warn}) >= alert_threshold ({alert}), setting both to {alert}"
        );
        *warn = *alert;
    }
}

impl SystemInfoCpu {
    fn validate(&mut self) {
        validate_thresholds(&mut self.warn_threshold, &mut self.alert_threshold, "CPU");
    }
}

impl Default for SystemInfoCpu {
    fn default() -> Self {
        Self {
            warn_threshold: 60,
            alert_threshold: 80,
            format: CpuFormat::Percentage,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct SystemInfoMemory {
    pub warn_threshold: u32,
    pub alert_threshold: u32,
    pub format: MemoryFormat,
}

impl SystemInfoMemory {
    fn validate(&mut self) {
        validate_thresholds(
            &mut self.warn_threshold,
            &mut self.alert_threshold,
            "Memory",
        );
    }
}

impl Default for SystemInfoMemory {
    fn default() -> Self {
        Self {
            warn_threshold: 70,
            alert_threshold: 85,
            format: MemoryFormat::Percentage,
        }
    }
}

const DEFAULT_TEMP_WARN_CELSIUS: i32 = 60;
const DEFAULT_TEMP_ALERT_CELSIUS: i32 = 80;

#[derive(Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub enum TemperatureSensorType {
    #[default]
    Cpu,
    Gpu,
    Acpi,
    Nvme,
}

/// A type keyword (auto-detected) or an exact sensor label.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum TemperatureSensor {
    Type(TemperatureSensorType),
    Label(String),
}

impl Default for TemperatureSensor {
    fn default() -> Self {
        Self::Type(TemperatureSensorType::default())
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct SystemInfoTemperature {
    warn_threshold: Option<i32>,
    alert_threshold: Option<i32>,
    pub sensor: TemperatureSensor,
    units: Option<TemperatureUnit>,
}

impl SystemInfoTemperature {
    pub fn resolved_units(&self) -> TemperatureUnit {
        self.units
            .unwrap_or_else(|| unit_system().temperature_unit())
    }

    pub fn warn_threshold(&self) -> i32 {
        self.warn_threshold.unwrap_or_else(|| {
            self.resolved_units()
                .convert_celsius(DEFAULT_TEMP_WARN_CELSIUS)
        })
    }

    pub fn alert_threshold(&self) -> i32 {
        self.alert_threshold.unwrap_or_else(|| {
            self.resolved_units()
                .convert_celsius(DEFAULT_TEMP_ALERT_CELSIUS)
        })
    }

    fn validate(&mut self) {
        if let (Some(warn), Some(alert)) = (&mut self.warn_threshold, &mut self.alert_threshold) {
            validate_thresholds(warn, alert, "Temperature");
        }
    }
}

#[derive(Clone, Debug, Deserialize, Default)]
pub enum DiskFormat {
    #[default]
    Percentage,
    Fraction,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub enum MemoryFormat {
    #[default]
    Percentage,
    Fraction,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub enum CpuFormat {
    #[default]
    Percentage,
    Frequency,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct SystemInfoDisk {
    pub warn_threshold: u32,
    pub alert_threshold: u32,
    pub format: DiskFormat,
    pub mounts: Option<Vec<String>>,
}

impl SystemInfoDisk {
    fn validate(&mut self) {
        validate_thresholds(&mut self.warn_threshold, &mut self.alert_threshold, "Disk");
    }
}

impl Default for SystemInfoDisk {
    fn default() -> Self {
        Self {
            warn_threshold: 80,
            alert_threshold: 90,
            format: DiskFormat::Percentage,
            mounts: None,
        }
    }
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SystemInfoDiskIndicatorConfig {
    #[serde(rename = "Disk")]
    pub path: String,
    #[serde(rename = "Name")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub enum SystemInfoIndicator {
    Cpu,
    Memory,
    MemorySwap,
    Temperature,
    IpAddress,
    DownloadSpeed,
    UploadSpeed,
    #[serde(untagged)]
    Disk(SystemInfoDiskIndicatorConfig),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct SystemInfoModuleConfig {
    pub indicators: Vec<SystemInfoIndicator>,
    #[serde(default = "SystemInfoModuleConfig::default_interval")]
    pub interval: u64,
    pub cpu: SystemInfoCpu,
    pub memory: SystemInfoMemory,
    pub temperature: SystemInfoTemperature,
    pub disk: SystemInfoDisk,
}

impl SystemInfoModuleConfig {
    const fn default_interval() -> u64 {
        5
    }

    pub(super) fn validate(&mut self) {
        if self.interval == 0 {
            warn!("SystemInfoModuleConfig.interval is 0, setting to 1");
            self.interval = 1;
        }
        self.cpu.validate();
        self.memory.validate();
        self.temperature.validate();
        self.disk.validate();
    }
}

impl Default for SystemInfoModuleConfig {
    fn default() -> Self {
        Self {
            indicators: vec![
                SystemInfoIndicator::Cpu,
                SystemInfoIndicator::Memory,
                SystemInfoIndicator::Temperature,
            ],
            interval: Self::default_interval(),
            cpu: SystemInfoCpu::default(),
            memory: SystemInfoMemory::default(),
            temperature: SystemInfoTemperature::default(),
            disk: SystemInfoDisk::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_thresholds_clamps_warn_to_alert_when_equal_or_over() {
        let mut warn = 90;
        let mut alert = 80;
        validate_thresholds(&mut warn, &mut alert, "test");
        assert_eq!(warn, 80);
        assert_eq!(alert, 80);
    }

    #[test]
    fn validate_thresholds_leaves_valid_pair_untouched() {
        let mut warn = 60;
        let mut alert = 80;
        validate_thresholds(&mut warn, &mut alert, "test");
        assert_eq!(warn, 60);
        assert_eq!(alert, 80);
    }

    #[test]
    fn zero_interval_is_clamped_to_one() {
        let mut config = SystemInfoModuleConfig {
            interval: 0,
            ..SystemInfoModuleConfig::default()
        };
        config.validate();
        assert_eq!(config.interval, 1);
    }

    #[test]
    fn temperature_threshold_defaults_follow_the_unit_system() {
        let temp = SystemInfoTemperature::default();
        assert_eq!(
            temp.warn_threshold(),
            temp.resolved_units().convert_celsius(60)
        );
        assert_eq!(
            temp.alert_threshold(),
            temp.resolved_units().convert_celsius(80)
        );
    }
}
