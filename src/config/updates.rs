use log::warn;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
pub struct UpdatesModuleConfig {
    pub check_cmd: String,
    pub update_cmd: String,
    #[serde(default = "UpdatesModuleConfig::default_interval")]
    pub interval: u64,
}

impl UpdatesModuleConfig {
    const fn default_interval() -> u64 {
        3600
    }

    pub(super) fn validate(&mut self) {
        if self.interval == 0 {
            warn!("UpdatesModuleConfig.interval is 0, setting to 1");
            self.interval = 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_interval_is_clamped_to_one() {
        let mut config = UpdatesModuleConfig {
            check_cmd: String::new(),
            update_cmd: String::new(),
            interval: 0,
        };
        config.validate();
        assert_eq!(config.interval, 1);
    }
}
