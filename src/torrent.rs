#![allow(dead_code)]

#[cfg(feature = "torrent")]
mod inner {
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use tracing::{error, info, warn};

use librqbit::http_api::{HttpApi, HttpApiOptions};
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, Api, ListenerOptions, Session, SessionOptions,
    TorrentStatsState,
};
use librqbit_dualstack_sockets::{BindOpts, TcpListener};

use crate::types::{StreamStatus, TorrentDownloadItem, TorrentFileInfo, TorrentTaskState};

#[derive(Debug, Clone)]
pub struct ManagedTaskInfo {
    pub torrent_id: usize,
    pub file_idx: Option<usize>,
    pub title: String,
    pub file_rel_path: Option<PathBuf>,
    pub file_len: u64,
    pub is_stream: bool,
    pub dest_dir: PathBuf,
}

/// Embedded BitTorrent streaming engine utilizing `librqbit` and its internal HTTP API server.
pub struct TorrentEngine {
    cache_dir: PathBuf,
    auto_clean_on_exit: bool,
    session: Arc<Session>,
    api: Api,
    http_port: u16,
    tasks: Arc<RwLock<HashMap<usize, ManagedTaskInfo>>>,
}

impl TorrentEngine {
    /// Initialized with `cache_dir: std::path::PathBuf`, `auto_clean_on_exit: bool`.
    pub async fn new(cache_dir: PathBuf, auto_clean_on_exit: bool) -> Result<Self, String> {
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Failed to create torrent cache directory {:?}: {e}", cache_dir))?;

        let mut session_opts = SessionOptions::default();
        if let Some(dht) = &mut session_opts.dht {
            dht.persistence = None;
        }
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
            tasks: Arc::new(RwLock::new(HashMap::new())),
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

    /// Internal parser for adding torrents from magnet URIs, info hashes, file paths, or URLs
    fn parse_torrent_input(input: &str) -> Result<AddTorrent<'static>, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("Torrent target URL/hash cannot be empty".to_string());
        }

        let path = Path::new(trimmed);
        if path.exists() && path.is_file() {
            let bytes = std::fs::read(path)
                .map_err(|e| format!("Failed to read local torrent file {:?}: {e}", path))?;
            return Ok(AddTorrent::from_bytes(bytes));
        }

        if (trimmed.len() == 40 && trimmed.chars().all(|c| c.is_ascii_hexdigit()))
            || (trimmed.len() == 32 && trimmed.chars().all(|c| c.is_ascii_alphanumeric()))
        {
            return Ok(AddTorrent::from_url(format!("magnet:?xt=urn:btih:{}", trimmed)));
        }

