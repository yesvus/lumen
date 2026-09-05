use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Clone, Default, Debug)]
#[serde(default)]
pub struct KeyboardLayoutModuleConfig {
    pub labels: HashMap<String, String>,
}
