use std::path::PathBuf;

#[derive(serde::Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogTarget {
    #[default]
    File,
    Stdout,
    Stderr,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub target: LogTarget,
    pub directory: Option<PathBuf>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "warn".to_owned(),
            target: LogTarget::default(),
            directory: None,
        }
    }
}

impl LoggingConfig {
    pub fn log_directory(&self) -> PathBuf {
        let default_directory = || {
            crate::xdg::get_runtime_dir().unwrap_or_else(|| {
                [std::env::temp_dir(), PathBuf::from("lumen")]
                    .iter()
                    .collect()
            })
        };

        match &self.directory {
            Some(directory) => super::loader::expand_path(directory.clone()).unwrap_or_else(|e| {
                eprintln!(
                    "lumen: warning: cannot expand logging.directory {directory:?}: {e}, using the default"
                );
                default_directory()
            }),
            None => default_directory(),
        }
    }
}
