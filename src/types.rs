#![allow(dead_code)]

use std::path::PathBuf;

/// Single live subtitle cue captured from the player.
#[derive(Debug, Clone, PartialEq)]
pub struct SubCue {
    /// Timestamp in seconds when this cue appeared
    pub timestamp: f64,
    /// Subtitle display text (stripped of styling tags)
    pub text: String,
    /// Optional end time in seconds
    pub end_time: Option<f64>,
}

/// Subtitle track descriptor
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubTrack {
    pub id: i64,
    pub title: String,
    pub lang: String,
    pub codec: String,
    pub default: bool,
    pub forced: bool,
    pub external: bool,
    pub selected: bool,
}

/// Audio track descriptor
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioTrack {
    pub id: i64,
    pub title: String,
    pub lang: String,
    pub codec: String,
    pub channels: Option<i64>,
    pub default: bool,
    pub forced: bool,
    pub external: bool,
    pub selected: bool,
}

/// Chapter marker within the MKV media
#[derive(Debug, Clone, PartialEq)]
pub struct ChapterInfo {
    pub index: usize,
    pub title: String,
    pub time: f64,
}

/// Episode entry discovered in sibling folder or local directory
#[derive(Debug, Clone, PartialEq)]
pub struct EpisodeEntry {
    pub path: PathBuf,
    pub raw_filename: String,
    pub clean_title: String,
    pub episode_number: Option<f32>,
}

/// Item returned from Nyaa RSS query
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NyaaItem {
    pub title: String,
    pub clean_title: String,
    pub link: String,
    pub info_hash: Option<String>,
    pub seeders: u32,
    pub leechers: u32,
    pub size: String,
    pub date: String,
}

/// Filter for Nyaa searches
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NyaaFilter {
    NoFilter = 0,
    NoRemakes = 1,
    TrustedOnly = 2,
}

/// Category for Nyaa searches
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NyaaCategory {
    AnimeEng,
    AllAnime,
    AnimeRaw,
}

impl NyaaCategory {
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::AnimeEng => "1_2",
            Self::AllAnime => "1_0",
            Self::AnimeRaw => "1_4",
        }
    }
}

/// Detailed file info inside a torrent
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorrentFileInfo {
    pub index: usize,
    pub path: String,
    pub size_bytes: u64,
    pub is_mkv: bool,
}

/// High-level torrent download / stream status
#[derive(Debug, Clone, PartialEq)]
pub enum StreamStatus {
    Idle,
    Connecting { title: String },
    DownloadingMetadata { title: String },
    SelectingBatch { id: usize, files: Vec<TorrentFileInfo> },
    Streaming {
        id: usize,
        file_idx: usize,
        title: String,
        port: u16,
        progress_pct: f32,
        speed_bytes: u64,
        is_complete: bool,
    },
    Completed { file_path: PathBuf, title: String },
    Error(String),
}

/// Command sent to controlling the MPV player instance
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerCommand {
    Load(String),
    Play,
    Pause,
    TogglePause,
    SeekRelative(f64),
    SeekAbsolute(f64),
    SetVolume(f64),
    CycleSubtitle,
    SetSubtitleTrack(i64),
    CycleAudio,
    SetAudioTrack(i64),
    Stop,
}

/// Application-wide events emitted by player, background workers, or UI
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// MPV internal event wakeup (zero polling notification)
    MpvWakeup,
    /// Player loaded a media file
    FileLoaded { title: String, duration: f64, path: Option<PathBuf> },
    /// Time position changed in seconds
    TimePosChanged(f64),
    /// Total duration changed in seconds
    DurationChanged(f64),
    /// Pause state toggled
    PauseChanged(bool),
    /// Volume changed (0.0 .. 100.0)
    VolumeChanged(f64),
    /// Chapter markers loaded from file
    ChaptersLoaded(Vec<ChapterInfo>),
    /// Subtitle tracks loaded from file
    TracksLoaded(Vec<SubTrack>),
    /// Audio tracks loaded from file
    AudioTracksLoaded(Vec<AudioTrack>),
    /// Subtitle text changed from mpv 'sub-text' property
    SubTextChanged(String),
    /// Live cue emitted
    SubCueEmitted(SubCue),
    /// Sibling folder episodes loaded
    EpisodesLoaded(Vec<EpisodeEntry>),
    /// Nyaa RSS search query results
    NyaaResultsLoaded(Vec<NyaaItem>),
    /// Nyaa RSS search failed
    NyaaSearchError(String),
    /// Stream status update
    TorrentStatusChanged(StreamStatus),
    /// Batch torrent file list ready for user selection
    TorrentBatchDiscovered { id: usize, files: Vec<TorrentFileInfo> },
    /// Subtitle track extraction succeeded
    SubtitleExtracted { track_name: String, path: PathBuf },
    /// Subtitle track extraction failed
    SubtitleExtractionError(String),
    /// File picked via native dialog
    OpenFilePath(PathBuf),
    /// Folder picked via native dialog
    OpenFolderPath(PathBuf),
    /// General status bar or notification message
    StatusMessage(String),
}
