use serde::Deserialize;

#[derive(Deserialize, Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum WorkspaceVisibilityMode {
    #[default]
    All,
    MonitorSpecific,
    MonitorSpecificExclusive,
}

#[derive(Deserialize, Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum WorkspaceIndicatorFormat {
    #[default]
    Name,
    NameAndIcons,
}

#[derive(Deserialize, Clone, Default, Debug)]
#[serde(default)]
pub struct WorkspacesModuleConfig {
    pub visibility_mode: WorkspaceVisibilityMode,
    pub indicator_format: WorkspaceIndicatorFormat,
    pub group_by_monitor: bool,
    pub enable_workspace_filling: bool,
    pub disable_special_workspaces: bool,
    pub max_workspaces: Option<u32>,
    pub workspace_names: Vec<String>,
    pub enable_virtual_desktops: bool,
    pub invert_scroll_direction: Option<InvertScrollDirection>,
}

#[derive(Deserialize, Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum InvertScrollDirection {
    #[default]
    All,
    Mouse,
    Trackpad,
}
