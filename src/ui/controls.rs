#![allow(dead_code)]

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;
use windows::Win32::Graphics::Gdi::*;

use crate::stream::{is_direct_stream_url, is_magnet, is_torrent_url};
use crate::types::{
    AudioTrack, BatchMode, BufferingStatus, ChapterInfo, EpisodeEntry, NyaaCategory, NyaaItem, SubCue, SubTrack,
    TorrentDownloadItem, TorrentFileInfo, TorrentTaskState,
};
use crate::ui::layout::{LayoutGeometry, Rect};
use crate::ui::theme::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Subtitles,
    Episodes,
    Nyaa,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoverControl {
    BottomBarOpen,
    EmptyStateOpenFile,
    EmptyStateOpenFolder,
    EpisodesOpenFolder,
    PlayPause,
    Next,
    Seekbar,
    VolumeIcon,
    VolumeSlider,
    AudioToggle,
    SubToggle,
    SubPopoverItem(usize),
    AudioPopoverItem(usize),
    SubTrackSelector,
    SubTrackSelectorItem(usize),
    Fullscreen,
    DrawerToggle,
    Tab(Tab),
    CueCard(usize),
    CueCopy(usize),
    ExtractAss,
    Episode(usize),
    NyaaSearchInput,
    NyaaSearchBtn,
    NyaaTrustedCheckbox,
    NyaaCategoryDropdown,
    NyaaCategoryItem(usize),
    StreamFolderPicker,
    NyaaResult(usize),
    NyaaResultStream(usize),
    NyaaResultDownload(usize),
    NyaaResultOpenMagnet(usize),
    NyaaResultCopyMagnet(usize),
    DownloadCard(usize),
    DownloadPlay(usize),
    DownloadPauseResume(usize),
    DownloadFolder(usize),
    DownloadRemove(usize),
    BatchFile(usize),
    BatchItemWatch(usize),
    BatchSelectAll,
    BatchClearAll,
    BatchConfirm,
    BatchCancel,
    StreamTrackersBtn,
    TrackersModalClose,
    TrackersModalAutoSyncToggle,
    TrackersModalUrlInput,
    TrackersModalSyncNow,
    TrackersModalCustomInput,
    TrackersModalSave,
    TrackersModalReset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackersFocusedField {
    None,
    Url,
    Custom,
}

#[derive(Debug, Clone)]
pub struct TrackersModalState {
    pub auto_sync: bool,
    pub tracker_url: String,
    pub custom_text: String,
    pub sync_status: Option<String>,
    pub focused_field: TrackersFocusedField,
    pub scroll_y: i32,
}

#[derive(Debug, Clone)]
pub struct BatchModalState {
    pub torrent_id: usize,
    pub title: String,
    pub mode: BatchMode,
    pub files: Vec<TorrentFileInfo>,
    pub selected_indices: HashSet<usize>,
    pub scroll_y: i32,
}

pub struct UiState {
    // Playback state
    pub media_title: String,
    pub current_path: Option<PathBuf>,
    pub time_pos: f64,
    pub duration: f64,
    pub paused: bool,
    pub volume: f64,
    pub muted: bool,
    pub unmuted_volume: f64,
    pub chapters: Vec<ChapterInfo>,
    pub tracks: Vec<SubTrack>,
    pub audio_tracks: Vec<AudioTrack>,
    pub selected_sub_idx: usize,
    pub sub_popover_open: bool,
    pub audio_popover_open: bool,
    pub selected_extract_sub_id: Option<i64>,
    pub sidebar_sub_dropdown_open: bool,
    pub is_fullscreen: bool,
    pub drawer_open: bool,
    pub active_tab: Tab,

    // Subtitles tab
    pub cues: Vec<SubCue>,
    pub active_cue_idx: Option<usize>,
    pub sub_scroll_y: i32,
    pub copied_feedback: Option<(usize, Instant)>,
    pub is_streaming_torrent: bool,
    pub is_torrent_complete: bool,
    pub torrent_status_text: String,

    // Episodes tab
    pub episodes: Vec<EpisodeEntry>,
    pub active_ep_idx: Option<usize>,
    pub ep_scroll_y: i32,
    pub last_ep_click: Option<(Instant, usize)>,

    // Nyaa tab
    pub search_query: String,
    pub search_focused: bool,
    pub trusted_only: bool,
    pub category: NyaaCategory,
    pub category_dropdown_open: bool,
    pub nyaa_results: Vec<NyaaItem>,
    pub searching: bool,
    pub nyaa_error: Option<String>,
    pub nyaa_scroll_y: i32,

    // Download management
    pub download_dir: PathBuf,
    pub downloads: Vec<TorrentDownloadItem>,

    // Batch torrent selection modal
    pub batch_modal: Option<BatchModalState>,

    // Trackers configuration modal & pool
    pub trackers_modal: Option<TrackersModalState>,
    pub auto_sync_trackers: bool,
    pub tracker_list_url: String,
    pub custom_trackers: Vec<String>,
    pub cached_remote_trackers: Vec<String>,

    // Interaction
    pub hovered_control: Option<HoverControl>,
    pub is_dragging_seekbar: bool,
    pub is_dragging_volume: bool,
    pub mouse_pos: (i32, i32),
    pub tooltip: Option<(String, i32, i32)>,
    pub status_message: Option<(String, Instant)>,
    pub last_mouse_activity: Instant,
    pub bar_visible: bool,

    // Buffering & overlay states
    pub stream_buffering: Option<BufferingStatus>,
    pub is_rebuffering: bool,
    pub rebuffering_percent: i64,
    pub rebuffering_speed_bytes: u64,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            media_title: "Vesper".to_string(),
            current_path: None,
            time_pos: 0.0,
            duration: 0.0,
            paused: false,
            volume: 100.0,
            muted: false,
            unmuted_volume: 100.0,
            chapters: Vec::new(),
            tracks: Vec::new(),
            audio_tracks: Vec::new(),
            selected_sub_idx: 0,
            sub_popover_open: false,
            audio_popover_open: false,
            selected_extract_sub_id: None,
            sidebar_sub_dropdown_open: false,
            is_fullscreen: false,
            drawer_open: true,
            active_tab: Tab::Subtitles,

            cues: Vec::new(),
            active_cue_idx: None,
            sub_scroll_y: 0,
            copied_feedback: None,
            is_streaming_torrent: false,
            is_torrent_complete: false,
            torrent_status_text: String::new(),

            episodes: Vec::new(),
            active_ep_idx: None,
            ep_scroll_y: 0,
            last_ep_click: None,

            search_query: String::new(),
            search_focused: false,
            trusted_only: true,
            category: NyaaCategory::AnimeEng,
            category_dropdown_open: false,
            nyaa_results: Vec::new(),
            searching: false,
            nyaa_error: None,
            nyaa_scroll_y: 0,

            download_dir: PathBuf::from(r"Downloads\Vesper"),
            downloads: Vec::new(),

            batch_modal: None,

            trackers_modal: None,
            auto_sync_trackers: true,
            tracker_list_url: crate::config::default_tracker_list_url(),
            custom_trackers: Vec::new(),
            cached_remote_trackers: Vec::new(),

            hovered_control: None,
            is_dragging_seekbar: false,
            is_dragging_volume: false,
            mouse_pos: (0, 0),
            tooltip: None,
            status_message: None,
            last_mouse_activity: Instant::now(),
            bar_visible: true,

            stream_buffering: None,
            is_rebuffering: false,
            rebuffering_percent: 0,
            rebuffering_speed_bytes: 0,
        }
    }
}

pub fn audio_button_label(state: &UiState) -> String {
    if let Some(tr) = state.audio_tracks.iter().find(|t| t.selected) {
        let lang = if !tr.lang.is_empty() {
            tr.lang.to_uppercase()
        } else {
            "AUD".to_string()
        };
        let codec = if !tr.codec.is_empty() {
            tr.codec.to_uppercase()
        } else {
            "".to_string()
        };
        if !codec.is_empty() {
            format!("{lang} · {codec}")
        } else {
            lang
        }
    } else if state.audio_tracks.is_empty() {
        "AUDIO".to_string()
    } else {
        "AUDIO".to_string()
    }
}

