#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application configuration for Vesper
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VesperConfig {
    #[serde(default = "default_nyaa_base_url")]
    pub nyaa_base_url: String,

    #[serde(default = "default_cache_dir")]
    pub cache_dir: PathBuf,

    #[serde(default = "default_auto_clean_cache")]
    pub auto_clean_cache: bool,

    #[serde(default = "default_download_dir")]
    pub download_dir: PathBuf,

    #[serde(default = "default_keep_streamed_files")]
    pub keep_streamed_files: bool,

    #[serde(default = "default_sub_lang")]
    pub default_sub_lang: String,

    #[serde(default = "default_volume")]
    pub volume: f64,

    #[serde(default = "default_auto_sync_trackers")]
    pub auto_sync_trackers: bool,

    #[serde(default = "default_tracker_list_url")]
    pub tracker_list_url: String,

    #[serde(default)]
    pub custom_trackers: Vec<String>,

    #[serde(default)]
    pub cached_remote_trackers: Vec<String>,
}

pub type Config = VesperConfig;

fn default_nyaa_base_url() -> String {
    "https://nyaa.si".to_string()
}

fn default_cache_dir() -> PathBuf {
    std::env::temp_dir().join("vesper-cache")
}

fn default_auto_clean_cache() -> bool {
    true
}

fn default_download_dir() -> PathBuf {
    if let Ok(profile) = std::env::var("USERPROFILE") {
        PathBuf::from(profile).join("Downloads").join("Vesper")
    } else {
        std::env::temp_dir().join("Vesper-Downloads")
    }
}

fn default_keep_streamed_files() -> bool {
    false
}

fn default_sub_lang() -> String {
    "eng".to_string()
}

fn default_volume() -> f64 {
    100.0
}

pub fn default_auto_sync_trackers() -> bool {
    true
}

pub fn default_tracker_list_url() -> String {
    "https://raw.githubusercontent.com/ngosang/trackerslist/master/trackers_best.txt".to_string()
}

impl Default for VesperConfig {
    fn default() -> Self {
        Self {
            nyaa_base_url: default_nyaa_base_url(),
            cache_dir: default_cache_dir(),
            auto_clean_cache: default_auto_clean_cache(),
            download_dir: default_download_dir(),
            keep_streamed_files: default_keep_streamed_files(),
            default_sub_lang: default_sub_lang(),
            volume: default_volume(),
            auto_sync_trackers: default_auto_sync_trackers(),
            tracker_list_url: default_tracker_list_url(),
            custom_trackers: Vec::new(),
            cached_remote_trackers: Vec::new(),
        }
    }
}

/// Locates the `vesper.toml` file path.
///
/// Priority:
/// 1. `vesper.toml` next to the executable if it exists.
/// 2. `vesper.toml` in the current working directory if it exists.
/// 3. If neither exists yet, defaults to `vesper.toml` next to the executable
///    (or current working directory if current exe cannot be determined).
pub fn config_path() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let candidate = parent.join("vesper.toml");
            if candidate.exists() {
                return candidate;
            }
        }
    }

    let cwd_candidate = PathBuf::from("vesper.toml");
    if cwd_candidate.exists() {
        return cwd_candidate;
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            return parent.join("vesper.toml");
        }
    }

    PathBuf::from("vesper.toml")
}

/// Loads configuration from `vesper.toml`.
/// Returns default configuration if the file does not exist or fails to parse.
pub fn load() -> VesperConfig {
    let path = config_path();
    if !path.exists() {
        tracing::info!("Configuration file not found at {:?}, using defaults", path);
        return VesperConfig::default();
    }

    match std::fs::read_to_string(&path) {
        Ok(contents) => match toml::from_str::<VesperConfig>(&contents) {
            Ok(config) => {
                tracing::info!("Loaded configuration from {:?}", path);
                config
            }
            Err(e) => {
                tracing::warn!("Failed to parse config at {:?}: {e}. Using defaults.", path);
                VesperConfig::default()
            }
        },
        Err(e) => {
            tracing::warn!("Failed to read config at {:?}: {e}. Using defaults.", path);
            VesperConfig::default()
        }
    }
}

/// Saves the configuration struct into `vesper.toml`.
pub fn save(config: &VesperConfig) -> Result<()> {
    config.save()
}

impl VesperConfig {
    /// Load configuration from `vesper.toml`.
    pub fn load() -> Self {
        load()
    }

    /// Save configuration to `vesper.toml`.
    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory {:?}", parent))?;
            }
        }

        let serialized = toml::to_string_pretty(self)
            .context("Failed to serialize VesperConfig to TOML")?;

        std::fs::write(&path, serialized)
            .with_context(|| format!("Failed to write configuration to {:?}", path))?;

        tracing::info!("Configuration saved to {:?}", path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = VesperConfig::default();
        assert_eq!(config.nyaa_base_url, "https://nyaa.si");
        assert_eq!(config.default_sub_lang, "eng");
        assert_eq!(config.volume, 100.0);
        assert!(config.auto_clean_cache);
        assert!(config.cache_dir.ends_with("vesper-cache"));
        assert!(!config.keep_streamed_files);
        assert!(config.download_dir.ends_with("Vesper") || config.download_dir.ends_with("Vesper-Downloads"));
        assert!(config.auto_sync_trackers);
        assert!(config.tracker_list_url.contains("ngosang/trackerslist"));
        assert!(config.custom_trackers.is_empty());
        assert!(config.cached_remote_trackers.is_empty());
    }

    #[test]
    fn test_serialize_deserialize() {
        let config = VesperConfig::default();
        let toml_str = toml::to_string(&config).expect("serialize");
        let parsed: VesperConfig = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(config, parsed);
    }
}
