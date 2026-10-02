//! Configuration persistence with backup rotation and safe mode.
//!
//! Failure policy (docs/threat-model.md T4): a corrupted config must never
//! brick the app or the network. We keep N rotated backups; if the live
//! file is unreadable we fall back to the newest valid backup; if that
//! fails too we start from defaults with `degraded` set so the UI can
//! tell the user exactly what happened. We never panic and never exit.

use std::fs;
use std::path::{Path, PathBuf};

use linkfyr_ipc::Config;
use thiserror::Error;

const MAX_BACKUPS: usize = 5;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

#[derive(Debug)]
pub struct ConfigStore {
    path: PathBuf,
}

#[derive(Debug)]
pub struct LoadedConfig {
    pub config: Config,
    /// True when the live file was unusable and we fell back.
    pub degraded: bool,
    pub reason: Option<String>,
}

impl ConfigStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("config.json"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load with fallback chain: live file → newest valid backup → default.
    /// A *missing* live file is a clean first run (not degraded); only an
    /// unreadable/corrupt one triggers fallback.
    pub fn load(&self) -> Result<LoadedConfig, StoreError> {
        match read_config(&self.path) {
            Ok(config) => Ok(LoadedConfig {
                config,
                degraded: false,
                reason: None,
            }),
            Err(StoreError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(LoadedConfig {
                    config: Config::default(),
                    degraded: false,
                    reason: None,
                })
            }
            Err(live_err) => {
                let backup = self.newest_valid_backup();
                match backup {
                    Some((path, config)) => Ok(LoadedConfig {
                        config,
                        degraded: true,
                        reason: Some(format!(
                            "config.json unreadable ({live_err}); restored from {}",
                            path.display()
                        )),
                    }),
                    None => Ok(LoadedConfig {
                        config: Config::default(),
                        degraded: true,
                        reason: Some(format!(
                            "config.json unreadable ({live_err}); no valid backup; using defaults"
                        )),
                    }),
                }
            }
        }
    }

    /// Save atomically: write temp file, rename over live, then rotate
    /// backups (previous live becomes backup N-1 … backup 0 dropped).
    pub fn save(&self, config: &Config) -> Result<(), StoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Back up the current live file (if any) before overwriting.
        if self.path.exists() {
            self.rotate_backups()?;
            fs::copy(&self.path, self.backup_path(0))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(config)?)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    fn backup_path(&self, n: usize) -> PathBuf {
        self.path.with_extension(format!("json.bak{n}"))
    }

    fn rotate_backups(&self) -> Result<(), StoreError> {
        // Shift .bak3 → .bak4, … .bak0 → .bak1; drop oldest beyond MAX_BACKUPS.
        for n in (0..MAX_BACKUPS - 1).rev() {
            let from = self.backup_path(n);
            let to = self.backup_path(n + 1);
            if from.exists() {
                fs::copy(&from, &to)?;
            }
        }
        // Remove the overflow slot if present.
        let overflow = self.backup_path(MAX_BACKUPS);
        if overflow.exists() {
            let _ = fs::remove_file(overflow);
        }
        Ok(())
    }

    fn newest_valid_backup(&self) -> Option<(PathBuf, Config)> {
        (0..MAX_BACKUPS).find_map(|n| {
            let p = self.backup_path(n);
            read_config(&p).ok().map(|c| (p, c))
        })
    }
}

fn read_config(path: &Path) -> Result<Config, StoreError> {
    let raw = fs::read_to_string(path)?;
    let config: Config = serde_json::from_str(&raw)?;
    if config.schema_version > linkfyr_ipc::API_VERSION {
        return Err(StoreError::Serde(serde::de::Error::custom(format!(
            "config schema {} newer than supported {}",
            config.schema_version,
            linkfyr_ipc::API_VERSION
        ))));
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use linkfyr_ipc::{Preferences, Theme};

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("linkfyr-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    fn cfg(theme: Theme) -> Config {
        Config {
            schema_version: 1,
            preferences: Preferences {
                theme,
                ..Preferences::default()
            },
        }
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempdir("roundtrip");
        let store = ConfigStore::new(&dir);
        store.save(&cfg(Theme::Light)).expect("save");
        let loaded = store.load().expect("load");
        assert!(!loaded.degraded);
        assert_eq!(loaded.config.preferences.theme, Theme::Light);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupted_live_file_restores_from_backup() {
        let dir = tempdir("corrupt");
        let store = ConfigStore::new(&dir);
        store.save(&cfg(Theme::Dark)).expect("save v1");
        store.save(&cfg(Theme::Light)).expect("save v2");

        // Corrupt the live file.
        fs::write(store.path(), "{ not json").expect("corrupt");

        let loaded = store.load().expect("load");
        assert!(loaded.degraded);
        assert!(loaded.reason.as_deref().unwrap().contains("restored from"));
        // The most recent backup (v1 = Dark) wins.
        assert_eq!(loaded.config.preferences.theme, Theme::Dark);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn total_corruption_falls_back_to_defaults() {
        let dir = tempdir("defaults");
        let store = ConfigStore::new(&dir);
        fs::write(store.path(), "garbage").expect("write garbage");

        let loaded = store.load().expect("load");
        assert!(loaded.degraded);
        assert_eq!(loaded.config, Config::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_a_clean_first_run() {
        let dir = tempdir("firstrun");
        let store = ConfigStore::new(&dir);
        let loaded = store.load().expect("load");
        assert!(!loaded.degraded, "first run is not an error condition");
        assert_eq!(loaded.config, Config::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn backups_rotate_and_are_bounded() {
        let dir = tempdir("rotate");
        let store = ConfigStore::new(&dir);
        for i in 0..8 {
            store
                .save(&cfg(if i % 2 == 0 {
                    Theme::Dark
                } else {
                    Theme::Light
                }))
                .expect("save");
        }
        let backups = fs::read_dir(&dir)
            .expect("readdir")
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.contains(".bak")
            })
            .count();
        assert!(
            backups <= MAX_BACKUPS,
            "expected ≤{MAX_BACKUPS} backups, found {backups}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn future_schema_version_is_rejected_and_backups_save_us() {
        let dir = tempdir("future");
        let store = ConfigStore::new(&dir);
        store.save(&cfg(Theme::Dark)).expect("save");

        let future = r#"{"schemaVersion":99,"preferences":{"theme":"light","expertMode":false,"animation":"full","localOnly":true}}"#;
        fs::write(store.path(), future).expect("write future");

        let loaded = store.load().expect("load");
        assert!(loaded.degraded, "future schema must degrade, not crash");
        assert_eq!(loaded.config.preferences.theme, Theme::Dark);
        let _ = fs::remove_dir_all(&dir);
    }
}
