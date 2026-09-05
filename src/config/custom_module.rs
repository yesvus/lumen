use regex::Regex;
use serde::{Deserialize, Deserializer};
use serde_with::DisplayFromStr;
use serde_with::serde_as;
use std::{collections::HashMap, ops::Deref};

pub(super) fn empty_string_as_none<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(d)?
        .and_then(|value| (!value.trim().is_empty()).then_some(value)))
}

/// Newtype wrapper around `Regex`to be deserializable and usable as a hashmap key
#[serde_as]
#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct RegexCfg(#[serde_as(as = "DisplayFromStr")] pub Regex);

impl PartialEq for RegexCfg {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}
impl Eq for RegexCfg {}

impl std::hash::Hash for RegexCfg {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // hash the raw pattern string
        self.0.as_str().hash(state);
    }
}

impl Deref for RegexCfg {
    type Target = Regex;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Deserialize, Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum CustomModuleType {
    #[default]
    Button,
    Text,
}

#[serde_as]
#[derive(Deserialize, Clone, Debug)]
pub struct CustomModuleDef {
    pub name: String,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,

    /// yields json lines containing text, alt, (pot tooltip)
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub listen_cmd: Option<String>,
    /// map of regex -> icon
    pub icons: Option<HashMap<RegexCfg, String>>,
    /// regex to show alert
    pub alert: Option<RegexCfg>,
    /// Display type: Button (clickable) or Text (display only)
    #[serde(default)]
    pub r#type: CustomModuleType,
    /// command to run on right-click
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub on_right_click: Option<String>,
    /// command to run on middle-click
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub on_middle_click: Option<String>,
    /// command to run on scroll up
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub on_scroll_up: Option<String>,
    /// command to run on scroll down
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub on_scroll_down: Option<String>,
    // .. appearance etc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Wrapper {
        #[serde(default, deserialize_with = "empty_string_as_none")]
        value: Option<String>,
    }

    #[test]
    fn empty_string_becomes_none() {
        let w: Wrapper = toml::from_str("value = \"\"").unwrap();
        assert_eq!(w.value, None);
    }

    #[test]
    fn whitespace_only_string_becomes_none() {
        let w: Wrapper = toml::from_str("value = \"   \"").unwrap();
        assert_eq!(w.value, None);
    }

    #[test]
    fn non_empty_string_is_kept() {
        let w: Wrapper = toml::from_str("value = \"hello\"").unwrap();
        assert_eq!(w.value, Some("hello".to_string()));
    }

    #[test]
    fn regex_cfg_equality_compares_pattern_source() {
        let a = RegexCfg(Regex::new("critical").unwrap());
        let b = RegexCfg(Regex::new("critical").unwrap());
        assert_eq!(a, b);
    }
}
