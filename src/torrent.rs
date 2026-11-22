#![allow(dead_code)]

#[cfg(feature = "torrent")]
mod inner {
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use tracing::{debug, error, info, warn};

use librqbit::http_api::{HttpApi, HttpApiOptions};
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, Api, ListenerOptions, Session, SessionOptions,
};
use librqbit_dualstack_sockets::{BindOpts, TcpListener};

use crate::types::{StreamStatus, TorrentFileInfo};

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct ActiveStreamInfo {
    torrent_id: usize,
    file_idx: usize,
    title: String,
    file_rel_path: PathBuf,
    file_len: u64,
}

/// Embedded BitTorrent streaming engine utilizing `librqbit` and its internal HTTP API server.
pub struct TorrentEngine {
    cache_dir: PathBuf,
    auto_clean_on_exit: bool,
    session: Arc<Session>,
    api: Api,
    http_port: u16,
    active_streams: Arc<RwLock<HashMap<usize, ActiveStreamInfo>>>,
}

impl TorrentEngine {
    /// Initialized with `cache_dir: std::path::PathBuf`, `auto_clean_on_exit: bool`.
    ///
    /// Spawns a `librqbit::Session` with `SessionOptions`, and starts the HTTP streaming
    /// server on `127.0.0.1:0` (ephemeral port) so video streaming is immediately accessible.
    pub async fn new(cache_dir: PathBuf, auto_clean_on_exit: bool) -> Result<Self, String> {
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Failed to create torrent cache directory {:?}: {e}", cache_dir))?;

        let mut session_opts = SessionOptions::default();
        session_opts.fastresume = true;
        session_opts.listen = Some(ListenerOptions {
            listen_addr: "0.0.0.0:0"
                .parse()
                .unwrap_or_else(|_| ([0, 0, 0, 0], 0).into()),
            ipv4_only: true,
            ..Default::default()
        });

        let session = Session::new_with_opts(cache_dir.clone(), session_opts)
            .await
            .map_err(|e| format!("Failed to initialize librqbit session: {e}"))?;

        let api = Api::new(session.clone(), None, None);
        let http_api = HttpApi::new(
            api.clone(),
            Some(HttpApiOptions {
                read_only: false,
                basic_auth: None,
                allow_create: true,
                max_upload_body_size: None,
                ..Default::default()
            }),
        );

        let listener = TcpListener::bind_tcp(
            "127.0.0.1:0"
                .parse()
                .map_err(|e| format!("Invalid bind address: {e}"))?,
            BindOpts {
                request_dualstack: false,
                reuseport: false,
                device: None,
            },
        )
        .map_err(|e| format!("Failed to bind HTTP streaming listener: {e}"))?;

        let http_port = listener.bind_addr().port();
        info!("Embedded Torrent HTTP streaming server started on 127.0.0.1:{http_port}");

        tokio::spawn(async move {
            if let Err(e) = http_api.make_http_api_and_run(listener, None).await {
                error!("Torrent HTTP streaming server terminated with error: {e:#}");
            }
        });

        Ok(Self {
            cache_dir,
            auto_clean_on_exit,
            session,
            api,
            http_port,
            active_streams: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Synchronously construct `TorrentEngine` on a current or newly created Tokio runtime.
    pub fn new_sync(cache_dir: PathBuf, auto_clean_on_exit: bool) -> Result<Self, String> {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(Self::new(cache_dir, auto_clean_on_exit)))
        } else {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| format!("Failed to initialize tokio runtime: {e}"))?;
            rt.block_on(Self::new(cache_dir, auto_clean_on_exit))
        }
    }

    /// Returns the local HTTP stream URL: `http://127.0.0.1:{port}/torrents/{id}/stream/{file_idx}`
    pub fn get_stream_url(&self, torrent_id: usize, file_idx: usize) -> String {
        format!(
            "http://127.0.0.1:{}/torrents/{}/stream/{}",
            self.http_port, torrent_id, file_idx
        )
    }

    /// Returns the active HTTP API port.
    pub fn http_port(&self) -> u16 {
        self.http_port
    }

    /// Access the underlying `librqbit::Api` instance.
    pub fn api(&self) -> &Api {
        &self.api
    }

    /// Access the underlying `librqbit::Session` instance.
    pub fn session(&self) -> &Arc<Session> {
        &self.session
    }

