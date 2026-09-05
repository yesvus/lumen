use serde::{Deserialize, Deserializer, de::Visitor};

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Position {
    #[default]
    Top,
    Bottom,
}

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layer {
    #[default]
    Bottom,
    Top,
    Overlay,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleName {
    Updates,
    Workspaces,
    Colonnade,
    ArchMenu,
    WindowTitle,
    SystemInfo,
    KeyboardLayout,
    KeyboardSubmap,
    Tray,
    Tempo,
    Privacy,
    Settings,
    MediaPlayer,
    Custom(String),
    Notifications,
}

impl<'de> Deserialize<'de> for ModuleName {
    fn deserialize<D>(deserializer: D) -> Result<ModuleName, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ModuleNameVisitor;
        impl Visitor<'_> for ModuleNameVisitor {
            type Value = ModuleName;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string representing a ModuleName")
            }
            fn visit_str<E>(self, value: &str) -> Result<ModuleName, E>
            where
                E: serde::de::Error,
            {
                Ok(match value {
                    "Updates" => ModuleName::Updates,
                    "Workspaces" => ModuleName::Workspaces,
                    "Colonnade" => ModuleName::Colonnade,
                    "ArchMenu" => ModuleName::ArchMenu,
                    "WindowTitle" => ModuleName::WindowTitle,
                    "SystemInfo" => ModuleName::SystemInfo,
                    "KeyboardLayout" => ModuleName::KeyboardLayout,
                    "KeyboardSubmap" => ModuleName::KeyboardSubmap,
                    "Tray" => ModuleName::Tray,
                    "Notifications" => ModuleName::Notifications,
                    "Tempo" => ModuleName::Tempo,
                    "Privacy" => ModuleName::Privacy,
                    "Settings" => ModuleName::Settings,
                    "MediaPlayer" => ModuleName::MediaPlayer,
                    other => ModuleName::Custom(other.to_string()),
                })
            }
        }
        deserializer.deserialize_str(ModuleNameVisitor)
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum ModuleDef {
    Single(ModuleName),
    Group(Vec<ModuleName>),
}

#[derive(Deserialize, Clone, Debug)]
pub struct Modules {
    #[serde(default)]
    pub left: Vec<ModuleDef>,
    #[serde(default)]
    pub center: Vec<ModuleDef>,
    #[serde(default)]
    pub right: Vec<ModuleDef>,
}

impl Modules {
    pub fn contains(&self, target: &ModuleName) -> bool {
        self.left
            .iter()
            .chain(&self.center)
            .chain(&self.right)
            .any(|def| match def {
                ModuleDef::Single(m) => m == target,
                ModuleDef::Group(ms) => ms.contains(target),
            })
    }
}

impl Default for Modules {
    fn default() -> Self {
        Self {
            left: vec![
                ModuleDef::Single(ModuleName::ArchMenu),
                ModuleDef::Single(ModuleName::Custom("Applications".to_string())),
                ModuleDef::Single(ModuleName::Custom("Overview".to_string())),
                ModuleDef::Single(ModuleName::Colonnade),
            ],
            center: vec![],
            right: vec![
                ModuleDef::Single(ModuleName::Custom("Claude5h".to_string())),
                ModuleDef::Single(ModuleName::Custom("Claude7d".to_string())),
                ModuleDef::Single(ModuleName::SystemInfo),
                ModuleDef::Single(ModuleName::Settings),
                ModuleDef::Single(ModuleName::Tempo),
            ],
        }
    }
}

#[derive(Deserialize, Clone, Default, Debug, PartialEq, Eq)]
pub enum Outputs {
    #[default]
    All,
    Active,
    #[serde(deserialize_with = "non_empty")]
    Targets(Vec<String>),
}

fn non_empty<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let vec = <Vec<T>>::deserialize(d)?;
    if vec.is_empty() {
        use serde::de::Error;

        Err(D::Error::custom("need non-empty"))
    } else {
        Ok(vec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_name_falls_back_to_custom_for_unknown_strings() {
        assert_eq!(
            ModuleName::deserialize(
                serde::de::value::StrDeserializer::<serde::de::value::Error>::new("MyPlugin")
            )
            .unwrap(),
            ModuleName::Custom("MyPlugin".to_string())
        );
    }

    #[test]
    fn modules_contains_finds_names_inside_groups() {
        let modules = Modules {
            left: vec![],
            center: vec![ModuleDef::Group(vec![
                ModuleName::Tray,
                ModuleName::Notifications,
            ])],
            right: vec![],
        };
        assert!(modules.contains(&ModuleName::Notifications));
        assert!(!modules.contains(&ModuleName::Settings));
    }
}