        Ok(AddTorrent::from_url(trimmed.to_string()))
    }

    /// Adds a torrent via URL, magnet link, or local file for sequential streaming into temp cache
    pub async fn add_torrent_stream(
        &self,
        torrent_url_or_hash: &str,
        selected_file_idx: Option<usize>,
    ) -> Result<StreamStatus, String> {
        let add = Self::parse_torrent_input(torrent_url_or_hash)?;

        let mut opts = AddTorrentOptions::default();
        opts.overwrite = true;
        opts.output_folder = Some(self.cache_dir.to_string_lossy().to_string());
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

        // Wait up to 10 seconds for initial metadata
        let _ = tokio::time::timeout(Duration::from_secs(10), handle.wait_until_initialized()).await;

        let metadata_arc = handle.metadata.load();
        let metadata = match metadata_arc.as_ref() {
            Some(m) => m,
            None => {
                let title = handle.name().unwrap_or_else(|| format!("Torrent #{id}"));
                {
                    let mut tasks = self.tasks.write();
                    tasks.insert(
                        id,
                        ManagedTaskInfo {
                            torrent_id: id,
                            file_idx: selected_file_idx,
                            title: title.clone(),
                            file_rel_path: None,
                            file_len: 0,
                            is_stream: true,
                            dest_dir: self.cache_dir.clone(),
                        },
                    );
                }
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
                .map(|e| e.eq_ignore_ascii_case("mkv") || e.eq_ignore_ascii_case("mp4") || e.eq_ignore_ascii_case("webm"))
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

        let file_idx = match selected_file_idx {
            Some(idx) => {
                if idx >= metadata.file_infos.len() {
                    return Err(format!("Selected file index {idx} out of range"));
                }
                idx
            }
            None => {
                if mkv_files.len() > 1 {
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

        {
            let mut tasks = self.tasks.write();
            tasks.insert(
                id,
                ManagedTaskInfo {
                    torrent_id: id,
                    file_idx: Some(file_idx),
                    title: title.clone(),
                    file_rel_path: Some(file_rel_path.clone()),
                    file_len,
                    is_stream: true,
                    dest_dir: self.cache_dir.clone(),
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

    /// Adds a torrent for full-speed background downloading into `dest_dir`
    pub async fn add_torrent_download(
        &self,
        torrent_url_or_hash: &str,
        dest_dir: &Path,
        selected_file_idx: Option<usize>,
    ) -> Result<usize, String> {
        let _ = std::fs::create_dir_all(dest_dir);
        let add = Self::parse_torrent_input(torrent_url_or_hash)?;

        let mut opts = AddTorrentOptions::default();
        opts.overwrite = true;
        opts.output_folder = Some(dest_dir.to_string_lossy().to_string());
        if let Some(idx) = selected_file_idx {
            opts.only_files = Some(vec![idx]);
        }

        let add_response = self
            .session
            .add_torrent(add, Some(opts))
            .await
            .map_err(|e| format!("Failed to add download to session: {e}"))?;

        let (id, handle) = match add_response {
            AddTorrentResponse::Added(id, handle) => (id, handle),
            AddTorrentResponse::AlreadyManaged(id, handle) => (id, handle),
            AddTorrentResponse::ListOnly(_) => {
                return Err("Torrent was added in list-only mode".to_string());
            }
        };

        let title = handle.name().unwrap_or_else(|| format!("Torrent #{id}"));
        let metadata_arc = handle.metadata.load();

        let (file_rel_path, file_len) = if let Some(m) = metadata_arc.as_ref() {
            let total_len = m.file_infos.iter().map(|f| f.len).sum::<u64>();
            if let Some(idx) = selected_file_idx {
                if let Some(fi) = m.file_infos.get(idx) {
                    (Some(fi.relative_filename.clone()), fi.len)
                } else {
                    (None, total_len)
                }
            } else if m.file_infos.len() == 1 {
                (Some(m.file_infos[0].relative_filename.clone()), m.file_infos[0].len)
            } else {
                (None, total_len)
            }
        } else {
            (None, 0)
        };

        {
            let mut tasks = self.tasks.write();
            tasks.insert(
                id,
                ManagedTaskInfo {
                    torrent_id: id,
                    file_idx: selected_file_idx,
                    title,
                    file_rel_path,
                    file_len,
                    is_stream: false,
                    dest_dir: dest_dir.to_path_buf(),
                },
            );
        }

        Ok(id)
    }

    /// Checks status for active stream playback
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

        let tasks = self.tasks.read();
        let active = match tasks.get(&torrent_id) {
            Some(a) => a,
            None => {
                let mut mkv_files = Vec::new();
                for (idx, fi) in metadata.file_infos.iter().enumerate() {
                    let is_mkv = fi
                        .relative_filename
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("mkv") || e.eq_ignore_ascii_case("mp4") || e.eq_ignore_ascii_case("webm"))
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

        let file_idx = active.file_idx.unwrap_or(0);
        let (is_complete, progress_pct) = compute_file_progress(&stats, file_idx, active.file_len);
        let speed_bytes = stats
            .live
            .as_ref()
            .map(|l| l.download_speed.as_bytes())
            .unwrap_or(0);

        if is_complete {
            if let Some(ref rel) = active.file_rel_path {
                let full_path = handle.output_folder().join(rel);
                if full_path.exists() {
                    return StreamStatus::Completed {
                        file_path: full_path,
                        title: active.title.clone(),
                    };
                }
            }
        }

        StreamStatus::Streaming {
            id: torrent_id,
            file_idx,
            title: active.title.clone(),
            port: self.http_port,
            progress_pct,
            speed_bytes,
            is_complete,
        }
    }

    /// Query and snapshot all active and completed torrent downloads for the UI
    pub fn poll_all_tasks(&self) -> Vec<TorrentDownloadItem> {
        let tasks = self.tasks.read().clone();
        let mut results = Vec::new();

        for (id, mut task) in tasks {
            let handle = match self.session.get(id.into()) {
                Some(h) => h,
                None => continue,
            };

            let stats = handle.stats();
            let metadata_arc = handle.metadata.load();

            // Lazy resolution of metadata if it was resolving
            if task.file_rel_path.is_none() {
                if let Some(m) = metadata_arc.as_ref() {
                    let total_len = m.file_infos.iter().map(|f| f.len).sum::<u64>();
                    let (rel, len) = if let Some(idx) = task.file_idx {
                        if let Some(fi) = m.file_infos.get(idx) {
                            (Some(fi.relative_filename.clone()), fi.len)
                        } else {
                            (None, total_len)
                        }
                    } else if m.file_infos.len() == 1 {
                        (Some(m.file_infos[0].relative_filename.clone()), m.file_infos[0].len)
                    } else {
                        (None, total_len)
                    };
                    task.file_rel_path = rel;
                    task.file_len = len;
                    task.title = handle.name().unwrap_or(task.title);
                }
            }

            let speed_bytes = stats
                .live
                .as_ref()
                .map(|l| l.download_speed.as_bytes())
                .unwrap_or(0);

            let peer_count = stats
                .live
                .as_ref()
                .map(|l| l.snapshot.peer_stats.live as usize)
                .unwrap_or(0);

            let (is_complete, pct) = if let Some(idx) = task.file_idx {
                compute_file_progress(&stats, idx, task.file_len)
            } else if stats.total_bytes > 0 {
                let p = (stats.progress_bytes as f32 / stats.total_bytes as f32) * 100.0;
                (stats.finished || stats.progress_bytes >= stats.total_bytes, p.min(100.0))
            } else {
                (false, 0.0)
            };

            let state = if let Some(ref err) = stats.error {
                TorrentTaskState::Error(err.clone())
            } else if matches!(stats.state, TorrentStatsState::Paused) {
                TorrentTaskState::Paused
            } else if metadata_arc.is_none() {
                TorrentTaskState::Resolving
            } else if is_complete {
                TorrentTaskState::Completed
            } else {
                TorrentTaskState::Downloading
            };

            let (downloaded_bytes, total_bytes) = if let Some(idx) = task.file_idx {
                let dl = stats.file_progress.get(idx).copied().unwrap_or(0);
                (dl, task.file_len)
            } else {
                (stats.progress_bytes, stats.total_bytes)
            };

            let output_path = if let Some(ref rel) = task.file_rel_path {
                handle.output_folder().join(rel)
            } else {
                handle.output_folder().to_path_buf()
            };

            results.push(TorrentDownloadItem {
                id,
                name: task.title,
                downloaded_bytes,
                total_bytes,
                progress_pct: pct,
                speed_bytes,
                peer_count,
                state,
                is_stream: task.is_stream,
                output_path,
            });
        }

        results.sort_by_key(|t| t.id);
        results
    }

    /// Pause downloading or seeding a torrent
    pub async fn pause_torrent(&self, id: usize) -> Result<(), String> {
        self.api
            .api_torrent_action_pause(id.into())
            .await
            .map(|_| ())
            .map_err(|e| format!("Failed to pause torrent #{id}: {e}"))
    }

    /// Resume downloading a torrent
    pub async fn resume_torrent(&self, id: usize) -> Result<(), String> {
        self.api
            .api_torrent_action_start(id.into())
            .await
            .map(|_| ())
            .map_err(|e| format!("Failed to resume torrent #{id}: {e}"))
    }

    /// Remove a torrent from session and optionally delete downloaded data
    pub async fn cancel_torrent(&self, id: usize, delete_files: bool) -> Result<(), String> {
        let res = if delete_files {
            self.api.api_torrent_action_delete(id.into()).await
        } else {
            self.api.api_torrent_action_forget(id.into()).await
        };

        {
            let mut tasks = self.tasks.write();
            tasks.remove(&id);
        }

        res.map(|_| ()).map_err(|e| format!("Failed to remove torrent #{id}: {e}"))
    }

    /// Performs clean shutdown and removes temporary cache files if `auto_clean_on_exit` is enabled
    pub fn cleanup(&self) {
        if self.auto_clean_on_exit && self.cache_dir.exists() {
            info!("Cleaning up torrent cache directory on shutdown: {:?}", self.cache_dir);
            if let Err(e) = std::fs::remove_dir_all(&self.cache_dir) {
                warn!("Failed to remove torrent cache directory {:?}: {e}", self.cache_dir);
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

        let tasks = engine.poll_all_tasks();
        assert!(tasks.is_empty());

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
    use std::path::{Path, PathBuf};
    use crate::types::{StreamStatus, TorrentDownloadItem};

    pub struct TorrentEngine;

    impl TorrentEngine {
        pub async fn new(_cache_dir: PathBuf, _auto_clean_on_exit: bool) -> Result<Self, String> {
            Ok(Self)
        }

        pub fn http_port(&self) -> u16 {
            0
        }

        pub async fn add_torrent_stream(
            &self,
            _torrent_input: &str,
            _file_idx: Option<usize>,
        ) -> Result<StreamStatus, String> {
            Err("Torrent streaming requires compiling with '--features torrent'".to_string())
        }

        pub async fn add_torrent_download(
            &self,
            _torrent_input: &str,
            _dest_dir: &Path,
            _file_idx: Option<usize>,
        ) -> Result<usize, String> {
            Err("Torrent downloading requires compiling with '--features torrent'".to_string())
        }

        pub fn poll_status(&self, _torrent_id: usize) -> StreamStatus {
            StreamStatus::Idle
        }

        pub fn poll_all_tasks(&self) -> Vec<TorrentDownloadItem> {
            Vec::new()
        }

        pub async fn pause_torrent(&self, _id: usize) -> Result<(), String> {
            Ok(())
        }

        pub async fn resume_torrent(&self, _id: usize) -> Result<(), String> {
            Ok(())
        }

        pub async fn cancel_torrent(&self, _id: usize, _delete_files: bool) -> Result<(), String> {
            Ok(())
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
            assert!(engine.add_torrent_download("test", Path::new("."), None).await.is_err());
            assert!(engine.poll_all_tasks().is_empty());
        }
    }
}

#[cfg(not(feature = "torrent"))]
pub use stub::*;