    /// Adds a torrent via URL, magnet link, or bare 40-character infohash, discovers files,
    /// and configures sequential streaming.
    ///
    /// - If single `.mkv` file: automatically picks it.
    /// - If multiple `.mkv` files (batch torrent) and `selected_file_idx` is None: returns `StreamStatus::SelectingBatch { id, files }`.
    /// - Once file index is selected: configures sequential streaming for that file index and returns `StreamStatus::Streaming { id, file_idx, title, port, ... }`.
    pub async fn add_torrent_stream(
        &self,
        torrent_url_or_hash: &str,
        selected_file_idx: Option<usize>,
    ) -> Result<StreamStatus, String> {
        let input = torrent_url_or_hash.trim();
        if input.is_empty() {
            return Err("Torrent target URL/hash cannot be empty".to_string());
        }

        let magnet_str;
        let target = if input.starts_with("magnet:")
            || input.starts_with("http://")
            || input.starts_with("https://")
        {
            input
        } else if input.len() == 40 && input.chars().all(|c| c.is_ascii_hexdigit()) {
            magnet_str = format!("magnet:?xt=urn:btih:{}", input);
            &magnet_str
        } else if input.len() == 32 && input.chars().all(|c| c.is_ascii_alphanumeric()) {
            magnet_str = format!("magnet:?xt=urn:btih:{}", input);
            &magnet_str
        } else {
            input
        };

        debug!("Adding torrent stream with target: {target}");

        let add = AddTorrent::from_cli_argument(target)
            .or_else(|_| Ok::<_, anyhow::Error>(AddTorrent::from_url(target)))
            .map_err(|e| format!("Failed to parse torrent input '{target}': {e}"))?;

        let mut opts = AddTorrentOptions::default();
        opts.overwrite = true;
        if let Some(idx) = selected_file_idx {
            opts.only_files = Some(vec![idx]);
        }

        let add_response = self
            .session
            .add_torrent(add, Some(opts))
            .await
            .map_err(|e| format!("Failed to add torrent to session: {e}"))?;

        let (id, handle) = match add_response {
            AddTorrentResponse::Added(id, handle) => (id, handle),
            AddTorrentResponse::AlreadyManaged(id, handle) => (id, handle),
            AddTorrentResponse::ListOnly(_) => {
                return Err("Torrent was added in list-only mode".to_string());
            }
        };

        // Wait briefly for torrent metadata to resolve
        let init_timeout = Duration::from_secs(15);
        let _ = tokio::time::timeout(init_timeout, handle.wait_until_initialized()).await;

        let metadata_arc = handle.metadata.load();
        let metadata = match metadata_arc.as_ref() {
            Some(m) => m,
            None => {
                let title = handle.name().unwrap_or_else(|| format!("Torrent #{id}"));
                return Ok(StreamStatus::DownloadingMetadata { title });
            }
        };

        // Enumerate and classify all files
        let mut all_files = Vec::new();
        let mut mkv_files = Vec::new();

        for (idx, fi) in metadata.file_infos.iter().enumerate() {
            let rel_path_str = fi.relative_filename.to_string_lossy().to_string();
            let is_mkv = fi
                .relative_filename
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("mkv"))
                .unwrap_or(false);

            let file_info = TorrentFileInfo {
                index: idx,
                path: rel_path_str,
                size_bytes: fi.len,
                is_mkv,
            };

            if is_mkv {
                mkv_files.push(file_info.clone());
            }
            all_files.push(file_info);
        }

        if all_files.is_empty() {
            return Err("Torrent contains no files".to_string());
        }

        // Determine which file index to stream
        let file_idx = match selected_file_idx {
            Some(idx) => {
                if idx >= metadata.file_infos.len() {
                    return Err(format!(
                        "Selected file index {idx} out of range (total files: {})",
                        metadata.file_infos.len()
                    ));
                }
                idx
            }
            None => {
                if mkv_files.len() > 1 {
                    info!(
                        "Discovered batch torrent #{id} with {} MKV files",
                        mkv_files.len()
                    );
                    return Ok(StreamStatus::SelectingBatch {
                        id,
                        files: mkv_files,
                    });
                } else if mkv_files.len() == 1 {
                    mkv_files[0].index
                } else if all_files.len() > 1 {
                    return Ok(StreamStatus::SelectingBatch {
                        id,
                        files: all_files,
                    });
                } else {
                    all_files[0].index
                }
            }
        };