pub fn sub_button_label(state: &UiState) -> String {
    if let Some(tr) = state.tracks.iter().find(|t| t.selected) {
        let lang = if !tr.lang.is_empty() {
            tr.lang.to_uppercase()
        } else {
            "SUB".to_string()
        };
        let codec = if !tr.codec.is_empty() {
            tr.codec.to_uppercase()
        } else {
            "".to_string()
        };
        if !codec.is_empty() {
            format!("{lang} · {codec}")
        } else {
            lang
        }
    } else if state.tracks.is_empty() {
        "SUB".to_string()
    } else {
        "OFF".to_string()
    }
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_speed(bytes_per_sec: u64) -> String {
    if bytes_per_sec >= 1024 * 1024 {
        format!("{:.1} MB", bytes_per_sec as f64 / (1024.0 * 1024.0))
    } else if bytes_per_sec >= 1024 {
        format!("{:.0} KB", bytes_per_sec as f64 / 1024.0)
    } else {
        format!("{} B", bytes_per_sec)
    }
}

pub fn format_time(seconds: f64) -> String {
    let total_secs = seconds.max(0.0) as u64;
    let s = total_secs % 60;
    let m = (total_secs / 60) % 60;
    let h = total_secs / 3600;

    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

pub fn render_stream_buffering_card(
    hdc: HDC,
    geom: &LayoutGeometry,
    _state: &UiState,
    buf: &BufferingStatus,
    fonts: &ThemeFonts,
) {
    let card_w = 420.min(geom.video_rect.w - 40).max(280);
    let card_h = 136;
    let cx = geom.video_rect.x + (geom.video_rect.w - card_w) / 2;
    let cy = geom.video_rect.y + (geom.video_rect.h - card_h) / 2;

    // Card background & 1px border
    draw_rounded_rect(
        hdc,
        cx,
        cy,
        card_w,
        card_h,
        10,
        COLOR_SURFACE,
        Some(COLOR_BORDER),
    );

    // Headline stage
    draw_text(
        hdc,
        &buf.stage,
        cx + 20,
        cy + 18,
        card_w - 40,
        22,
        fonts.heading,
        COLOR_TEXT_PRIMARY,
        DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
    );

    // Subtitle title
    draw_text(
        hdc,
        &buf.title,
        cx + 20,
        cy + 44,
        card_w - 40,
        18,
        fonts.small,
        COLOR_TEXT_SECONDARY,
        DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
    );

    // Progress / shimmer bar (3px height)
    let bar_x = cx + 20;
    let bar_y = cy + 74;
    let bar_w = card_w - 40;
    let bar_h = 3;

    fill_rect(hdc, bar_x, bar_y, bar_w, bar_h, COLOR_SEEK_TRACK);

    if buf.total_bytes > 0 {
        let pct = (buf.buffered_bytes as f64 / buf.total_bytes as f64).clamp(0.02, 1.0);
        let filled_w = ((bar_w as f64) * pct).round() as i32;
        fill_rect(hdc, bar_x, bar_y, filled_w.max(4), bar_h, COLOR_ACCENT);
    } else {
        // Animated shimmer segment
        let shimmer_w = (bar_w / 4).max(30);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let travel = (bar_w + shimmer_w) as u128;
        let offset = ((now_ms / 15) % travel) as i32 - shimmer_w;
        let sx = (bar_x + offset).max(bar_x);
        let ex = (bar_x + offset + shimmer_w).min(bar_x + bar_w);
        if ex > sx {
            fill_rect(hdc, sx, bar_y, ex - sx, bar_h, COLOR_ACCENT);
        }
    }

    // Live stats line
    let stats_text = if buf.total_bytes > 0 {
        format!(
            "{} / {} · {} · {} peers",
            format_bytes(buf.buffered_bytes),
            format_bytes(buf.total_bytes),
            format_speed(buf.speed_bytes),
            buf.peers
        )
    } else if buf.speed_bytes > 0 || buf.peers > 0 {
        format!("{} · {} peers", format_speed(buf.speed_bytes), buf.peers)
    } else {
        "Connecting to peers...".to_string()
    };

    draw_text(
        hdc,
        &stats_text,
        cx + 20,
        cy + 88,
        card_w - 40,
        20,
        fonts.small,
        COLOR_TEXT_MUTED,
        DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
}

pub fn render_rebuffering_pill(
    hdc: HDC,
    w: i32,
    h: i32,
    state: &UiState,
    fonts: &ThemeFonts,
) {
    // Background & border
    draw_rounded_rect(
        hdc,
        0,
        0,
        w,
        h,
        8,
        COLOR_SURFACE,
        Some(COLOR_BORDER),
    );

    // Accent line at bottom (2px height)
    fill_rect(hdc, 12, h - 3, w - 24, 2, COLOR_SEEK_TRACK);
    let pct = if state.rebuffering_percent > 0 {
        (state.rebuffering_percent as f64 / 100.0).clamp(0.05, 1.0)
    } else {
        0.5
    };
    let fill_w = (((w - 24) as f64) * pct).round() as i32;
    fill_rect(hdc, 12, h - 3, fill_w.max(6), 2, COLOR_ACCENT);

    // Text: e.g. "Buffering 64% · 2.4 MB/s" or "Buffering..."
    let text = if state.rebuffering_percent > 0 && state.rebuffering_speed_bytes > 0 {
        format!(
            "Buffering {}% · {}",
            state.rebuffering_percent,
            format_speed(state.rebuffering_speed_bytes)
        )
    } else if state.rebuffering_percent > 0 {
        format!("Buffering {}%", state.rebuffering_percent)
    } else if state.rebuffering_speed_bytes > 0 {
        format!("Buffering · {}", format_speed(state.rebuffering_speed_bytes))
    } else {
        "Buffering...".to_string()
    };

    draw_text(
        hdc,
        &text,
        0,
        0,
        w,
        h - 3,
        fonts.small_bold,
        COLOR_TEXT_PRIMARY,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}

pub fn render_ui(hdc: HDC, geom: &LayoutGeometry, state: &UiState, fonts: &ThemeFonts) {
    // 1. Draw theater video backdrop (deep viewport black #0E0E11)
    fill_rect(
        hdc,
        geom.video_rect.x,
        geom.video_rect.y,
        geom.video_rect.w,
        geom.video_rect.h,
        COLOR_VIEWPORT_BG,
    );

    // If stream is buffering, render stream buffering card
    if let Some(ref buf) = state.stream_buffering {
        render_stream_buffering_card(hdc, geom, state, buf, fonts);
    } else if state.current_path.is_none() && state.duration <= 0.0 {
        let dz = &geom.empty_dropzone_rect;
        if dz.w > 160 && dz.h > 120 {
            // Dropzone card
            draw_rounded_rect(
                hdc,
                dz.x,
                dz.y,
                dz.w,
                dz.h,
                10,
                COLOR_SURFACE,
                Some(COLOR_BORDER),
            );

            // App title
            draw_text(
                hdc,
                "Vesper",
                dz.x,
                dz.y + 24,
                dz.w,
                26,
                fonts.heading,
                COLOR_TEXT_PRIMARY,
                DT_CENTER | DT_SINGLELINE,
            );

            // Subtle drop hint
            draw_text(
                hdc,
                "Drop an .mkv anime file here",
                dz.x,
                dz.y + 58,
                dz.w,
                20,
                fonts.regular,
                COLOR_TEXT_SECONDARY,
                DT_CENTER | DT_SINGLELINE,
            );

            // Secondary subtitle
            draw_text(
                hdc,
                "or open media to begin playback",
                dz.x,
                dz.y + 80,
                dz.w,
                18,
                fonts.small,
                COLOR_TEXT_MUTED,
                DT_CENTER | DT_SINGLELINE,
            );

            // Open File button (Ctrl+O)
            let of_hover = state.hovered_control == Some(HoverControl::EmptyStateOpenFile);
            draw_rounded_rect(
                hdc,
                geom.empty_open_file.x,
                geom.empty_open_file.y,
                geom.empty_open_file.w,
                geom.empty_open_file.h,
                6,
                if of_hover {
                    COLOR_ACCENT_HOVER
                } else {
                    COLOR_ACCENT
                },
                Some(COLOR_ACCENT_MUTED),
            );
            draw_text(
                hdc,
                "🎬 Open File",
                geom.empty_open_file.x,
                geom.empty_open_file.y,
                geom.empty_open_file.w,
                geom.empty_open_file.h,
                fonts.bold,
                COLOR_TEXT_PRIMARY,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );

            // Open Folder button (Ctrl+Shift+O)
            let ofd_hover = state.hovered_control == Some(HoverControl::EmptyStateOpenFolder);
            draw_rounded_rect(
                hdc,
                geom.empty_open_folder.x,
                geom.empty_open_folder.y,
                geom.empty_open_folder.w,
                geom.empty_open_folder.h,
                6,
                if ofd_hover {
                    COLOR_SURFACE_HOVER
                } else {
                    COLOR_BG
                },
                Some(if ofd_hover {
                    COLOR_ACCENT
                } else {
                    COLOR_BORDER
                }),
            );
            draw_text(
                hdc,
                "📁 Open Folder",
                geom.empty_open_folder.x,
                geom.empty_open_folder.y,
                geom.empty_open_folder.w,
                geom.empty_open_folder.h,
                fonts.bold,
                if ofd_hover {
                    COLOR_ACCENT
                } else {
                    COLOR_TEXT_PRIMARY
                },
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );

            // Keyboard shortcut guide line
            draw_text(
                hdc,
                "Ctrl+O: Open File  •  Ctrl+Shift+O: Open Folder  •  Space: Play  •  Tab: Sidebar",
                dz.x,
                dz.bottom() + 16,
                dz.w,
                18,
                fonts.small,
                COLOR_TEXT_MUTED,
                DT_CENTER | DT_SINGLELINE,
            );
        }
    }

    // 2. Render right collapsible drawer
    if geom.drawer_rect.w > 0 {
        render_drawer(hdc, geom, state, fonts);
    }

    // 4. Render subtle auto-dismissing status/error toast in viewport
    if let Some((ref msg, instant)) = state.status_message {
        if instant.elapsed().as_secs_f32() < 4.5 {
            render_toast(hdc, geom, msg, fonts);
        }
    }

    // 5. Render modal dialog if open (e.g. batch torrent selection, trackers configuration)
    if let Some(ref modal) = state.batch_modal {
        render_batch_modal(hdc, geom, modal, fonts, state);
    } else if let Some(ref modal) = state.trackers_modal {
        render_trackers_modal(hdc, geom, modal, fonts, state);
    }

    // 6. Render active tooltip
    if let Some((ref text, tx, ty)) = state.tooltip {
        render_tooltip(hdc, text, tx, ty, fonts);
    }
}

pub fn render_bottom_bar(hdc: HDC, geom: &LayoutGeometry, state: &UiState, fonts: &ThemeFonts) {
    let r = &geom.bottom_bar_rect;
    // Surface background
    fill_rect(hdc, r.x, r.y, r.w, r.h, COLOR_SURFACE);

    // Top border line
    fill_rect(hdc, r.x, r.y, r.w, 1, COLOR_BORDER);

    // Open File/Folder button (compact left button)
    let open_hover = state.hovered_control == Some(HoverControl::BottomBarOpen);
    draw_rounded_rect(
        hdc,
        geom.btn_open.x,
        geom.btn_open.y,
        geom.btn_open.w,
        geom.btn_open.h,
        6,
        if open_hover {
            COLOR_SURFACE_HOVER
        } else {
            COLOR_SURFACE
        },
        Some(if open_hover {
            COLOR_ACCENT
        } else {
            COLOR_BORDER
        }),
    );
    draw_text(
        hdc,
        "📁",
        geom.btn_open.x,
        geom.btn_open.y,
        geom.btn_open.w,
        geom.btn_open.h,
        fonts.regular,
        if open_hover {
            COLOR_ACCENT
        } else {
            COLOR_TEXT_SECONDARY
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Play/Pause button
    let pp_hover = state.hovered_control == Some(HoverControl::PlayPause);
    draw_rounded_rect(
        hdc,
        geom.btn_play_pause.x,
        geom.btn_play_pause.y,
        geom.btn_play_pause.w,
        geom.btn_play_pause.h,
        6,
        if pp_hover {
            COLOR_SURFACE_HOVER
        } else {
            COLOR_SURFACE
        },
        Some(if pp_hover {
            COLOR_ACCENT
        } else {
            COLOR_BORDER
        }),
    );
    let pp_icon = if state.paused { "▶" } else { "❚❚" };
    draw_text(
        hdc,
        pp_icon,
        geom.btn_play_pause.x,
        geom.btn_play_pause.y,
        geom.btn_play_pause.w,
        geom.btn_play_pause.h,
        fonts.bold,
        if pp_hover {
            COLOR_ACCENT
        } else {
            COLOR_TEXT_PRIMARY
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Next Episode button
    let next_hover = state.hovered_control == Some(HoverControl::Next);
    draw_rounded_rect(
        hdc,
        geom.btn_next.x,
        geom.btn_next.y,
        geom.btn_next.w,
        geom.btn_next.h,
        6,
        if next_hover {
            COLOR_SURFACE_HOVER
        } else {
            COLOR_SURFACE
        },
        Some(if next_hover {
            COLOR_BORDER_HOVER
        } else {
            COLOR_BORDER
        }),
    );
    draw_text(
        hdc,
        "⏭",
        geom.btn_next.x,
        geom.btn_next.y,
        geom.btn_next.w,
        geom.btn_next.h,
        fonts.regular,
        if next_hover {
            COLOR_TEXT_PRIMARY
        } else {
            COLOR_TEXT_SECONDARY
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Time text: 05:23 / 24:12 or error indicator
    let is_recent_err = state
        .status_message
        .as_ref()
        .map(|(msg, inst)| inst.elapsed().as_secs_f32() < 4.5 && msg.starts_with("Playback error"))
        .unwrap_or(false);

    if is_recent_err {
        draw_text(
            hdc,
            "⚠ Playback Error",
            geom.time_text_rect.x,
            geom.time_text_rect.y,
            geom.time_text_rect.w,
            geom.time_text_rect.h,
            fonts.small_bold,
            COLOR_ERROR,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
    } else {
        let mut time_str = format!(
            "{} / {}",
            format_time(state.time_pos),
            format_time(state.duration)
        );
        if state.is_rebuffering {
            time_str.push_str(" · Buffering...");
        }
        draw_text(
            hdc,
            &time_str,
            geom.time_text_rect.x,
            geom.time_text_rect.y,
            geom.time_text_rect.w,
            geom.time_text_rect.h,
            fonts.mono,
            COLOR_TEXT_SECONDARY,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
    }

    // Seekbar with chapters
    render_seekbar(hdc, geom, state);

    // Volume icon
    let vol_hover = state.hovered_control == Some(HoverControl::VolumeIcon);
    let vol_icon = if state.muted || state.volume <= 0.0 {
        "🔇"
    } else if state.volume < 40.0 {
        "🔈"
    } else {
        "🔊"
    };
    draw_text(
        hdc,
        vol_icon,
        geom.btn_vol_icon.x,
        geom.btn_vol_icon.y,
        geom.btn_vol_icon.w,
        geom.btn_vol_icon.h,
        fonts.regular,
        if vol_hover {
            COLOR_ACCENT
        } else {
            COLOR_TEXT_SECONDARY
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Volume slider
    render_vol_slider(hdc, geom, state);

    // Audio track toggle button
    let aud_hover = state.hovered_control == Some(HoverControl::AudioToggle);
    let aud_label = audio_button_label(state);
    draw_rounded_rect(
        hdc,
        geom.btn_audio_toggle.x,
        geom.btn_audio_toggle.y,
        geom.btn_audio_toggle.w,
        geom.btn_audio_toggle.h,
        6,
        if aud_hover || state.audio_popover_open {
            COLOR_SURFACE_HOVER
        } else {
            COLOR_SURFACE
        },
        Some(if aud_hover || state.audio_popover_open {
            COLOR_ACCENT
        } else {
            COLOR_BORDER
        }),
    );
    draw_text(
        hdc,
        &aud_label,
        geom.btn_audio_toggle.x,
        geom.btn_audio_toggle.y,
        geom.btn_audio_toggle.w,
        geom.btn_audio_toggle.h,
        fonts.small_bold,
        if aud_hover || state.audio_popover_open {
            COLOR_ACCENT
        } else {
            COLOR_TEXT_PRIMARY
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );

    // Subtitle track toggle button
    let sub_hover = state.hovered_control == Some(HoverControl::SubToggle);
    let sub_label = sub_button_label(state);
    draw_rounded_rect(
        hdc,
        geom.btn_sub_toggle.x,
        geom.btn_sub_toggle.y,
        geom.btn_sub_toggle.w,
        geom.btn_sub_toggle.h,
        6,
        if sub_hover || state.sub_popover_open {
            COLOR_SURFACE_HOVER
        } else {
            COLOR_SURFACE
        },
        Some(if sub_hover || state.sub_popover_open {
            COLOR_ACCENT
        } else {
            COLOR_BORDER
        }),
    );
    draw_text(
        hdc,
        &sub_label,
        geom.btn_sub_toggle.x,
        geom.btn_sub_toggle.y,
        geom.btn_sub_toggle.w,
        geom.btn_sub_toggle.h,
        fonts.small_bold,
        if sub_hover || state.sub_popover_open {
            COLOR_ACCENT
        } else {
            COLOR_TEXT_PRIMARY
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );

    // Fullscreen toggle button
    let fs_hover = state.hovered_control == Some(HoverControl::Fullscreen);
    draw_rounded_rect(
        hdc,
        geom.btn_fullscreen.x,
        geom.btn_fullscreen.y,
        geom.btn_fullscreen.w,
        geom.btn_fullscreen.h,
        6,
        if fs_hover {
            COLOR_SURFACE_HOVER
        } else {
            COLOR_SURFACE
        },
        Some(if fs_hover {
            COLOR_BORDER_HOVER
        } else {
            COLOR_BORDER
        }),
    );
    draw_text(
        hdc,
        if state.is_fullscreen { "⛶" } else { "⛶" },
        geom.btn_fullscreen.x,
        geom.btn_fullscreen.y,
        geom.btn_fullscreen.w,
        geom.btn_fullscreen.h,
        fonts.regular,
        if fs_hover {
            COLOR_ACCENT
        } else {
            COLOR_TEXT_PRIMARY
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Drawer collapse toggle button (< / >)
    let dr_hover = state.hovered_control == Some(HoverControl::DrawerToggle);
    draw_rounded_rect(
        hdc,
        geom.btn_drawer_toggle.x,
        geom.btn_drawer_toggle.y,
        geom.btn_drawer_toggle.w,
        geom.btn_drawer_toggle.h,
        6,
        if dr_hover {
            COLOR_SURFACE_HOVER
        } else {
            COLOR_SURFACE
        },
        Some(if dr_hover {
            COLOR_ACCENT
        } else {
            COLOR_BORDER
        }),
    );
    let toggle_symbol = if state.drawer_open { ">" } else { "<" };
    draw_text(
        hdc,
        toggle_symbol,
        geom.btn_drawer_toggle.x,
        geom.btn_drawer_toggle.y,
        geom.btn_drawer_toggle.w,
        geom.btn_drawer_toggle.h,
        fonts.bold,
        if dr_hover {
            COLOR_ACCENT
        } else {
            COLOR_TEXT_SECONDARY
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}

fn render_seekbar(hdc: HDC, geom: &LayoutGeometry, state: &UiState) {
    let sb = &geom.seekbar_rect;
    if sb.w <= 10 {
        return;
    }

    let track_y = sb.y + (sb.h - 6) / 2;
    let track_h = 6;

    // Background track
    draw_rounded_rect(
        hdc,
        sb.x,
        track_y,
        sb.w,
        track_h,
        3,
        COLOR_SEEK_TRACK,
        None,
    );

    // Progress fraction
    let pct = if state.duration > 0.0 {
        (state.time_pos / state.duration).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let fill_w = ((sb.w as f64) * pct) as i32;

    if fill_w > 0 {
        draw_rounded_rect(
            hdc,
            sb.x,
            track_y,
            fill_w,
            track_h,
            3,
            COLOR_ACCENT,
            None,
        );
    }

    // Chapter tick marks (OP/ED/parts) - contained within seekbar track height, disabled if > 50
    if state.duration > 0.0 && state.chapters.len() <= 50 {
        for ch in &state.chapters {
            let ch_pct = (ch.time / state.duration).clamp(0.0, 1.0);
            let ch_x = sb.x + ((sb.w as f64) * ch_pct) as i32;
            let is_op_ed = ch.title.to_lowercase().contains("op")
                || ch.title.to_lowercase().contains("ed")
                || ch.title.to_lowercase().contains("opening")
                || ch.title.to_lowercase().contains("ending");

            let tick_color = if is_op_ed {
                COLOR_WARNING
            } else {
                COLOR_ACCENT_HOVER
            };
            fill_rect(hdc, ch_x - 1, track_y, 2, track_h, tick_color);
        }
    }

    // Seek thumb handle
    let thumb_x = (sb.x + fill_w).clamp(sb.x, sb.x + sb.w);
    let thumb_radius = if state.is_dragging_seekbar || state.hovered_control == Some(HoverControl::Seekbar) {
        6
    } else {
        4
    };
    draw_rounded_rect(
        hdc,
        thumb_x - thumb_radius,
        track_y + track_h / 2 - thumb_radius,
        thumb_radius * 2,
        thumb_radius * 2,
        thumb_radius * 2,
        COLOR_TEXT_PRIMARY,
        Some(COLOR_ACCENT),
    );
}

fn render_vol_slider(hdc: HDC, geom: &LayoutGeometry, state: &UiState) {
    let vs = &geom.vol_slider_rect;
    if vs.w <= 5 {
        return;
    }
    let track_y = vs.y + (vs.h - 4) / 2;
    let track_h = 4;

    // Track
    draw_rounded_rect(hdc, vs.x, track_y, vs.w, track_h, 2, COLOR_SEEK_TRACK, None);

    // Fill
    let vol_pct = if state.muted {
        0.0
    } else {
        (state.volume / 100.0).clamp(0.0, 1.0)
    };
    let fill_w = ((vs.w as f64) * vol_pct) as i32;
    if fill_w > 0 {
        draw_rounded_rect(hdc, vs.x, track_y, fill_w, track_h, 2, COLOR_ACCENT, None);
    }

    // Thumb
    let thumb_x = (vs.x + fill_w).clamp(vs.x, vs.x + vs.w);
    let r = 4;
    draw_rounded_rect(
        hdc,
        thumb_x - r,
        track_y + track_h / 2 - r,
        r * 2,
        r * 2,
        r * 2,
        COLOR_TEXT_PRIMARY,
        Some(COLOR_ACCENT),
    );
}

fn render_drawer(hdc: HDC, geom: &LayoutGeometry, state: &UiState, fonts: &ThemeFonts) {
    let dr = &geom.drawer_rect;

    // Drawer background
    fill_rect(hdc, dr.x, dr.y, dr.w, dr.h, COLOR_SURFACE);

    // Left border separating viewport and drawer
    fill_rect(hdc, dr.x, dr.y, 1, dr.h, COLOR_BORDER);

    // Tab Header (3 tabs: Subtitles, Episodes, Nyaa)
    render_tabs(hdc, geom, state, fonts);

    // Tab Content Area
    match state.active_tab {
        Tab::Subtitles => render_tab_subtitles(hdc, geom, state, fonts),
        Tab::Episodes => render_tab_episodes(hdc, geom, state, fonts),
        Tab::Nyaa => render_tab_nyaa(hdc, geom, state, fonts),
    }
}

fn render_tabs(hdc: HDC, geom: &LayoutGeometry, state: &UiState, fonts: &ThemeFonts) {
    // Bottom border of tab bar
    fill_rect(
        hdc,
        geom.drawer_tab_bar.x,
        geom.drawer_tab_bar.bottom() - 1,
        geom.drawer_tab_bar.w,
        1,
        COLOR_BORDER,
    );

    let tabs = [
        (Tab::Subtitles, "Subtitles", geom.tab_subtitles),
        (Tab::Episodes, "Playlist", geom.tab_episodes),
        (Tab::Nyaa, "Network", geom.tab_nyaa),
    ];

    for (tab, title, r) in tabs {
        let is_active = state.active_tab == tab;
        let is_hover = state.hovered_control == Some(HoverControl::Tab(tab));

        let bg_color = if is_active {
            COLOR_ACCENT
        } else if is_hover {
            COLOR_SURFACE_HOVER
        } else {
            COLOR_SURFACE
        };

        let text_color = if is_active {
            COLOR_TEXT_PRIMARY
        } else if is_hover {
            COLOR_TEXT_PRIMARY
        } else {
            COLOR_TEXT_SECONDARY
        };

        draw_rounded_rect(hdc, r.x, r.y, r.w, r.h, 6, bg_color, None);
        draw_text(
            hdc,
            title,
            r.x,
            r.y,
            r.w,
            r.h,
            if is_active {
                fonts.bold
            } else {
                fonts.regular
            },
            text_color,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
}

fn render_tab_subtitles(
    hdc: HDC,
    geom: &LayoutGeometry,
    state: &UiState,
    fonts: &ThemeFonts,
) {
    // 1. Top Subtitle Track Selector dropdown
    let sel_track = state
        .tracks
        .iter()
        .find(|t| Some(t.id) == state.selected_extract_sub_id)
        .or_else(|| state.tracks.iter().find(|t| t.selected))
        .or_else(|| state.tracks.first());

    let sel_r = &geom.sub_track_selector_rect;
    let sel_hover = state.hovered_control == Some(HoverControl::SubTrackSelector);

    draw_rounded_rect(
        hdc,
        sel_r.x,
        sel_r.y,
        sel_r.w,
        sel_r.h,
        6,
        if sel_hover || state.sidebar_sub_dropdown_open {
            COLOR_SURFACE_HOVER
        } else {
            COLOR_BG
        },
        Some(if sel_hover || state.sidebar_sub_dropdown_open {
            COLOR_ACCENT
        } else {
            COLOR_BORDER
        }),
    );

    let sel_text = if let Some(tr) = sel_track {
        let lang = if !tr.lang.is_empty() {
            tr.lang.to_uppercase()
        } else {
            "UND".to_string()
        };
        let title = if !tr.title.is_empty() {
            tr.title.clone()
        } else {
            format!("Track {}", tr.id)
        };
        let codec = if !tr.codec.is_empty() {
            tr.codec.to_uppercase()
        } else {
            "SUB".to_string()
        };
        format!("[{lang}] {title} ({codec})")
    } else {
        "No Subtitle Tracks Found".to_string()
    };

    draw_text(
        hdc,
        &sel_text,
        sel_r.x + 8,
        sel_r.y,
        sel_r.w - 32,
        sel_r.h,
        fonts.small_bold,
        COLOR_TEXT_PRIMARY,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
    draw_text(
        hdc,
        if state.sidebar_sub_dropdown_open { "▲" } else { "▼" },
        sel_r.right() - 24,
        sel_r.y,
        20,
        sel_r.h,
        fonts.small,
        COLOR_TEXT_SECONDARY,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    let is_bitmap = sel_track
        .map(|t| {
            let c = t.codec.to_lowercase();
            c.contains("pgs") || c.contains("vobsub") || c.contains("hdmv")
        })
        .unwrap_or(false);

    let list_r = &geom.subtitle_list_rect;

    if is_bitmap {
        draw_text(
            hdc,
            "Bitmap / PGS Subtitle Stream Selected\n\nLive text capture is unavailable for bitmap subtitles.\nUse 'Extract .SUP' below to export raw Blu-ray subtitles.",
            list_r.x + 20,
            list_r.y + 60,
            list_r.w - 40,
            90,
            fonts.regular,
            COLOR_TEXT_SECONDARY,
            DT_CENTER | DT_WORDBREAK,
        );
    } else if state.cues.is_empty() {
        draw_text(
            hdc,
            "No live subtitle cues captured yet.\nSubtitles appear here as video plays.",
            list_r.x + 20,
            list_r.y + 60,
            list_r.w - 40,
            60,
            fonts.regular,
            COLOR_TEXT_MUTED,
            DT_CENTER | DT_WORDBREAK,
        );
    } else {
        // Draw list of cue cards
        let card_h = 70;
        let card_margin = 8;
        let mut y = list_r.y + 8 - state.sub_scroll_y;

        for (idx, cue) in state.cues.iter().enumerate() {
            if y + card_h >= list_r.y && y <= list_r.bottom() {
                let card_r = Rect::new(list_r.x + 12, y, list_r.w - 24, card_h);
                let is_active = state.active_cue_idx == Some(idx);
                let is_hover = state.hovered_control == Some(HoverControl::CueCard(idx));

                let card_bg = if is_active {
                    COLOR_CARD_ACTIVE
                } else if is_hover {
                    COLOR_SURFACE_HOVER
                } else {
                    COLOR_BG
                };

                let card_border = if is_active {
                    Some(COLOR_ACCENT)
                } else if is_hover {
                    Some(COLOR_BORDER_HOVER)
                } else {
                    Some(COLOR_BORDER)
                };

                draw_rounded_rect(
                    hdc,
                    card_r.x,
                    card_r.y,
                    card_r.w,
                    card_r.h,
                    6,
                    card_bg,
                    card_border,
                );

                // Active cue highlight bar (vertical accent line)
                if is_active {
                    fill_rect(hdc, card_r.x + 2, card_r.y + 8, 3, card_r.h - 16, COLOR_ACCENT);
                }

                // Timestamp badge
                let time_str = format_time(cue.timestamp);
                let badge_x = card_r.x + 10;
                let badge_y = card_r.y + 8;
                let _bw = draw_badge(
                    hdc,
                    &time_str,
                    badge_x,
                    badge_y,
                    fonts.mono,
                    COLOR_SURFACE,
                    Some(COLOR_BORDER),
                    COLOR_ACCENT,
                );

                // Copy button on cue card
                let copy_r = Rect::new(card_r.right() - 56, card_r.y + 8, 48, 20);
                let is_copied = if let Some((c_idx, instant)) = state.copied_feedback {
                    c_idx == idx && instant.elapsed().as_secs_f32() < 1.5
                } else {
                    false
                };
                let copy_hover = state.hovered_control == Some(HoverControl::CueCopy(idx));

                let copy_label = if is_copied { "✓ Copied" } else { "Copy" };
                draw_rounded_rect(
                    hdc,
                    copy_r.x,
                    copy_r.y,
                    copy_r.w,
                    copy_r.h,
                    4,
                    if is_copied {
                        COLOR_SUCCESS
                    } else if copy_hover {
                        COLOR_ACCENT
                    } else {
                        COLOR_SURFACE
                    },
                    Some(COLOR_BORDER),
                );
                draw_text(
                    hdc,
                    copy_label,
                    copy_r.x,
                    copy_r.y,
                    copy_r.w,
                    copy_r.h,
                    fonts.small,
                    if is_copied || copy_hover {
                        COLOR_TEXT_PRIMARY
                    } else {
                        COLOR_TEXT_SECONDARY
                    },
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );

                // Subtitle dialogue text
                draw_text(
                    hdc,
                    &cue.text,
                    card_r.x + 12,
                    card_r.y + 32,
                    card_r.w - 24,
                    32,
                    fonts.regular,
                    if is_active {
                        COLOR_TEXT_PRIMARY
                    } else {
                        COLOR_TEXT_SECONDARY
                    },
                    DT_LEFT | DT_WORDBREAK,
                );
            }
            y += card_h + card_margin;
        }
    }

    // Pinned footer button: Extract Subtitles (adaptive format)
    let btn_r = &geom.btn_extract_ass;
    let is_disabled = state.is_streaming_torrent && !state.is_torrent_complete;
    let hover_extract = state.hovered_control == Some(HoverControl::ExtractAss);

    let default_label = if is_bitmap {
        "💾 Extract .SUP Subtitles"
    } else if let Some(tr) = sel_track {
        let c = tr.codec.to_lowercase();
        if c.contains("srt") || c.contains("utf8") {
            "💾 Extract .SRT Subtitles"
        } else {
            "💾 Extract .ASS (+ Fonts)"
        }
    } else {
        "💾 Extract Subtitles"
    };

    let (bg_color, border_color, text_color, label) = if is_disabled {
        (
            COLOR_SURFACE_HOVER,
            Some(COLOR_BORDER),
            COLOR_TEXT_MUTED,
            "Extract Subtitles (Torrent Incomplete)",
        )
    } else if hover_extract {
        (
            COLOR_ACCENT_HOVER,
            Some(COLOR_ACCENT),
            COLOR_TEXT_PRIMARY,
            default_label,
        )
    } else {
        (
            COLOR_ACCENT,
            Some(COLOR_ACCENT_MUTED),
            COLOR_TEXT_PRIMARY,
            default_label,
        )
    };

    draw_rounded_rect(
        hdc,
        btn_r.x,
        btn_r.y,
        btn_r.w,
        btn_r.h,
        6,
        bg_color,
        border_color,
    );
    draw_text(
        hdc,
        label,
        btn_r.x,
        btn_r.y,
        btn_r.w,
        btn_r.h,
        fonts.bold,
        text_color,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Sidebar subtitle dropdown menu when open
    if state.sidebar_sub_dropdown_open && !state.tracks.is_empty() {
        let dd_x = geom.sub_track_selector_rect.x;
        let dd_y = geom.sub_track_selector_rect.bottom() + 4;
        let dd_w = geom.sub_track_selector_rect.w;
        let item_h = 28;
        let dd_h = ((state.tracks.len() as i32) * item_h).min(180);

        draw_rounded_rect(
            hdc,
            dd_x,
            dd_y,
            dd_w,
            dd_h,
            6,
            COLOR_SURFACE,
            Some(COLOR_BORDER_HOVER),
        );

        for (idx, tr) in state.tracks.iter().enumerate().take(6) {
            let row_y = dd_y + (idx as i32) * item_h;
            let is_item_hover =
                state.hovered_control == Some(HoverControl::SubTrackSelectorItem(idx));
            if is_item_hover {
                draw_rounded_rect(
                    hdc,
                    dd_x + 2,
                    row_y + 2,
                    dd_w - 4,
                    item_h - 4,
                    4,
                    COLOR_SURFACE_HOVER,
                    None,
                );
            }
            let is_sel = Some(tr.id) == state.selected_extract_sub_id;
            let bullet = if is_sel { "● " } else { "   " };
            let lang = if !tr.lang.is_empty() {
                tr.lang.to_uppercase()
            } else {
                "UND".to_string()
            };
            let title = if !tr.title.is_empty() {
                tr.title.clone()
            } else {
                format!("Track {}", tr.id)
            };
            let codec = if !tr.codec.is_empty() {
                tr.codec.to_uppercase()
            } else {
                "SUB".to_string()
            };
            let item_text = format!("{bullet}[{lang}] {title} ({codec})");

            draw_text(
                hdc,
                &item_text,
                dd_x + 8,
                row_y,
                dd_w - 16,
                item_h,
                if is_sel {
                    fonts.bold
                } else {
                    fonts.regular
                },
                if is_sel {
                    COLOR_ACCENT
                } else if is_item_hover {
                    COLOR_TEXT_PRIMARY
                } else {
                    COLOR_TEXT_SECONDARY
                },
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
        }
    }
}

fn render_toast(hdc: HDC, geom: &LayoutGeometry, msg: &str, fonts: &ThemeFonts) {
    let (tw, th) = measure_text(hdc, msg, fonts.regular);
    let pad_x = 16;
    let pad_y = 6;
    let toast_w = (tw + pad_x * 2).max(220).min(geom.video_rect.w - 32);
    let toast_h = (th + pad_y * 2).max(32);
    let toast_x = geom.video_rect.x + (geom.video_rect.w - toast_w) / 2;
    let toast_y = (geom.video_rect.bottom() - toast_h - 16).max(10);

    let is_error = msg.to_lowercase().contains("error") || msg.to_lowercase().contains("failed");
    let border_color = if is_error { COLOR_ERROR } else { COLOR_ACCENT };
    let text_color = if is_error { COLOR_ERROR } else { COLOR_TEXT_PRIMARY };

    draw_rounded_rect(
        hdc,
        toast_x,
        toast_y,
        toast_w,
        toast_h,
        6,
        COLOR_SURFACE,
        Some(border_color),
    );

    draw_text(
        hdc,
        msg,
        toast_x + pad_x,
        toast_y + pad_y,
        toast_w - pad_x * 2,
        th,
        fonts.regular,
        text_color,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
}

fn render_tab_episodes(
    hdc: HDC,
    geom: &LayoutGeometry,
    state: &UiState,
    fonts: &ThemeFonts,
) {
    // Open Folder header button
    let ep_header_r = &geom.ep_header_open_folder;
    let ep_hdr_hover = state.hovered_control == Some(HoverControl::EpisodesOpenFolder);
    draw_rounded_rect(
        hdc,
        ep_header_r.x,
        ep_header_r.y,
        ep_header_r.w,
        ep_header_r.h,
        6,
        if ep_hdr_hover {
            COLOR_SURFACE_HOVER
        } else {
            COLOR_SURFACE
        },
        Some(if ep_hdr_hover {
            COLOR_ACCENT
        } else {
            COLOR_BORDER
        }),
    );
    draw_text(
        hdc,
        "📁 Open Folder",
        ep_header_r.x,
        ep_header_r.y,
        ep_header_r.w,
        ep_header_r.h,
        fonts.bold,
        if ep_hdr_hover {
            COLOR_ACCENT
        } else {
            COLOR_TEXT_PRIMARY
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    let list_r = &geom.episode_list_rect;

    if state.episodes.is_empty() {
        draw_text(
            hdc,
            "No anime folder loaded.\nOpen a season directory to list episodes:",
            list_r.x + 20,
            list_r.y + 36,
            list_r.w - 40,
            44,
            fonts.regular,
            COLOR_TEXT_MUTED,
            DT_CENTER | DT_WORDBREAK,
        );

        let ep_btn = &geom.ep_empty_open_folder;
        let ep_btn_hover = state.hovered_control == Some(HoverControl::EpisodesOpenFolder);
        draw_rounded_rect(
            hdc,
            ep_btn.x,
            ep_btn.y,
            ep_btn.w,
            ep_btn.h,
            6,
            if ep_btn_hover {
                COLOR_ACCENT_HOVER
            } else {
                COLOR_ACCENT
            },
            Some(COLOR_ACCENT_MUTED),
        );
        draw_text(
            hdc,
            "Choose Folder",
            ep_btn.x,
            ep_btn.y,
            ep_btn.w,
            ep_btn.h,
            fonts.bold,
            COLOR_TEXT_PRIMARY,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        return;
    }

    let item_h = 56;
    let item_margin = 6;
    let mut y = list_r.y + 8 - state.ep_scroll_y;

    for (idx, ep) in state.episodes.iter().enumerate() {
        if y + item_h >= list_r.y && y <= list_r.bottom() {
            let row_r = Rect::new(list_r.x + 12, y, list_r.w - 24, item_h);
            let is_active = state.active_ep_idx == Some(idx);
            let is_hover = state.hovered_control == Some(HoverControl::Episode(idx));

            let bg_color = if is_active {
                COLOR_CARD_ACTIVE
            } else if is_hover {
                COLOR_SURFACE_HOVER
            } else {
                COLOR_BG
            };

            let border_color = if is_active {
                Some(COLOR_ACCENT)
            } else if is_hover {
                Some(COLOR_BORDER_HOVER)
            } else {
                Some(COLOR_BORDER)
            };

            draw_rounded_rect(
                hdc,
                row_r.x,
                row_r.y,
                row_r.w,
                row_r.h,
                6,
                bg_color,
                border_color,
            );

            // Episode number badge
            let ep_badge = if let Some(num) = ep.episode_number {
                format!("EP {:02}", num as u32)
            } else {
                format!("EP {:02}", idx + 1)
            };

            let _bw = draw_badge(
                hdc,
                &ep_badge,
                row_r.x + 10,
                row_r.y + 8,
                fonts.small_bold,
                COLOR_SURFACE,
                Some(COLOR_BORDER),
                if is_active {
                    COLOR_ACCENT
                } else {
                    COLOR_TEXT_SECONDARY
                },
            );

            // Clean title
            draw_text(
                hdc,
                &ep.clean_title,
                row_r.x + 10,
                row_r.y + 28,
                row_r.w - 20,
                22,
                fonts.bold,
                if is_active {
                    COLOR_TEXT_PRIMARY
                } else {
                    COLOR_TEXT_SECONDARY
                },
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
        }
        y += item_h + item_margin;
    }
}

fn render_tab_nyaa(hdc: HDC, geom: &LayoutGeometry, state: &UiState, fonts: &ThemeFonts) {
    // Search input
    let si = &geom.nyaa_search_input;
    let is_focused = state.search_focused;
    draw_rounded_rect(
        hdc,
        si.x,
        si.y,
        si.w,
        si.h,
        6,
        COLOR_BG,
        Some(if is_focused {
            COLOR_ACCENT
        } else {
            COLOR_BORDER
        }),
    );

    let display_query = if state.search_query.is_empty() && !is_focused {
        "Search anime or paste magnet: / URL..."
    } else {
        &state.search_query
    };
    let text_color = if state.search_query.is_empty() && !is_focused {
        COLOR_TEXT_MUTED
    } else {
        COLOR_TEXT_PRIMARY
    };

    let mut query_with_cursor = state.search_query.clone();
    if is_focused {
        query_with_cursor.push('|');
    }

    draw_text(
        hdc,
        if is_focused {
            &query_with_cursor
        } else {
            display_query
        },
        si.x + 8,
        si.y,
        si.w - 16,
        si.h,
        fonts.regular,
        text_color,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );

    // Search / Stream button
    let sb = &geom.nyaa_search_btn;
    let sb_hover = state.hovered_control == Some(HoverControl::NyaaSearchBtn);
    draw_rounded_rect(
        hdc,
        sb.x,
        sb.y,
        sb.w,
        sb.h,
        6,
        if sb_hover {
            COLOR_ACCENT_HOVER
        } else {
            COLOR_ACCENT
        },
        Some(COLOR_ACCENT_MUTED),
    );
    let btn_label = if state.searching {
        "..."
    } else if is_magnet(&state.search_query)
        || is_torrent_url(&state.search_query)
        || is_direct_stream_url(&state.search_query)
    {
        "Watch"
    } else {
        "Search"
    };
    draw_text(
        hdc,
        btn_label,
        sb.x,
        sb.y,
        sb.w,
        sb.h,
        fonts.bold,
        COLOR_TEXT_PRIMARY,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Filters row: Trusted Only checkbox + Category dropdown + Download Folder button
    let cb = &geom.nyaa_trusted_checkbox;
    let cb_hover = state.hovered_control == Some(HoverControl::NyaaTrustedCheckbox);
    let box_size = 14;
    let box_y = cb.y + (cb.h - box_size) / 2;
    draw_rounded_rect(
        hdc,
        cb.x,
        box_y,
        box_size,
        box_size,
        3,
        if state.trusted_only {
            COLOR_ACCENT
        } else {
            COLOR_BG
        },
        Some(if cb_hover {
            COLOR_ACCENT
        } else {
            COLOR_BORDER
        }),
    );
    if state.trusted_only {
        draw_text(
            hdc,
            "✓",
            cb.x,
            box_y - 1,
            box_size,
            box_size,
            fonts.small_bold,
            COLOR_TEXT_PRIMARY,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
    draw_text(
        hdc,
        "Trusted Only",
        cb.x + box_size + 6,
        cb.y,
        cb.w - box_size - 6,
        cb.h,
        fonts.small,
        if cb_hover {
            COLOR_TEXT_PRIMARY
        } else {
            COLOR_TEXT_SECONDARY
        },
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );

    // Category dropdown selector
    let dd = &geom.nyaa_category_dropdown;
    let dd_hover = state.hovered_control == Some(HoverControl::NyaaCategoryDropdown);
    draw_rounded_rect(
        hdc,
        dd.x,
        dd.y,
        dd.w,
        dd.h,
        4,
        COLOR_SURFACE_HOVER,
        Some(if dd_hover {
            COLOR_ACCENT
        } else {
            COLOR_BORDER
        }),
    );
    let cat_label = match state.category {
        NyaaCategory::AnimeEng => "Anime - Eng ▾",
        NyaaCategory::AllAnime => "All Anime ▾",
        NyaaCategory::AnimeRaw => "Anime - Raw ▾",
    };
    draw_text(
        hdc,
        cat_label,
        dd.x + 6,
        dd.y,
        dd.w - 12,
        dd.h,
        fonts.small_bold,
        COLOR_TEXT_PRIMARY,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Download Folder button
    let df = &geom.stream_download_dir_btn;
    let df_hover = state.hovered_control == Some(HoverControl::StreamFolderPicker);
    draw_rounded_rect(
        hdc,
        df.x,
        df.y,
        df.w,
        df.h,
        4,
        if df_hover {
            COLOR_SURFACE_HOVER
        } else {
            COLOR_SURFACE
        },
        Some(if df_hover {
            COLOR_ACCENT
        } else {
            COLOR_BORDER
        }),
    );
    let folder_name = state
        .download_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Downloads");
    let df_label = if df.w < 65 {
        "DIR".to_string()
    } else {
        format!("DIR: {}", folder_name)
    };
    draw_text(
        hdc,
        &df_label,
        df.x + 2,
        df.y,
        df.w - 4,
        df.h,
        fonts.small_bold,
        if df_hover {
            COLOR_TEXT_PRIMARY
        } else {
            COLOR_TEXT_SECONDARY
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );

    // Trackers Settings button
    let tr_btn = &geom.stream_trackers_btn;
    let tr_hover = state.hovered_control == Some(HoverControl::StreamTrackersBtn);
    draw_rounded_rect(
        hdc,
        tr_btn.x,
        tr_btn.y,
        tr_btn.w,
        tr_btn.h,
        4,
        if tr_hover {
            COLOR_SURFACE_HOVER
        } else {
            COLOR_SURFACE
        },
        Some(if tr_hover {
            COLOR_ACCENT
        } else {
            COLOR_BORDER
        }),
    );
    draw_text(
        hdc,
        "TRACKERS",
        tr_btn.x + 2,
        tr_btn.y,
        tr_btn.w - 4,
        tr_btn.h,
        fonts.small_bold,
        if tr_hover {
            COLOR_TEXT_PRIMARY
        } else {
            COLOR_TEXT_SECONDARY
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Results & Downloads content area
    let list_r = &geom.nyaa_results_list;

    if state.searching {
        draw_text(
            hdc,
            "Searching Nyaa...",
            list_r.x + 20,
            list_r.y + 40,
            list_r.w - 40,
            30,
            fonts.regular,
            COLOR_ACCENT,
            DT_CENTER | DT_SINGLELINE,
        );
        return;
    }

    if let Some(ref err) = state.nyaa_error {
        draw_text(
            hdc,
            err,
            list_r.x + 20,
            list_r.y + 40,
            list_r.w - 40,
            60,
            fonts.regular,
            COLOR_ERROR,
            DT_CENTER | DT_WORDBREAK,
        );
        return;
    }

    if state.downloads.is_empty() && state.nyaa_results.is_empty() {
        if state.torrent_status_text == "Fetching torrent file list..." {
            draw_text(
                hdc,
                "Fetching torrent file list...",
                list_r.x + 20,
                list_r.y + 40,
                list_r.w - 40,
                30,
                fonts.regular,
                COLOR_ACCENT,
                DT_CENTER | DT_SINGLELINE,
            );
            return;
        }

        draw_text(
            hdc,
            "Enter keywords to search anime, or paste a magnet: link, .torrent URL, or direct stream.\n\nYou can also drag & drop .torrent files directly into Vesper.",
            list_r.x + 20,
            list_r.y + 40,
            list_r.w - 40,
            100,
            fonts.regular,
            COLOR_TEXT_MUTED,
            DT_CENTER | DT_WORDBREAK,
        );
        return;
    }

    let mut y = list_r.y + 4 - state.nyaa_scroll_y;

    // Status banner if actively resolving torrent metadata
    if state.torrent_status_text == "Fetching torrent file list..." {
        let banner_h = 24;
        let banner_r = Rect::new(list_r.x + 12, y, list_r.w - 24, banner_h);
        draw_rounded_rect(
            hdc,
            banner_r.x,
            banner_r.y,
            banner_r.w,
            banner_r.h,
            4,
            COLOR_SURFACE,
            Some(COLOR_ACCENT),
        );
        draw_text(
            hdc,
            "Fetching torrent file list...",
            banner_r.x + 8,
            banner_r.y,
            banner_r.w - 16,
            banner_r.h,
            fonts.small_bold,
            COLOR_ACCENT,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        y += banner_h + 8;
    }

    // 1. Render Downloads Section if any tasks exist
    if !state.downloads.is_empty() {
        if y + 18 >= list_r.y && y <= list_r.bottom() {
            draw_text(
                hdc,
                &format!("DOWNLOADS ({})", state.downloads.len()),
                list_r.x + 14,
                y,
                list_r.w - 28,
                18,
                fonts.small_bold,
                COLOR_ACCENT,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
        }
        y += 22;

        for (idx, dl) in state.downloads.iter().enumerate() {
            let card_h = 68;
            if y + card_h >= list_r.y && y <= list_r.bottom() {
                let card_r = Rect::new(list_r.x + 12, y, list_r.w - 24, card_h);
                let is_card_hover = state.hovered_control == Some(HoverControl::DownloadCard(idx));

                draw_rounded_rect(
                    hdc,
                    card_r.x,
                    card_r.y,
                    card_r.w,
                    card_r.h,
                    6,
                    if is_card_hover {
                        COLOR_SURFACE_HOVER
                    } else {
                        COLOR_SURFACE
                    },
                    Some(if is_card_hover {
                        COLOR_ACCENT
                    } else {
                        COLOR_BORDER
                    }),
                );

                // Line 1: Title (left) & State Badge (right)
                let (state_text, state_color) = match &dl.state {
                    TorrentTaskState::Resolving => ("Resolving", COLOR_TEXT_MUTED),
                    TorrentTaskState::Downloading => ("Downloading", COLOR_ACCENT),
                    TorrentTaskState::Paused => ("Paused", COLOR_TEXT_SECONDARY),
                    TorrentTaskState::Completed => ("Done", COLOR_SUCCESS),
                    TorrentTaskState::Error(_) => ("Error", COLOR_ERROR),
                };

                let badge_w = 70;
                let card_inner_w = (card_r.w - 20).max(0);
                let title_w = (card_inner_w - badge_w - 10).max(0);
                draw_text(
                    hdc,
                    &dl.name,
                    card_r.x + 10,
                    card_r.y + 6,
                    title_w,
                    20,
                    fonts.bold,
                    COLOR_TEXT_PRIMARY,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );

                draw_badge(
                    hdc,
                    state_text,
                    card_r.x + card_r.w - 10 - badge_w,
                    card_r.y + 6,
                    fonts.small_bold,
                    COLOR_BG,
                    Some(state_color),
                    state_color,
                );

                // Line 2: Progress bar
                let bar_x = card_r.x + 10;
                let bar_y = card_r.y + 28;
                let bar_w = card_r.w - 20;
                let bar_h = 4;
                draw_rounded_rect(
                    hdc,
                    bar_x,
                    bar_y,
                    bar_w,
                    bar_h,
                    2,
                    COLOR_SURFACE_HOVER,
                    None,
                );
                let pct_clamped = (dl.progress_pct / 100.0).clamp(0.0, 1.0);
                let fill_w = ((bar_w as f32) * pct_clamped) as i32;
                if fill_w > 0 {
                    let fill_color = if matches!(dl.state, TorrentTaskState::Completed) {
                        COLOR_SUCCESS
                    } else {
                        COLOR_ACCENT
                    };
                    draw_rounded_rect(
                        hdc,
                        bar_x,
                        bar_y,
                        fill_w,
                        bar_h,
                        2,
                        fill_color,
                        None,
                    );
                }

                // Line 3: Stats text (left) & Action buttons (right)
                let stats_text = if matches!(dl.state, TorrentTaskState::Completed) {
                    format!("{} · Completed", format_bytes(dl.total_bytes))
                } else if matches!(dl.state, TorrentTaskState::Paused) {
                    format!("{:.1}% · Paused", dl.progress_pct)
                } else if matches!(dl.state, TorrentTaskState::Resolving) {
                    "Fetching metadata...".to_string()
                } else {
                    format!(
                        "{}/{} ({:.1}%) · {}/s",
                        format_bytes(dl.downloaded_bytes),
                        format_bytes(dl.total_bytes),
                        dl.progress_pct,
                        format_speed(dl.speed_bytes),
                    )
                };

                let btn_y = card_r.y + 38;
                let btn_h = 22;
                let is_complete = matches!(dl.state, TorrentTaskState::Completed);

                let rem_x = card_r.x + card_r.w - 10 - 24;
                let fol_x = rem_x - 6 - 48;
                let act_btn_w = if is_complete { 44 } else { 50 };
                let act_x = fol_x - 6 - act_btn_w;

                let text_max_w = (act_x - card_r.x - 14).max(0);
                draw_text(
                    hdc,
                    &stats_text,
                    card_r.x + 10,
                    btn_y,
                    text_max_w,
                    btn_h,
                    fonts.small,
                    COLOR_TEXT_MUTED,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );

                // Action button (Play / Pause / Resume)
                let is_act_hover = state.hovered_control
                    == Some(if is_complete {
                        HoverControl::DownloadPlay(idx)
                    } else {
                        HoverControl::DownloadPauseResume(idx)
                    });
                let act_label = if is_complete {
                    "Play"
                } else if matches!(dl.state, TorrentTaskState::Paused) {
                    "Resume"
                } else {
                    "Pause"
                };
                draw_rounded_rect(
                    hdc,
                    act_x,
                    btn_y,
                    act_btn_w,
                    btn_h,
                    4,
                    if is_act_hover {
                        if is_complete {
                            COLOR_ACCENT_HOVER
                        } else {
                            COLOR_SURFACE_HOVER
                        }
                    } else if is_complete {
                        COLOR_ACCENT
                    } else {
                        COLOR_SURFACE
                    },
                    Some(if is_act_hover {
                        COLOR_ACCENT
                    } else {
                        COLOR_BORDER
                    }),
                );
                draw_text(
                    hdc,
                    act_label,
                    act_x,
                    btn_y,
                    act_btn_w,
                    btn_h,
                    fonts.small_bold,
                    COLOR_TEXT_PRIMARY,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );

                // Folder button
                let is_fol_hover =
                    state.hovered_control == Some(HoverControl::DownloadFolder(idx));
                draw_rounded_rect(
                    hdc,
                    fol_x,
                    btn_y,
                    48,
                    btn_h,
                    4,
                    if is_fol_hover {
                        COLOR_SURFACE_HOVER
                    } else {
                        COLOR_SURFACE
                    },
                    Some(if is_fol_hover {
                        COLOR_ACCENT
                    } else {
                        COLOR_BORDER
                    }),
                );
                draw_text(
                    hdc,
                    "Folder",
                    fol_x,
                    btn_y,
                    48,
                    btn_h,
                    fonts.small,
                    if is_fol_hover {
                        COLOR_TEXT_PRIMARY
                    } else {
                        COLOR_TEXT_SECONDARY
                    },
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );

                // Remove / Cancel button
                let is_rem_hover =
                    state.hovered_control == Some(HoverControl::DownloadRemove(idx));
                draw_rounded_rect(
                    hdc,
                    rem_x,
                    btn_y,
                    24,
                    btn_h,
                    4,
                    if is_rem_hover {
                        COLOR_SURFACE_HOVER
                    } else {
                        COLOR_SURFACE
                    },
                    Some(if is_rem_hover {
                        COLOR_ERROR
                    } else {
                        COLOR_BORDER
                    }),
                );
                draw_text(
                    hdc,
                    "×",
                    rem_x,
                    btn_y - 1,
                    24,
                    btn_h,
                    fonts.small_bold,
                    if is_rem_hover {
                        COLOR_ERROR
                    } else {
                        COLOR_TEXT_MUTED
                    },
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            }
            y += card_h + 6;
        }
        y += 6;
    }

    // 2. Render Search Results Section
    if !state.nyaa_results.is_empty() {
        if !state.downloads.is_empty() {
            if y + 18 >= list_r.y && y <= list_r.bottom() {
                draw_text(
                    hdc,
                    "SEARCH RESULTS",
                    list_r.x + 14,
                    y,
                    list_r.w - 28,
                    18,
                    fonts.small_bold,
                    COLOR_TEXT_MUTED,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
            }
            y += 22;
        }

        let item_h = 72;
        let item_margin = 6;

        for (idx, item) in state.nyaa_results.iter().enumerate() {
            if y + item_h >= list_r.y && y <= list_r.bottom() {
                let row_r = Rect::new(list_r.x + 12, y, list_r.w - 24, item_h);
                let is_hover = state.hovered_control == Some(HoverControl::NyaaResult(idx));

                let bg_color = if is_hover {
                    COLOR_SURFACE_HOVER
                } else {
                    COLOR_BG
                };
                let border_color = if is_hover {
                    Some(COLOR_ACCENT)
                } else {
                    Some(COLOR_BORDER)
                };

                draw_rounded_rect(
                    hdc,
                    row_r.x,
                    row_r.y,
                    row_r.w,
                    row_r.h,
                    6,
                    bg_color,
                    border_color,
                );

                // Title
                draw_text(
                    hdc,
                    &item.clean_title,
                    row_r.x + 10,
                    row_r.y + 8,
                    row_r.w - 20,
                    20,
                    fonts.bold,
                    if is_hover {
                        COLOR_TEXT_PRIMARY
                    } else {
                        COLOR_TEXT_SECONDARY
                    },
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );

                let btn_y = row_r.y + 38;
                let btn_h = 22;

                if cfg!(feature = "torrent") {
                    let copy_x = row_r.x + row_r.w - 10 - 40;
                    let dl_x = copy_x - 6 - 58;
                    let watch_x = dl_x - 6 - 50;

                    let meta_text = format!("{} · {}S / {}L", item.size, item.seeders, item.leechers);
                    let meta_max_w = (watch_x - row_r.x - 14).max(0);
                    draw_text(
                        hdc,
                        &meta_text,
                        row_r.x + 10,
                        btn_y,
                        meta_max_w,
                        btn_h,
                        fonts.small,
                        COLOR_TEXT_MUTED,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                    );

                    let is_watch_hover =
                        state.hovered_control == Some(HoverControl::NyaaResultStream(idx));
                    draw_rounded_rect(
                        hdc,
                        watch_x,
                        btn_y,
                        50,
                        btn_h,
                        4,
                        if is_watch_hover {
                            COLOR_ACCENT_HOVER
                        } else {
                            COLOR_ACCENT
                        },
                        Some(COLOR_ACCENT_MUTED),
                    );
                    draw_text(
                        hdc,
                        "Watch",
                        watch_x,
                        btn_y,
                        50,
                        btn_h,
                        fonts.small_bold,
                        COLOR_TEXT_PRIMARY,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );

                    let is_dl_hover =
                        state.hovered_control == Some(HoverControl::NyaaResultDownload(idx));
                    draw_rounded_rect(
                        hdc,
                        dl_x,
                        btn_y,
                        58,
                        btn_h,
                        4,
                        if is_dl_hover {
                            COLOR_SURFACE_HOVER
                        } else {
                            COLOR_SURFACE
                        },
                        Some(if is_dl_hover {
                            COLOR_ACCENT
                        } else {
                            COLOR_BORDER
                        }),
                    );
                    draw_text(
                        hdc,
                        "Download",
                        dl_x,
                        btn_y,
                        58,
                        btn_h,
                        fonts.small_bold,
                        COLOR_TEXT_PRIMARY,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );

                    let is_copy_hover =
                        state.hovered_control == Some(HoverControl::NyaaResultCopyMagnet(idx));
                    draw_rounded_rect(
                        hdc,
                        copy_x,
                        btn_y,
                        40,
                        btn_h,
                        4,
                        if is_copy_hover {
                            COLOR_SURFACE_HOVER
                        } else {
                            COLOR_SURFACE
                        },
                        Some(if is_copy_hover {
                            COLOR_ACCENT
                        } else {
                            COLOR_BORDER
                        }),
                    );
                    draw_text(
                        hdc,
                        "Copy",
                        copy_x,
                        btn_y,
                        40,
                        btn_h,
                        fonts.small,
                        if is_copy_hover {
                            COLOR_TEXT_PRIMARY
                        } else {
                            COLOR_TEXT_SECONDARY
                        },
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );
                } else {
                    let copy_x = row_r.x + row_r.w - 10 - 40;
                    let open_x = copy_x - 6 - 76;

                    let meta_text = format!("{} · {}S / {}L", item.size, item.seeders, item.leechers);
                    let meta_max_w = (open_x - row_r.x - 14).max(0);
                    draw_text(
                        hdc,
                        &meta_text,
                        row_r.x + 10,
                        btn_y,
                        meta_max_w,
                        btn_h,
                        fonts.small,
                        COLOR_TEXT_MUTED,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                    );

                    let is_open_hover =
                        state.hovered_control == Some(HoverControl::NyaaResultOpenMagnet(idx));
                    draw_rounded_rect(
                        hdc,
                        open_x,
                        btn_y,
                        76,
                        btn_h,
                        4,
                        if is_open_hover {
                            COLOR_ACCENT_HOVER
                        } else {
                            COLOR_ACCENT
                        },
                        Some(COLOR_ACCENT_MUTED),
                    );
                    draw_text(
                        hdc,
                        "Open Client",
                        open_x,
                        btn_y,
                        76,
                        btn_h,
                        fonts.small_bold,
                        COLOR_TEXT_PRIMARY,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );

                    let is_copy_hover =
                        state.hovered_control == Some(HoverControl::NyaaResultCopyMagnet(idx));
                    draw_rounded_rect(
                        hdc,
                        copy_x,
                        btn_y,
                        40,
                        btn_h,
                        4,
                        if is_copy_hover {
                            COLOR_SURFACE_HOVER
                        } else {
                            COLOR_SURFACE
                        },
                        Some(if is_copy_hover {
                            COLOR_ACCENT
                        } else {
                            COLOR_BORDER
                        }),
                    );
                    draw_text(
                        hdc,
                        "Copy",
                        copy_x,
                        btn_y,
                        40,
                        btn_h,
                        fonts.small,
                        if is_copy_hover {
                            COLOR_TEXT_PRIMARY
                        } else {
                            COLOR_TEXT_SECONDARY
                        },
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );
                }
            }
            y += item_h + item_margin;
        }
    }

    // Category dropdown popup menu overlay
    if state.category_dropdown_open {
        let menu_x = dd.x;
        let menu_y = dd.bottom() + 4;
        let menu_w = dd.w;
        let item_h = 24;
        let menu_h = item_h * 3;

        draw_rounded_rect(
            hdc,
            menu_x,
            menu_y,
            menu_w,
            menu_h,
            4,
            COLOR_SURFACE,
            Some(COLOR_ACCENT),
        );

        let cats = [
            (NyaaCategory::AnimeEng, "Anime - Eng"),
            (NyaaCategory::AllAnime, "All Anime"),
            (NyaaCategory::AnimeRaw, "Anime - Raw"),
        ];

        for (i, (c, title)) in cats.iter().enumerate() {
            let item_y = menu_y + (i as i32) * item_h;
            let is_sel = state.category == *c;
            let is_hover = state.hovered_control == Some(HoverControl::NyaaCategoryItem(i));

            if is_hover || is_sel {
                fill_rect(
                    hdc,
                    menu_x + 2,
                    item_y,
                    menu_w - 4,
                    item_h,
                    if is_sel {
                        COLOR_CARD_ACTIVE
                    } else {
                        COLOR_SURFACE_HOVER
                    },
                );
            }

            draw_text(
                hdc,
                title,
                menu_x + 6,
                item_y,
                menu_w - 12,
                item_h,
                if is_sel {
                    fonts.small_bold
                } else {
                    fonts.small
                },
                if is_sel {
                    COLOR_ACCENT
                } else {
                    COLOR_TEXT_PRIMARY
                },
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
        }
    }
}

fn render_batch_modal(
    hdc: HDC,
    geom: &LayoutGeometry,
    modal: &BatchModalState,
    fonts: &ThemeFonts,
    state: &UiState,
) {
    let modal_w = 560.min(geom.total_width - 40);
    let modal_h = 440.min(geom.total_height - 60);
    let modal_x = (geom.total_width - modal_w) / 2;
    let modal_y = (geom.total_height - modal_h) / 2;

    // Dark dimming backdrop over main client area
    fill_rect(hdc, 0, 0, geom.total_width, geom.total_height, COLOR_VIEWPORT_BG);

    // Modal container (#141418 surface, #2A2A34 border)
    draw_rounded_rect(
        hdc,
        modal_x,
        modal_y,
        modal_w,
        modal_h,
        8,
        COLOR_SURFACE,
        Some(COLOR_BORDER),
    );

    // Header close button (26x24)
    let close_btn_w = 26;
    let close_btn_h = 24;
    let close_btn_x = modal_x + modal_w - 16 - close_btn_w;
    let close_btn_y = modal_y + 12;
    let is_close_hover = state.hovered_control == Some(HoverControl::BatchCancel);
    draw_rounded_rect(
        hdc,
        close_btn_x,
        close_btn_y,
        close_btn_w,
        close_btn_h,
        4,
        if is_close_hover { COLOR_SURFACE_HOVER } else { COLOR_SURFACE },
        Some(if is_close_hover { COLOR_ERROR } else { COLOR_BORDER }),
    );
    draw_text(
        hdc,
        "×",
        close_btn_x,
        close_btn_y - 1,
        close_btn_w,
        close_btn_h,
        fonts.bold,
        if is_close_hover { COLOR_ERROR } else { COLOR_TEXT_MUTED },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Mode badge
    let (badge_text, badge_color) = if modal.mode == BatchMode::Stream {
        ("WATCH BATCH", COLOR_ACCENT)
    } else {
        ("DOWNLOAD BATCH", COLOR_SUCCESS)
    };
    let bw = draw_badge(
        hdc,
        badge_text,
        modal_x + 16,
        modal_y + 14,
        fonts.small_bold,
        COLOR_BG,
        Some(badge_color),
        badge_color,
    );

    // Release title
    let title_x = modal_x + 22 + bw;
    let title_max_w = (close_btn_x - title_x - 12).max(0);
    draw_text(
        hdc,
        &modal.title,
        title_x,
        modal_y + 12,
        title_max_w,
        24,
        fonts.bold,
        COLOR_TEXT_PRIMARY,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );

    // Subheader row
    let sub_y = modal_y + 44;
    let sub_h = 26;
    if modal.mode == BatchMode::Stream {
        draw_text(
            hdc,
            "Select an episode to stream immediately:",
            modal_x + 16,
            sub_y,
            modal_w - 200,
            sub_h,
            fonts.small,
            COLOR_TEXT_MUTED,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        let total_size: u64 = modal.files.iter().map(|f| f.size_bytes).sum();
        let stats = format!("Total: {} ({} files)", format_bytes(total_size), modal.files.len());
        draw_text(
            hdc,
            &stats,
            modal_x + modal_w - 216,
            sub_y,
            200,
            sub_h,
            fonts.small,
            COLOR_TEXT_SECONDARY,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
        );
    } else {
        // Select All button
        let sel_all_w = 70;
        let sel_all_x = modal_x + 16;
        let is_sel_all_hover = state.hovered_control == Some(HoverControl::BatchSelectAll);
        draw_rounded_rect(
            hdc,
            sel_all_x,
            sub_y,
            sel_all_w,
            24,
            4,
            if is_sel_all_hover { COLOR_SURFACE_HOVER } else { COLOR_SURFACE },
            Some(if is_sel_all_hover { COLOR_ACCENT } else { COLOR_BORDER }),
        );
        draw_text(
            hdc,
            "Select All",
            sel_all_x,
            sub_y,
            sel_all_w,
            24,
            fonts.small,
            COLOR_TEXT_PRIMARY,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );

        // Clear button
        let clear_w = 52;
        let clear_x = sel_all_x + sel_all_w + 6;
        let is_clear_hover = state.hovered_control == Some(HoverControl::BatchClearAll);
        draw_rounded_rect(
            hdc,
            clear_x,
            sub_y,
            clear_w,
            24,
            4,
            if is_clear_hover { COLOR_SURFACE_HOVER } else { COLOR_SURFACE },
            Some(if is_clear_hover { COLOR_ACCENT } else { COLOR_BORDER }),
        );
        draw_text(
            hdc,
            "Clear",
            clear_x,
            sub_y,
            clear_w,
            24,
            fonts.small,
            COLOR_TEXT_SECONDARY,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );

        let sel_size: u64 = modal.files.iter().filter(|f| modal.selected_indices.contains(&f.index)).map(|f| f.size_bytes).sum();
        let stats = format!("Selected: {} / {} · {}", modal.selected_indices.len(), modal.files.len(), format_bytes(sel_size));
        let stats_x = clear_x + clear_w + 12;
        draw_text(
            hdc,
            &stats,
            stats_x,
            sub_y,
            (modal_x + modal_w - 16 - stats_x).max(0),
            24,
            fonts.small_bold,
            COLOR_TEXT_PRIMARY,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
        );
    }

    // Divider line
    fill_rect(hdc, modal_x + 16, sub_y + sub_h + 2, modal_w - 32, 1, COLOR_BORDER);

    // File list
    let list_x = modal_x + 16;
    let list_y = sub_y + sub_h + 6;
    let list_w = modal_w - 32;
    let list_h = modal_h - (list_y - modal_y) - 52;
    let item_h = 36;
    let mut item_y = list_y - modal.scroll_y;

    for (idx, file) in modal.files.iter().enumerate() {
        if item_y + item_h >= list_y && item_y <= list_y + list_h {
            let row_r = Rect::new(list_x, item_y, list_w, item_h - 4);
            let is_row_hover = state.hovered_control == Some(HoverControl::BatchFile(idx));
            let clean_name = crate::episodes::clean_nyaa_title(&file.path);

            if modal.mode == BatchMode::Stream {
                let is_watch_hover = state.hovered_control == Some(HoverControl::BatchItemWatch(idx));
                let bg = if is_row_hover { COLOR_SURFACE_HOVER } else { COLOR_BG };
                draw_rounded_rect(
                    hdc,
                    row_r.x,
                    row_r.y,
                    row_r.w,
                    row_r.h,
                    4,
                    bg,
                    Some(if is_row_hover { COLOR_BORDER_HOVER } else { COLOR_BORDER }),
                );

                // Episode index badge
                let ep_badge = format!("{:02}", idx + 1);
                let bw = draw_badge(
                    hdc,
                    &ep_badge,
                    row_r.x + 8,
                    row_r.y + 6,
                    fonts.small_bold,
                    COLOR_SURFACE,
                    Some(COLOR_BORDER),
                    COLOR_TEXT_MUTED,
                );

                // Watch button
                let watch_w = 54;
                let watch_h = 24;
                let watch_x = row_r.right() - 8 - watch_w;
                let watch_y = row_r.y + (row_r.h - watch_h) / 2;
                draw_rounded_rect(
                    hdc,
                    watch_x,
                    watch_y,
                    watch_w,
                    watch_h,
                    4,
                    if is_watch_hover { COLOR_ACCENT_HOVER } else { COLOR_ACCENT },
                    Some(COLOR_ACCENT_MUTED),
                );
                draw_text(
                    hdc,
                    "Watch",
                    watch_x,
                    watch_y,
                    watch_w,
                    watch_h,
                    fonts.small_bold,
                    COLOR_TEXT_PRIMARY,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );

                // Size text
                let size_str = format_bytes(file.size_bytes);
                let size_w = 68;
                let size_x = watch_x - 8 - size_w;
                draw_text(
                    hdc,
                    &size_str,
                    size_x,
                    row_r.y,
                    size_w,
                    row_r.h,
                    fonts.small,
                    COLOR_TEXT_MUTED,
                    DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
                );

                // Title
                let title_start = row_r.x + 14 + bw;
                let title_w = (size_x - title_start - 8).max(0);
                draw_text(
                    hdc,
                    &clean_name,
                    title_start,
                    row_r.y,
                    title_w,
                    row_r.h,
                    fonts.regular,
                    if is_row_hover { COLOR_TEXT_PRIMARY } else { COLOR_TEXT_SECONDARY },
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            } else {
                // Download mode
                let is_sel = modal.selected_indices.contains(&file.index);
                let bg = if is_sel { COLOR_CARD_ACTIVE } else if is_row_hover { COLOR_SURFACE_HOVER } else { COLOR_BG };
                draw_rounded_rect(
                    hdc,
                    row_r.x,
                    row_r.y,
                    row_r.w,
                    row_r.h,
                    4,
                    bg,
                    Some(if is_sel { COLOR_ACCENT } else if is_row_hover { COLOR_BORDER_HOVER } else { COLOR_BORDER }),
                );

                // Checkbox
                let box_size = 14;
                let box_x = row_r.x + 10;
                let box_y = row_r.y + (row_r.h - box_size) / 2;
                draw_rounded_rect(
                    hdc,
                    box_x,
                    box_y,
                    box_size,
                    box_size,
                    3,
                    if is_sel { COLOR_ACCENT } else { COLOR_BG },
                    Some(if is_sel { COLOR_ACCENT } else { COLOR_BORDER }),
                );
                if is_sel {
                    draw_text(
                        hdc,
                        "✓",
                        box_x,
                        box_y - 1,
                        box_size,
                        box_size,
                        fonts.small_bold,
                        COLOR_TEXT_PRIMARY,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );
                }

                // Size text
                let size_str = format_bytes(file.size_bytes);
                let size_w = 74;
                let size_x = row_r.right() - 10 - size_w;
                draw_text(
                    hdc,
                    &size_str,
                    size_x,
                    row_r.y,
                    size_w,
                    row_r.h,
                    fonts.small,
                    COLOR_TEXT_MUTED,
                    DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
                );

                // Title
                let title_start = box_x + box_size + 10;
                let title_w = (size_x - title_start - 8).max(0);
                draw_text(
                    hdc,
                    &clean_name,
                    title_start,
                    row_r.y,
                    title_w,
                    row_r.h,
                    fonts.regular,
                    if is_sel { COLOR_TEXT_PRIMARY } else { COLOR_TEXT_SECONDARY },
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
        }
        item_y += item_h;
    }

    // Scrollbar track
    let total_content_h = (modal.files.len() as i32) * item_h;
    if total_content_h > list_h && list_h > 0 {
        let sb_track_w = 4;
        let sb_x = list_x + list_w - sb_track_w;
        let thumb_h = ((list_h as f32 / total_content_h as f32) * list_h as f32).max(20.0) as i32;
        let max_scroll = (total_content_h - list_h).max(1);
        let thumb_y = list_y + ((modal.scroll_y as f32 / max_scroll as f32) * (list_h - thumb_h) as f32) as i32;
        draw_rounded_rect(hdc, sb_x, thumb_y, sb_track_w, thumb_h, 2, COLOR_BORDER_HOVER, None);
    }

    // Footer
    let footer_y = modal_y + modal_h - 44;
    let cancel_btn_w = 70;
    let cancel_btn_x = modal_x + modal_w - 16 - cancel_btn_w;
    let is_cancel_hover = state.hovered_control == Some(HoverControl::BatchCancel);

    if modal.mode == BatchMode::Stream {
        draw_text(
            hdc,
            "Tip: You can switch episodes anytime from the Playlist sidebar tab.",
            modal_x + 16,
            footer_y,
            (cancel_btn_x - modal_x - 24).max(0),
            30,
            fonts.small,
            COLOR_TEXT_MUTED,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        draw_rounded_rect(
            hdc,
            cancel_btn_x,
            footer_y,
            cancel_btn_w,
            30,
            4,
            if is_cancel_hover { COLOR_SURFACE_HOVER } else { COLOR_SURFACE },
            Some(COLOR_BORDER),
        );
        draw_text(
            hdc,
            "Cancel",
            cancel_btn_x,
            footer_y,
            cancel_btn_w,
            30,
            fonts.regular,
            COLOR_TEXT_SECONDARY,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    } else {
        let has_sel = !modal.selected_indices.is_empty();
        let dl_btn_w = 150;
        let dl_btn_x = modal_x + modal_w - 16 - dl_btn_w;
        let cancel_x = dl_btn_x - 10 - cancel_btn_w;

        draw_text(
            hdc,
            "Files will be saved to your configured download directory.",
            modal_x + 16,
            footer_y,
            (cancel_x - modal_x - 24).max(0),
            30,
            fonts.small,
            COLOR_TEXT_MUTED,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );

        // Cancel
        draw_rounded_rect(
            hdc,
            cancel_x,
            footer_y,
            cancel_btn_w,
            30,
            4,
            if is_cancel_hover { COLOR_SURFACE_HOVER } else { COLOR_SURFACE },
            Some(COLOR_BORDER),
        );
        draw_text(
            hdc,
            "Cancel",
            cancel_x,
            footer_y,
            cancel_btn_w,
            30,
            fonts.regular,
            COLOR_TEXT_SECONDARY,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );

        // Download Selected (N) button
        let is_dl_hover = state.hovered_control == Some(HoverControl::BatchConfirm);
        let btn_bg = if has_sel {
            if is_dl_hover { COLOR_ACCENT_HOVER } else { COLOR_ACCENT }
        } else {
            COLOR_SURFACE
        };
        draw_rounded_rect(
            hdc,
            dl_btn_x,
            footer_y,
            dl_btn_w,
            30,
            4,
            btn_bg,
            Some(if has_sel { COLOR_ACCENT_MUTED } else { COLOR_BORDER }),
        );
        let dl_label = format!("Download Selected ({})", modal.selected_indices.len());
        draw_text(
            hdc,
            &dl_label,
            dl_btn_x,
            footer_y,
            dl_btn_w,
            30,
            fonts.bold,
            if has_sel { COLOR_TEXT_PRIMARY } else { COLOR_TEXT_MUTED },
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
}

fn render_trackers_modal(
    hdc: HDC,
    geom: &LayoutGeometry,
    modal: &TrackersModalState,
    fonts: &ThemeFonts,
    state: &UiState,
) {
    let modal_w = 560.min(geom.total_width - 40);
    let modal_h = 440.min(geom.total_height - 60);
    let modal_x = (geom.total_width - modal_w) / 2;
    let modal_y = (geom.total_height - modal_h) / 2;

    // Dark dimming backdrop over main client area
    fill_rect(hdc, 0, 0, geom.total_width, geom.total_height, COLOR_VIEWPORT_BG);

    // Modal container (#141418 surface, #2A2A34 border)
    draw_rounded_rect(
        hdc,
        modal_x,
        modal_y,
        modal_w,
        modal_h,
        8,
        COLOR_SURFACE,
        Some(COLOR_BORDER),
    );

    // Header close button (26x24)
    let close_btn_w = 26;
    let close_btn_h = 24;
    let close_btn_x = modal_x + modal_w - 16 - close_btn_w;
    let close_btn_y = modal_y + 12;
    let is_close_hover = state.hovered_control == Some(HoverControl::TrackersModalClose);
    draw_rounded_rect(
        hdc,
        close_btn_x,
        close_btn_y,
        close_btn_w,
        close_btn_h,
        4,
        if is_close_hover { COLOR_SURFACE_HOVER } else { COLOR_SURFACE },
        Some(if is_close_hover { COLOR_ERROR } else { COLOR_BORDER }),
    );
    draw_text(
        hdc,
        "×",
        close_btn_x,
        close_btn_y - 1,
        close_btn_w,
        close_btn_h,
        fonts.bold,
        if is_close_hover { COLOR_ERROR } else { COLOR_TEXT_MUTED },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Badge
    let bw = draw_badge(
        hdc,
        "TRACKERS",
        modal_x + 16,
        modal_y + 14,
        fonts.small_bold,
        COLOR_BG,
        Some(COLOR_ACCENT),
        COLOR_ACCENT,
    );

    // Modal title
    let title_x = modal_x + 22 + bw;
    let title_max_w = (close_btn_x - title_x - 12).max(0);
    draw_text(
        hdc,
        "Public & Custom Trackers",
        title_x,
        modal_y + 12,
        title_max_w,
        24,
        fonts.heading,
        COLOR_TEXT_PRIMARY,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );

    // Auto-sync Checkbox row (modal_y + 46)
    let chk_x = modal_x + 18;
    let chk_y = modal_y + 46;
    let chk_w = 16;
    let chk_h = 16;
    let is_chk_hover = state.hovered_control == Some(HoverControl::TrackersModalAutoSyncToggle);
    draw_rounded_rect(
        hdc,
        chk_x,
        chk_y,
        chk_w,
        chk_h,
        3,
        if modal.auto_sync { COLOR_ACCENT } else { COLOR_BG },
        Some(if is_chk_hover { COLOR_ACCENT } else { COLOR_BORDER }),
    );
    if modal.auto_sync {
        draw_text(
            hdc,
            "✓",
            chk_x,
            chk_y - 1,
            chk_w,
            chk_h,
            fonts.small_bold,
            COLOR_TEXT_PRIMARY,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
    draw_text(
        hdc,
        "Auto-sync public trackers list on startup",
        chk_x + 24,
        chk_y - 2,
        modal_w - 60,
        20,
        fonts.regular,
        COLOR_TEXT_PRIMARY,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );

    // URL row (modal_y + 72)
    draw_text(
        hdc,
        "Remote Tracker List URL:",
        modal_x + 18,
        modal_y + 72,
        200,
        18,
        fonts.small_bold,
        COLOR_TEXT_SECONDARY,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
    if let Some(ref status) = modal.sync_status {
        let is_err = status.starts_with("Sync failed");
        draw_text(
            hdc,
            status,
            modal_x + 220,
            modal_y + 72,
            modal_w - 238,
            18,
            fonts.small_bold,
            if is_err { COLOR_ERROR } else { COLOR_SUCCESS },
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
        );
    }

    let sync_btn_w = 82;
    let sync_btn_h = 26;
    let sync_btn_x = modal_x + modal_w - 18 - sync_btn_w;
    let sync_btn_y = modal_y + 92;
    let is_sync_hover = state.hovered_control == Some(HoverControl::TrackersModalSyncNow);
    draw_rounded_rect(
        hdc,
        sync_btn_x,
        sync_btn_y,
        sync_btn_w,
        sync_btn_h,
        4,
        if is_sync_hover { COLOR_SURFACE_HOVER } else { COLOR_SURFACE },
        Some(if is_sync_hover { COLOR_ACCENT } else { COLOR_BORDER }),
    );
    draw_text(
        hdc,
        "Sync Now",
        sync_btn_x,
        sync_btn_y,
        sync_btn_w,
        sync_btn_h,
        fonts.small_bold,
        if is_sync_hover { COLOR_TEXT_PRIMARY } else { COLOR_TEXT_SECONDARY },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    let url_x = modal_x + 18;
    let url_y = modal_y + 92;
    let url_w = sync_btn_x - url_x - 8;
    let url_h = 26;
    let is_url_focused = modal.focused_field == TrackersFocusedField::Url;
    draw_rounded_rect(
        hdc,
        url_x,
        url_y,
        url_w,
        url_h,
        4,
        COLOR_BG,
        Some(if is_url_focused { COLOR_ACCENT } else { COLOR_BORDER }),
    );
    let url_display = if is_url_focused {
        format!("{}|", modal.tracker_url)
    } else {
        modal.tracker_url.clone()
    };
    draw_text(
        hdc,
        &url_display,
        url_x + 8,
        url_y,
        url_w - 16,
        url_h,
        fonts.small,
        COLOR_TEXT_PRIMARY,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );

    // Custom Trackers row (modal_y + 126)
    draw_text(
        hdc,
        "Custom Trackers (one per line):",
        modal_x + 18,
        modal_y + 126,
        260,
        18,
        fonts.small_bold,
        COLOR_TEXT_SECONDARY,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
    draw_text(
        hdc,
        "Appended to all torrents; original Nyaa and embedded trackers are always kept.",
        modal_x + 18,
        modal_y + 144,
        modal_w - 36,
        16,
        fonts.small,
        COLOR_TEXT_MUTED,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );

    let custom_box_x = modal_x + 18;
    let custom_box_y = modal_y + 164;
    let custom_box_w = modal_w - 36;
    let footer_y = modal_y + modal_h - 44;
    let custom_box_h = footer_y - custom_box_y - 10;
    let is_custom_focused = modal.focused_field == TrackersFocusedField::Custom;

    draw_rounded_rect(
        hdc,
        custom_box_x,
        custom_box_y,
        custom_box_w,
        custom_box_h,
        4,
        COLOR_BG,
        Some(if is_custom_focused { COLOR_ACCENT } else { COLOR_BORDER }),
    );

    let trimmed = modal.custom_text.trim();
    if trimmed.is_empty() && !is_custom_focused {
        let placeholder = "udp://tracker.opentrackr.org:1337/announce\nhttp://open.acgnxtracker.com:80/announce\n...";
        let mut py = custom_box_y + 8;
        for line in placeholder.lines() {
            draw_text(
                hdc,
                line,
                custom_box_x + 8,
                py,
                custom_box_w - 16,
                18,
                fonts.small,
                COLOR_TEXT_MUTED,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
            py += 18;
        }
    } else {
        let lines: Vec<&str> = modal.custom_text.lines().collect();
        let mut py = custom_box_y + 8;
        let line_h = 18;
        let max_visible_lines = ((custom_box_h - 16) / line_h).max(1) as usize;

        let start_idx = if lines.len() > max_visible_lines {
            lines.len() - max_visible_lines
        } else {
            0
        };

        for (i, line) in lines.iter().enumerate().skip(start_idx) {
            if py + line_h > custom_box_y + custom_box_h - 4 {
                break;
            }
            let is_last = i == lines.len().saturating_sub(1);
            let line_text = if is_last && is_custom_focused {
                format!("{line}|")
            } else {
                line.to_string()
            };
            draw_text(
                hdc,
                &line_text,
                custom_box_x + 8,
                py,
                custom_box_w - 16,
                line_h,
                fonts.small,
                COLOR_TEXT_PRIMARY,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
            py += line_h;
        }
        if lines.is_empty() && is_custom_focused {
            draw_text(
                hdc,
                "|",
                custom_box_x + 8,
                custom_box_y + 8,
                custom_box_w - 16,
                line_h,
                fonts.small,
                COLOR_TEXT_PRIMARY,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
        }
    }

    // Footer buttons
    // Reset Defaults on left
    let reset_w = 110;
    let reset_h = 30;
    let reset_x = modal_x + 18;
    let reset_y = footer_y;
    let is_reset_hover = state.hovered_control == Some(HoverControl::TrackersModalReset);
    draw_rounded_rect(
        hdc,
        reset_x,
        reset_y,
        reset_w,
        reset_h,
        4,
        if is_reset_hover { COLOR_SURFACE_HOVER } else { COLOR_SURFACE },
        Some(COLOR_BORDER),
    );
    draw_text(
        hdc,
        "Reset Defaults",
        reset_x,
        reset_y,
        reset_w,
        reset_h,
        fonts.regular,
        if is_reset_hover { COLOR_TEXT_PRIMARY } else { COLOR_TEXT_SECONDARY },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Cancel on right
    let cancel_w = 70;
    let cancel_h = 30;
    let cancel_x = modal_x + modal_w - 18 - cancel_w;
    let cancel_y = footer_y;
    let is_cancel_hover = state.hovered_control == Some(HoverControl::TrackersModalClose);
    draw_rounded_rect(
        hdc,
        cancel_x,
        cancel_y,
        cancel_w,
        cancel_h,
        4,
        if is_cancel_hover { COLOR_SURFACE_HOVER } else { COLOR_SURFACE },
        Some(COLOR_BORDER),
    );
    draw_text(
        hdc,
        "Cancel",
        cancel_x,
        cancel_y,
        cancel_w,
        cancel_h,
        fonts.regular,
        COLOR_TEXT_SECONDARY,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Save on right next to Cancel
    let save_w = 80;
    let save_h = 30;
    let save_x = cancel_x - 10 - save_w;
    let save_y = footer_y;
    let is_save_hover = state.hovered_control == Some(HoverControl::TrackersModalSave);
    draw_rounded_rect(
        hdc,
        save_x,
        save_y,
        save_w,
        save_h,
        4,
        if is_save_hover { COLOR_ACCENT_HOVER } else { COLOR_ACCENT },
        Some(COLOR_ACCENT_MUTED),
    );
    draw_text(
        hdc,
        "Save",
        save_x,
        save_y,
        save_w,
        save_h,
        fonts.bold,
        COLOR_TEXT_PRIMARY,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}

fn render_tooltip(hdc: HDC, text: &str, tx: i32, ty: i32, fonts: &ThemeFonts) {
    let (tw, th) = measure_text(hdc, text, fonts.small);
    let pad_x = 8;
    let pad_y = 4;
    let w = tw + pad_x * 2;
    let h = th + pad_y * 2;
    let x = (tx - w / 2).max(10);
    let y = ty - h - 8;

    draw_rounded_rect(hdc, x, y, w, h, 4, COLOR_SURFACE, Some(COLOR_ACCENT));
    draw_text(
        hdc,
        text,
        x + pad_x,
        y + pad_y,
        tw,
        th,
        fonts.small,
        COLOR_TEXT_PRIMARY,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}
