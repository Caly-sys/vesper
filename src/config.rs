#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    false
}

fn default_download_dir() -> PathBuf {
    if let Ok(profile) = std::env::var("USERPROFILE") {
        PathBuf::from(profile).join("Downloads").join("Vesper")
    } else {
        std::env::temp_dir().join("Vesper-Downloads")
    }
}

fn default_keep_streamed_files() -> bool {
    true
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

/// Locates the Vesper data directory (where state and session cache reside).
pub fn data_dir() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            return parent.to_path_buf();
        }
    }
    PathBuf::from(".")
}

/// Locates the `vesper_state.json` registry path.
pub fn state_file_path() -> PathBuf {
    data_dir().join("vesper_state.json")
}

/// Locates the `librqbit` persistent session state folder.
pub fn rqbit_state_dir() -> PathBuf {
    data_dir().join("vesper_rqbit_state")
}

/// Record of a completed torrent on disk
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletedTorrent {
    pub title: String,
    pub local_path: PathBuf,
    #[serde(default, rename = "downloaded_file_indices", alias = "file_indices")]
    pub file_indices: Vec<usize>,
    #[serde(default)]
    pub completed_at: u64,
    #[serde(default)]
    pub total_video_files: usize,
}

impl CompletedTorrent {
    pub fn is_batch(&self) -> bool {
        self.total_video_files > 1 || self.file_indices.len() > 1
    }

    pub fn is_all_episodes_done(&self) -> bool {
        if self.total_video_files <= 1 {
            true
        } else {
            self.file_indices.len() >= self.total_video_files
        }
    }
}

/// Watch position progress marker
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchProgress {
    pub position_secs: f64,
    pub duration_secs: f64,
}

/// Lightweight persistent state saved to `vesper_state.json`
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VesperState {
    #[serde(default)]
    pub completed_torrents: HashMap<String, CompletedTorrent>,
    #[serde(default)]
    pub watch_progress: HashMap<String, WatchProgress>,
}