        // Configure sequential download priority for the chosen file index
        let only_set: HashSet<usize> = [file_idx].into_iter().collect();
        let _ = self
            .api
            .api_torrent_action_update_only_files(id.into(), &only_set)
            .await;

        let title = handle.name().unwrap_or_else(|| format!("Torrent #{id}"));
        let fi = &metadata.file_infos[file_idx];
        let file_rel_path = fi.relative_filename.clone();
        let file_len = fi.len;

        // Store active stream metadata for status polling
        {
            let mut streams = self.active_streams.write();
            streams.insert(
                id,
                ActiveStreamInfo {
                    torrent_id: id,
                    file_idx,
                    title: title.clone(),
                    file_rel_path: file_rel_path.clone(),
                    file_len,
                },
            );
        }

        let stats = handle.stats();
        let (is_complete, progress_pct) = compute_file_progress(&stats, file_idx, file_len);
        let speed_bytes = stats
            .live
            .as_ref()
            .map(|l| l.download_speed.as_bytes())
            .unwrap_or(0);

        if is_complete {
            let full_path = handle.output_folder().join(&file_rel_path);
            if full_path.exists() {
                return Ok(StreamStatus::Completed {
                    file_path: full_path,
                    title,
                });
            }
        }

        Ok(StreamStatus::Streaming {
            id,
            file_idx,
            title,
            port: self.http_port,
            progress_pct,
            speed_bytes,
            is_complete,
        })
    }

    /// Checks download progress, piece completion, speed, and whether the torrent file has finished downloading.
    pub fn poll_status(&self, torrent_id: usize) -> StreamStatus {
        let handle = match self.session.get(torrent_id.into()) {
            Some(h) => h,
            None => {
                return StreamStatus::Error(format!("Torrent #{torrent_id} not found in session"));
            }
        };

        let stats = handle.stats();
        if let Some(err) = stats.error {
            return StreamStatus::Error(err);
        }

        let metadata_arc = handle.metadata.load();
        let metadata = match metadata_arc.as_ref() {
            Some(m) => m,
            None => {
                let title = handle
                    .name()
                    .unwrap_or_else(|| format!("Torrent #{torrent_id}"));
                return StreamStatus::DownloadingMetadata { title };
            }
        };

        let streams = self.active_streams.read();
        let active = match streams.get(&torrent_id) {
            Some(a) => a,
            None => {
                // If no active file index was selected yet, return batch selection list
                let mut mkv_files = Vec::new();
                for (idx, fi) in metadata.file_infos.iter().enumerate() {
                    let is_mkv = fi
                        .relative_filename
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("mkv"))
                        .unwrap_or(false);
                    if is_mkv {
                        mkv_files.push(TorrentFileInfo {
                            index: idx,
                            path: fi.relative_filename.to_string_lossy().to_string(),
                            size_bytes: fi.len,
                            is_mkv,
                        });
                    }
                }
                return StreamStatus::SelectingBatch {
                    id: torrent_id,
                    files: mkv_files,
                };
            }
        };

        let (is_complete, progress_pct) =
            compute_file_progress(&stats, active.file_idx, active.file_len);
        let speed_bytes = stats
            .live
            .as_ref()
            .map(|l| l.download_speed.as_bytes())
            .unwrap_or(0);

        if is_complete {
            let full_path = handle.output_folder().join(&active.file_rel_path);
            if full_path.exists() {
                return StreamStatus::Completed {
                    file_path: full_path,
                    title: active.title.clone(),
                };
            }
        }

        StreamStatus::Streaming {
            id: torrent_id,
            file_idx: active.file_idx,
            title: active.title.clone(),
            port: self.http_port,
            progress_pct,
            speed_bytes,
            is_complete,
        }
    }

    /// Performs clean shutdown and removes temporary cache files if `auto_clean_on_exit` is enabled.
    pub fn cleanup(&self) {
        if self.auto_clean_on_exit && self.cache_dir.exists() {
            info!(
                "Cleaning up torrent cache directory on shutdown: {:?}",
                self.cache_dir
            );
            if let Err(e) = std::fs::remove_dir_all(&self.cache_dir) {
                warn!(
                    "Failed to remove torrent cache directory {:?}: {e}",
                    self.cache_dir
                );
            }
        }
    }
}

