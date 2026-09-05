use log::warn;
use serde::Deserialize;

#[derive(Deserialize, Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum MediaPlayerFormat {
    Icon,
    #[serde(alias = "Title")]
    Text,
    #[serde(alias = "IconAndTitle")]
    #[default]
    IconAndText,
}

#[derive(Deserialize, Copy, Clone, PartialEq, Eq, Debug)]
pub enum MediaPlayerTextField {
    Artist,
    Title,
    Album,
}

#[derive(Deserialize, Copy, Clone, PartialEq, Eq, Debug)]
pub enum MediaPlayerVisualizer {
    Background,
    Before,
    After,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct MediaPlayerModuleConfig {
    pub indicator_format: MediaPlayerFormat,
    pub indicator_fields: Vec<MediaPlayerTextField>,
    pub max_text_length: u32,
    pub indicator_visualizer: Option<MediaPlayerVisualizer>,
    pub menu_visualizer: bool,
    pub visualizer_framerate: u32,
}

impl Default for MediaPlayerModuleConfig {
    fn default() -> Self {
        MediaPlayerModuleConfig {
            indicator_format: MediaPlayerFormat::default(),
            indicator_fields: vec![MediaPlayerTextField::Artist, MediaPlayerTextField::Title],
            max_text_length: 100,
            indicator_visualizer: None,
            menu_visualizer: false,
            visualizer_framerate: Self::DEFAULT_VISUALIZER_FRAMERATE,
        }
    }
}

impl MediaPlayerModuleConfig {
    const DEFAULT_VISUALIZER_FRAMERATE: u32 = 30;
    const MAX_VISUALIZER_FRAMERATE: u32 = 144;

    pub(super) fn validate(&mut self) {
        let clamped = self
            .visualizer_framerate
            .clamp(1, Self::MAX_VISUALIZER_FRAMERATE);
        if clamped != self.visualizer_framerate {
            warn!(
                "MediaPlayerModuleConfig.visualizer_framerate is {}, setting to {clamped}",
                self.visualizer_framerate
            );
            self.visualizer_framerate = clamped;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framerate_above_max_is_clamped() {
        let mut config = MediaPlayerModuleConfig {
            visualizer_framerate: 1000,
            ..MediaPlayerModuleConfig::default()
        };
        config.validate();
        assert_eq!(
            config.visualizer_framerate,
            MediaPlayerModuleConfig::MAX_VISUALIZER_FRAMERATE
        );
    }

    #[test]
    fn framerate_of_zero_is_clamped_to_one() {
        let mut config = MediaPlayerModuleConfig {
            visualizer_framerate: 0,
            ..MediaPlayerModuleConfig::default()
        };
        config.validate();
        assert_eq!(config.visualizer_framerate, 1);
    }
}