impl VesperState {
    pub fn load() -> Self {
        let path = state_file_path();
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(e) => {
                tracing::warn!("Failed to read state file {:?}: {e}", path);
                Self::default()
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = state_file_path();
        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize VesperState to JSON")?;
        std::fs::write(&path, json)
            .with_context(|| format!("Failed to write state to {:?}", path))?;
        Ok(())
    }

    pub fn normalize_key(key: &str) -> String {
        key.trim().to_lowercase().replace('/', "\\")
    }

    pub fn record_completed(
        &mut self,
        info_hash: &str,
        title: &str,
        local_path: PathBuf,
        file_indices: Vec<usize>,
        total_video_files: usize,
    ) {
        let key = info_hash.trim().to_lowercase();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let entry = self.completed_torrents.entry(key).or_insert_with(|| CompletedTorrent {
            title: title.to_string(),
            local_path: local_path.clone(),
            file_indices: Vec::new(),
            completed_at: now,
            total_video_files,
        });
        entry.title = title.to_string();
        entry.local_path = local_path;
        entry.completed_at = now;
        if total_video_files > 0 {
            entry.total_video_files = total_video_files;
        }
        for idx in file_indices {
            if !entry.file_indices.contains(&idx) {
                entry.file_indices.push(idx);
            }
        }
        entry.file_indices.sort_unstable();
        let _ = self.save();
    }

    pub fn record_progress(&mut self, file_or_hash: &str, pos: f64, dur: f64) {
        if pos <= 0.0 && dur <= 0.0 {
            return;
        }
        let key = Self::normalize_key(file_or_hash);
        self.watch_progress.insert(
            key,
            WatchProgress {
                position_secs: pos,
                duration_secs: dur,
            },
        );
        let _ = self.save();
    }

    pub fn get_progress(&self, file_or_hash: &str) -> Option<&WatchProgress> {
        let key = Self::normalize_key(file_or_hash);
        self.watch_progress.get(&key)
    }

    pub fn is_downloaded(&self, info_hash: &str) -> Option<&CompletedTorrent> {
        let key = info_hash.trim().to_lowercase();
        self.completed_torrents.get(&key).filter(|c| c.local_path.exists())
    }

    pub fn remove_completed(&mut self, key_or_path: &str) -> Option<CompletedTorrent> {
        let norm = Self::normalize_key(key_or_path);
        let found_key = self.completed_torrents.iter().find_map(|(k, v)| {
            if Self::normalize_key(k) == norm
                || Self::normalize_key(&v.local_path.to_string_lossy()) == norm
                || v.local_path == std::path::Path::new(key_or_path)
            {
                Some(k.clone())
            } else {
                None
            }
        });

        let target_key = found_key.unwrap_or_else(|| key_or_path.trim().to_lowercase());
        let removed = self.completed_torrents.remove(&target_key);

        let norm_hash = Self::normalize_key(&target_key);
        let norm_path = removed.as_ref().map(|c| Self::normalize_key(&c.local_path.to_string_lossy()));

        self.watch_progress.retain(|k, _| {
            if k == &norm || k == &norm_hash {
                return false;
            }
            if let Some(ref p) = norm_path {
                let norm_prefix = if p.ends_with('\\') {
                    p.clone()
                } else {
                    format!("{p}\\")
                };
                if k == p || k.starts_with(&norm_prefix) {
                    return false;
                }
            }
            true
        });

        let _ = self.save();
        removed
    }
}

/// Recursively computes the total size in bytes of a file or directory.
pub fn compute_disk_size(path: &std::path::Path) -> u64 {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return 0,
    };
    if meta.is_file() {
        meta.len()
    } else if meta.is_dir() {
        let mut total = 0;
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                total += compute_disk_size(&entry.path());
            }
        }
        total
    } else {
        0
    }
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
        assert!(!config.auto_clean_cache);
        assert!(config.cache_dir.ends_with("vesper-cache"));
        assert!(config.keep_streamed_files);
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

    #[test]
    fn test_vesper_state_methods() {
        let mut state = VesperState::default();
        state.record_progress("C:/Videos/Ep1.mkv", 120.5, 1440.0);
        let wp = state.get_progress("c:\\videos\\ep1.mkv").expect("progress found");
        assert_eq!(wp.position_secs, 120.5);
        assert_eq!(wp.duration_secs, 1440.0);

        state.record_completed(
            "0123456789abcdef0123456789abcdef01234567",
            "Episode 1",
            PathBuf::from("C:\\fake_file.mkv"),
            vec![0],
            12,
        );
        let ct = state.completed_torrents.get("0123456789abcdef0123456789abcdef01234567").unwrap();
        assert_eq!(ct.file_indices, vec![0]);
        assert_eq!(ct.total_video_files, 12);
        assert!(ct.is_batch());
        assert!(!ct.is_all_episodes_done());

        state.record_completed(
            "0123456789abcdef0123456789abcdef01234567",
            "Episode 1",
            PathBuf::from("C:\\fake_file.mkv"),
            vec![1],
            12,
        );
        let ct2 = state.completed_torrents.get("0123456789abcdef0123456789abcdef01234567").unwrap();
        assert_eq!(ct2.file_indices, vec![0, 1]);
    }

    #[test]
    fn test_compute_disk_size_and_remove_completed() {
        let temp_dir = std::env::temp_dir().join(format!(
            "vesper_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let file1 = temp_dir.join("test1.bin");
        let sub = temp_dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let file2 = sub.join("test2.bin");
        std::fs::write(&file1, vec![0u8; 120]).unwrap();
        std::fs::write(&file2, vec![0u8; 330]).unwrap();

        assert_eq!(compute_disk_size(&file1), 120);
        assert_eq!(compute_disk_size(&file2), 330);
        assert_eq!(compute_disk_size(&temp_dir), 450);

        let mut state = VesperState::default();
        let hash = "0123456789abcdef0123456789abcdef01234567";
        state.record_completed(hash, "Show", temp_dir.clone(), vec![0], 2);
        state.record_progress(hash, 42.0, 100.0);
        state.record_progress(&file1.to_string_lossy(), 15.0, 100.0);

        assert!(state.completed_torrents.contains_key(hash));
        assert!(state.get_progress(hash).is_some());
        assert!(state.get_progress(&file1.to_string_lossy()).is_some());

        let removed = state.remove_completed(hash);
        assert!(removed.is_some());
        assert!(!state.completed_torrents.contains_key(hash));
        assert!(state.get_progress(hash).is_none());
        assert!(state.get_progress(&file1.to_string_lossy()).is_none());

        state.record_completed(hash, "Show 2", file2.clone(), vec![0], 1);
        state.record_progress(&file2.to_string_lossy(), 10.0, 50.0);
        assert!(state.completed_torrents.contains_key(hash));
        assert!(state.get_progress(&file2.to_string_lossy()).is_some());

        let removed2 = state.remove_completed(&file2.to_string_lossy());
        assert!(removed2.is_some());
        assert!(!state.completed_torrents.contains_key(hash));
        assert!(state.get_progress(&file2.to_string_lossy()).is_none());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
