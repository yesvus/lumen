use super::{Config, DEFAULT_CONFIG_FILE_PATH, LoggingConfig};
use crate::app::Message;
use iced::futures::StreamExt;
use iced::{Subscription, futures::SinkExt, stream::channel};
use inotify::EventMask;
use inotify::Inotify;
use inotify::WatchMask;
use log::{debug, error, info, warn};
use serde::Deserialize;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::sleep;

/// Parses just the `[logging]` table, before the logger exists: no `log::*`
/// here, problems go to `stderr`.
pub fn read_logging_config(path: Option<&PathBuf>) -> LoggingConfig {
    #[derive(Deserialize)]
    struct LoggingConfigWrapper {
        #[serde(default)]
        logging: LoggingConfig,
    }

    let config_path = match resolve_config_path(path.map(PathBuf::as_path)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "lumen: warning: cannot expand the config path: {e}, using the default logging setup"
            );
            return LoggingConfig::default();
        }
    };

    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "lumen: warning: cannot read {}: {e}, using the default logging setup",
                    config_path.display()
                );
            }
            return LoggingConfig::default();
        }
    };

    match toml::from_str::<LoggingConfigWrapper>(&content) {
        Ok(wrapper) => wrapper.logging,
        Err(e) => {
            eprintln!(
                "lumen: warning: cannot read the [logging] section of {}, using the default logging setup:\n{e}",
                config_path.display()
            );
            LoggingConfig::default()
        }
    }
}

fn resolve_config_path(path: Option<&Path>) -> Result<PathBuf, Box<dyn Error + Send>> {
    expand_path(match path {
        Some(p) => p.to_path_buf(),
        None => PathBuf::from(DEFAULT_CONFIG_FILE_PATH),
    })
}

pub fn get_config(path: Option<PathBuf>) -> Result<(Config, PathBuf), Box<dyn Error + Send>> {
    let explicit = path.is_some();
    let expanded = resolve_config_path(path.as_deref())?;

    if explicit {
        info!("Config path provided {expanded:?}");

        if !expanded.exists() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Config file does not exist: {}", expanded.display()),
            )));
        }
    } else {
        // DEFAULT_CONFIG_FILE_PATH has a parent and shellexpand never strips components.
        let parent = expanded
            .parent()
            .expect("Failed to get default config parent directory");

        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .expect("Failed to create default config parent directory");
        }
    }

    Ok((read_config(&expanded).unwrap_or_default(), expanded))
}

pub(super) fn expand_path(path: PathBuf) -> Result<PathBuf, Box<dyn Error + Send>> {
    let str_path = path.to_string_lossy();
    let expanded =
        shellexpand::full(&str_path).map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;

    Ok(PathBuf::from(expanded.to_string()))
}

fn read_config(path: &Path) -> Result<Config, Box<dyn Error + Send>> {
    let content =
        std::fs::read_to_string(path).map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;

    info!("Decoding config file {path:?}");

    let de =
        toml::Deserializer::parse(&content).map_err(|e| Box::new(e) as Box<dyn Error + Send>)?;
    let mut unknown_fields = Vec::new();
    let res = serde_ignored::deserialize(de, |path| {
        unknown_fields.push(path.to_string());
    });

    match res {
        Ok(config) => {
            for field in &unknown_fields {
                let msg = format!("Unknown configuration field ignored: {field}");
                warn!("{msg}");
                eprintln!("lumen: warning: {msg}");
            }
            info!("Config file loaded successfully");
            let mut config: Config = config;
            config.validate();
            Ok(config)
        }
        Err(e) => {
            warn!("Failed to parse config file: {e}");
            Err(Box::new(e))
        }
    }
}

enum Event {
    Changed,
    Removed,
}

pub fn subscription(path: &Path) -> Subscription<Message> {
    let path = path.to_path_buf();

    Subscription::run_with(path, |path| {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        channel(100, async move |mut output| {
            match (path.parent(), path.file_name(), Inotify::init()) {
                (Some(folder), Some(file_name), Ok(inotify)) => {
                    debug!("Watching config file at {path:?}");

                    let res = inotify.watches().add(
                        folder,
                        WatchMask::CREATE | WatchMask::DELETE | WatchMask::MOVE | WatchMask::MODIFY,
                    );

                    if let Err(e) = res {
                        error!("Failed to add watch for {folder:?}: {e}");
                        return;
                    }

                    let buffer = [0; 1024];
                    let stream = inotify.into_event_stream(buffer);

                    if let Ok(stream) = stream {
                        let mut stream = stream.ready_chunks(10);

                        debug!("Starting config file watch loop");

                        loop {
                            let events = stream.next().await.unwrap_or(vec![]);

                            debug!("Received inotify events: {events:?}");

                            let mut file_event = None;

                            for event in events {
                                debug!("Event: {event:?}");
                                match event {
                                    Ok(inotify::Event {
                                        name: Some(name),
                                        mask: EventMask::DELETE | EventMask::MOVED_FROM,
                                        ..
                                    }) if file_name == name => {
                                        debug!("File deleted or moved");
                                        file_event = Some(Event::Removed);
                                    }
                                    Ok(inotify::Event {
                                        name: Some(name),
                                        mask:
                                            EventMask::CREATE | EventMask::MODIFY | EventMask::MOVED_TO,
                                        ..
                                    }) if file_name == name => {
                                        debug!("File created or moved");

                                        file_event = Some(Event::Changed);
                                    }
                                    _ => {
                                        debug!("Ignoring event");
                                    }
                                }
                            }

                            match file_event {
                                Some(Event::Changed) => {
                                    info!("Reload config file");

                                    let path_clone = path.clone();
                                    let new_config = tokio::task::spawn_blocking(move || {
                                        read_config(&path_clone).unwrap_or_default()
                                    })
                                    .await
                                    .unwrap_or_default();

                                    let _ = output
                                        .send(Message::ConfigChanged(Box::new(new_config)))
                                        .await;
                                }
                                Some(Event::Removed) => {
                                    // wait and double check if the file is really gone
                                    sleep(Duration::from_millis(500)).await;

                                    if !path.exists() {
                                        info!("Config file removed");
                                        let _ = output
                                            .send(Message::ConfigChanged(Box::default()))
                                            .await;
                                    }
                                }
                                None => {
                                    debug!("No relevant file event detected.");
                                }
                            }
                        }
                    } else {
                        error!("Failed to create inotify event stream");
                    }
                }
                (None, _, _) => {
                    error!(
                        "Config file path does not have a parent directory, cannot watch for changes"
                    );
                }
                (_, None, _) => {
                    error!("Config file path does not have a file name, cannot watch for changes");
                }
                (_, _, Err(e)) => {
                    error!("Failed to initialize inotify: {e}");
                }
            }
        })
    })
}