impl Drop for TorrentEngine {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Computes completion state and percentage for a specific file index
fn compute_file_progress(
    stats: &librqbit::TorrentStats,
    file_idx: usize,
    file_len: u64,
) -> (bool, f32) {
    if stats.finished {
        return (true, 100.0);
    }

    if let Some(&progress_bytes) = stats.file_progress.get(file_idx) {
        if file_len > 0 {
            let pct = (progress_bytes as f32 / file_len as f32) * 100.0;
            let complete = progress_bytes >= file_len;
            return (complete, pct.min(100.0));
        }
    }

    if stats.total_bytes > 0 {
        let pct = (stats.progress_bytes as f32 / stats.total_bytes as f32) * 100.0;
        (false, pct.min(100.0))
    } else {
        (false, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_file_progress() {
        let stats_finished = librqbit::TorrentStats {
            state: librqbit::TorrentStatsState::Live,
            file_progress: vec![1000, 2000],
            error: None,
            progress_bytes: 3000,
            uploaded_bytes: 0,
            total_bytes: 3000,
            finished: true,
            live: None,
        };
        let (is_complete, pct) = compute_file_progress(&stats_finished, 0, 1000);
        assert!(is_complete);
        assert_eq!(pct, 100.0);

        let stats_partial = librqbit::TorrentStats {
            state: librqbit::TorrentStatsState::Live,
            file_progress: vec![500, 0],
            error: None,
            progress_bytes: 500,
            uploaded_bytes: 0,
            total_bytes: 2000,
            finished: false,
            live: None,
        };
        let (is_complete, pct) = compute_file_progress(&stats_partial, 0, 1000);
        assert!(!is_complete);
        assert_eq!(pct, 50.0);
    }

    #[tokio::test]
    async fn test_torrent_engine_init_and_url() {
        let temp_dir = std::env::temp_dir().join(format!("vesper-test-engine-{}", rand_name()));
        let engine = TorrentEngine::new(temp_dir.clone(), true)
            .await
            .expect("TorrentEngine initialization should succeed");

        let port = engine.http_port();
        assert!(port > 0, "HTTP port should be a non-zero ephemeral port");

        let url = engine.get_stream_url(42, 3);
        assert_eq!(url, format!("http://127.0.0.1:{port}/torrents/42/stream/3"));

        let status = engine.poll_status(9999);
        match status {
            StreamStatus::Error(msg) => {
                assert!(msg.contains("not found"));
            }
            other => panic!("Expected StreamStatus::Error, got {:?}", other),
        }

        engine.cleanup();
        assert!(
            !temp_dir.exists(),
            "Cache directory should be removed on cleanup when auto_clean_on_exit is true"
        );
    }

    fn rand_name() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }
}
}

#[cfg(feature = "torrent")]
pub use inner::*;

#[cfg(not(feature = "torrent"))]
mod stub {
    use std::path::PathBuf;
    use crate::types::StreamStatus;

    pub struct TorrentEngine;

    impl TorrentEngine {
        pub async fn new(_cache_dir: PathBuf, _auto_clean_on_exit: bool) -> Result<Self, String> {
            Ok(Self)
        }

        pub fn http_port(&self) -> u16 {
            0
        }

        pub async fn add_torrent_stream(&self, _torrent_input: &str, _file_idx: Option<usize>) -> Result<StreamStatus, String> {
            Err("Torrent streaming requires compiling with '--features torrent'".to_string())
        }

        pub fn poll_status(&self, _torrent_id: usize) -> StreamStatus {
            StreamStatus::Idle
        }

        pub fn get_stream_url(&self, _torrent_id: usize, _file_idx: usize) -> String {
            String::new()
        }

        pub fn cleanup(&self) {}
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn test_torrent_stub_without_feature() {
            let engine = TorrentEngine::new(PathBuf::from("dummy"), false).await.unwrap();
            assert_eq!(engine.http_port(), 0);
            assert!(engine.add_torrent_stream("test", None).await.is_err());
        }
    }
}

#[cfg(not(feature = "torrent"))]
pub use stub::*;

