#![windows_subsystem = "windows"]

mod config;
mod episodes;
mod nyaa;
mod player;
mod subtitles;
mod torrent;
mod types;
mod ui;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use tracing::{error, info, warn};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::window::{Fullscreen, WindowBuilder};

use crate::config::VesperConfig;
use crate::player::Player;
use crate::subtitles::{extract_subtitle_track, SubCueBuffer};
use crate::torrent::TorrentEngine;
use crate::types::{AppEvent, StreamStatus};
use crate::ui::controls::Tab;
use crate::ui::{UiAction, VesperUi};

fn main() -> Result<()> {
    // 1. Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,vesper=debug")),
        )
        .try_init();

    info!("Starting Vesper Anime Player...");

    // 2. Load configuration
    let mut config = VesperConfig::load();

    // 3. Initialize background multi-threaded Tokio runtime
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("vesper-tokio")
            .build()
            .expect("Failed to initialize Tokio runtime"),
    );

    // 4. Initialize embedded TorrentEngine inside Tokio (if compiled with feature = "torrent")
    #[cfg(feature = "torrent")]
    let torrent_engine: Option<Arc<TorrentEngine>> = {
        let cache_dir = config.cache_dir.clone();
        let auto_clean = config.auto_clean_cache;
        let rt = runtime.clone();
        match rt.block_on(async move { TorrentEngine::new(cache_dir, auto_clean).await }) {
            Ok(engine) => {
                info!("TorrentEngine initialized on port {}", engine.http_port());
                Some(Arc::new(engine))
            }
            Err(e) => {
                error!("Failed to initialize TorrentEngine: {e}");
                None
            }
        }
    };
    #[cfg(not(feature = "torrent"))]
    let torrent_engine: Option<Arc<TorrentEngine>> = None;

    // 5. Create winit EventLoop with user event support
    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build()?;
    let event_proxy = event_loop.create_proxy();

    // 6. Create main application window with icon
    let icon_bytes = include_bytes!("../icon.png");
    let window_icon = image::load_from_memory(icon_bytes)
        .ok()
        .and_then(|img| {
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            winit::window::Icon::from_rgba(rgba.into_raw(), width, height).ok()
        });

    let mut window_builder = WindowBuilder::new()
        .with_title("Vesper")
        .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
        .with_min_inner_size(winit::dpi::LogicalSize::new(800.0, 480.0));

    if let Some(icon) = window_icon {
        window_builder = window_builder.with_window_icon(Some(icon));
    }

    let window = window_builder.build(&event_loop)?;

    // 7. Initialize Layout A UI (Win32 child HWND, double-buffering, and dark frame)
    let mut ui = Box::new(VesperUi::new(&window, event_proxy.clone()));
    ui.init_overlay();
    let video_hwnd = ui.video_hwnd();

    // 8. Initialize MPV embedded player
    let mut player = match Player::new(video_hwnd, event_proxy.clone()) {
        Ok(p) => {
            info!("libmpv embedded successfully into HWND 0x{:X}", video_hwnd);
            Some(p)
        }
        Err(e) => {
            error!("Failed to initialize libmpv: {e:#}");
            None
        }
    };

    if let Some(p) = player.as_mut() {
        let _ = p.set_volume(config.volume);
    }

    // 9. Runtime state trackers
    let mut current_file_path: Option<PathBuf> = None;
    let mut active_torrent_id: Option<usize> = None;
    let mut cue_buffer = SubCueBuffer::new();
    let mut is_fullscreen = false;
    let mut last_torrent_poll = Instant::now();
    let mut current_playback_time: f64 = 0.0;

    // Check command line arguments for initial .mkv file
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let target = PathBuf::from(&args[1]);
        if target.exists() {
            let target_str = target.display().to_string();
            if let Some(p) = player.as_mut() {
                let _ = p.load(&target_str);
            }
            ui.set_video_visible(true);
            current_file_path = Some(target.clone());
            let episodes = episodes::scan_sibling_episodes(&target);
            ui.handle_app_event(&AppEvent::EpisodesLoaded(episodes));
            let title = target
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Media")
                .to_string();
            ui.handle_app_event(&AppEvent::FileLoaded {
                title,
                duration: 0.0,
                path: Some(target),
            });
        }
    }

    // 10. Run main Event Loop with dynamic ControlFlow
    event_loop.run(move |event, elwt| {
        if ui.is_video_playing() {
            elwt.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50)));
        } else {
            elwt.set_control_flow(ControlFlow::Wait);
        }

        // Periodic background poll for torrent streaming progress (every 500ms)
        if let Some(torrent_id) = active_torrent_id {
            if last_torrent_poll.elapsed() >= Duration::from_millis(500) {
                last_torrent_poll = Instant::now();
                if let Some(engine) = torrent_engine.as_ref() {
                    let status = engine.poll_status(torrent_id);
                    ui.handle_app_event(&AppEvent::TorrentStatusChanged(status.clone()));

                    if let StreamStatus::Completed { file_path, .. } = status {
                        current_file_path = Some(file_path);
                    }
                }
            }
        }

        match event {
            Event::UserEvent(app_event) => {
                match app_event {
                    AppEvent::MpvWakeup => {
                        ui.check_inactivity();
                        // Drain all pending libmpv events
                        if let Some(p) = player.as_mut() {
                            let events = p.poll_events();
                            for ev in events {
                                match &ev {
                                    AppEvent::TimePosChanged(pos) => {
                                        current_playback_time = *pos;
                                        // Update active subtitle cue
                                        if let Some(cue) = cue_buffer.get_active_cue(*pos) {
                                            ui.handle_app_event(&AppEvent::SubCueEmitted(cue.clone()));
                                        }
                                        ui.handle_app_event(&ev);
                                    }
                                    AppEvent::SubTextChanged(text) => {
                                        cue_buffer.add_sub_text(text, current_playback_time);
                                        if let Some(cue) = cue_buffer.cues().last() {
                                            ui.handle_app_event(&AppEvent::SubCueEmitted(cue.clone()));
                                        }
                                        ui.handle_app_event(&ev);
                                    }
                                    AppEvent::FileLoaded { title, duration, path } => {
                                        if let Some(p) = path {
                                            current_file_path = Some(p.clone());
                                            let siblings = episodes::scan_sibling_episodes(p);
                                            ui.handle_app_event(&AppEvent::EpisodesLoaded(siblings));
                                        }
                                        ui.handle_app_event(&AppEvent::FileLoaded {
                                            title: title.clone(),
                                            duration: *duration,
                                            path: path.clone(),
                                        });
                                    }
                                    AppEvent::VolumeChanged(vol) => {
                                        config.volume = *vol;
                                        ui.handle_app_event(&ev);
                                    }
                                    _ => {
                                        ui.handle_app_event(&ev);
                                    }
                                }
                            }
                        }
                    }

                    AppEvent::TorrentStatusChanged(ref status) => {
                        if let StreamStatus::Streaming { id, port, file_idx, title, .. } = status {
                            if active_torrent_id != Some(*id) {
                                active_torrent_id = Some(*id);
                                let stream_url = format!("http://127.0.0.1:{port}/torrents/{id}/stream/{file_idx}");
                                info!("Connecting player to stream: {stream_url}");
                                if let Some(p) = player.as_mut() {
                                    let _ = p.load(&stream_url);
                                }
                                ui.set_video_visible(true);
                                cue_buffer.clear();
                                ui.handle_app_event(&AppEvent::FileLoaded {
                                    title: title.clone(),
                                    duration: 0.0,
                                    path: None,
                                });
                            }
                        }
                        ui.handle_app_event(&app_event);
                    }

                    AppEvent::EpisodesLoaded(_)
                    | AppEvent::TracksLoaded(_)
                    | AppEvent::AudioTracksLoaded(_)
                    | AppEvent::ChaptersLoaded(_)
                    | AppEvent::TimePosChanged(_)
                    | AppEvent::DurationChanged(_)
                    | AppEvent::PauseChanged(_)
                    | AppEvent::VolumeChanged(_)
                    | AppEvent::SubTextChanged(_)
                    | AppEvent::SubCueEmitted(_)
                    | AppEvent::NyaaResultsLoaded(_)
                    | AppEvent::NyaaSearchError(_)
                    | AppEvent::TorrentBatchDiscovered { .. }
                    | AppEvent::SubtitleExtracted { .. }
                    | AppEvent::SubtitleExtractionError(_)
                    | AppEvent::StatusMessage(_) => {
                        ui.handle_app_event(&app_event);
                    }

                    AppEvent::OpenFilePath(path) => {
                        let path_str = path.display().to_string();
                        info!("Opening file from native dialog: {path_str}");
                        if let Some(p) = player.as_mut() {
                            let _ = p.load(&path_str);
                        }
                        ui.set_video_visible(true);
                        current_file_path = Some(path.clone());
                        active_torrent_id = None;
                        cue_buffer.clear();

                        let siblings = episodes::scan_sibling_episodes(&path);
                        ui.handle_app_event(&AppEvent::EpisodesLoaded(siblings));
                        let filename = path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Media")
                            .to_string();
                        ui.handle_app_event(&AppEvent::FileLoaded {
                            title: filename,
                            duration: 0.0,
                            path: Some(path),
                        });
                    }

                    AppEvent::OpenFolderPath(folder) => {
                        info!("Opening folder from native dialog: {}", folder.display());
                        let ep_list = episodes::scan_folder_episodes(&folder);
                        let first_ep = ep_list.first().cloned();
                        ui.handle_app_event(&AppEvent::EpisodesLoaded(ep_list));
                        ui.state.active_tab = Tab::Episodes;
                        if let Some(ep) = first_ep {
                            let path_str = ep.path.display().to_string();
                            info!("Auto-playing episode 1 from opened folder: {path_str}");
                            if let Some(p) = player.as_mut() {
                                let _ = p.load(&path_str);
                            }
                            ui.set_video_visible(true);
                            current_file_path = Some(ep.path.clone());
                            active_torrent_id = None;
                            cue_buffer.clear();
                            ui.handle_app_event(&AppEvent::FileLoaded {
                                title: ep.clean_title,
                                duration: 0.0,
                                path: Some(ep.path),
                            });
                        }
                    }

                    AppEvent::FileLoaded { title, duration, path } => {
                        if let Some(p) = path.as_ref() {
                            current_file_path = Some(p.clone());
                            let siblings = episodes::scan_sibling_episodes(p);
                            ui.handle_app_event(&AppEvent::EpisodesLoaded(siblings));
                        }
                        ui.handle_app_event(&AppEvent::FileLoaded { title, duration, path });
                    }
                }

                window.request_redraw();
            }

            Event::WindowEvent { event, window_id } if window_id == window.id() => {
                // Pass window event to UI
                let _ = ui.handle_event(&event);

                match event {
                    WindowEvent::CloseRequested => {
                        info!("Exit requested: saving configuration and shutting down...");
                        let _ = config.save();
                        if let Some(engine) = torrent_engine.as_ref() {
                            engine.cleanup();
                        }
                        elwt.exit();
                        return;
                    }

                    WindowEvent::Moved(_) => {
                        ui.sync_overlay_position();
                    }

                    WindowEvent::Resized(physical_size) => {
                        ui.resize(physical_size.width, physical_size.height);
                    }

                    WindowEvent::RedrawRequested => {
                        ui.render();
                    }

                    _ => {}
                }

                // Process any UI actions requested by user interaction
                let actions = ui.take_actions();
                for action in actions {
                    match action {
                        UiAction::Player(cmd) => {
                            if let Some(p) = player.as_mut() {
                                let _ = p.handle_command(cmd);
                            }
                        }

                        UiAction::LoadFile(path) => {
                            let path_str = path.display().to_string();
                            info!("Loading local file: {path_str}");
                            if let Some(p) = player.as_mut() {
                                let _ = p.load(&path_str);
                            }
                            ui.set_video_visible(true);
                            current_file_path = Some(path.clone());
                            active_torrent_id = None;
                            cue_buffer.clear();

                            let siblings = episodes::scan_sibling_episodes(&path);
                            ui.handle_app_event(&AppEvent::EpisodesLoaded(siblings));
                            let filename = path
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Media")
                                .to_string();
                            ui.handle_app_event(&AppEvent::FileLoaded {
                                title: filename,
                                duration: 0.0,
                                path: Some(path),
                            });
                        }

                        UiAction::OpenFilePicker => {
                            let proxy = event_proxy.clone();
                            std::thread::spawn(move || {
                                if let Some(file) = rfd::FileDialog::new()
                                    .add_filter(
                                        "Video Files (*.mkv, *.mp4, *.webm, *.avi)",
                                        &["mkv", "mp4", "webm", "avi"],
                                    )
                                    .pick_file()
                                {
                                    let _ = proxy.send_event(AppEvent::OpenFilePath(file));
                                }
                            });
                        }

                        UiAction::OpenFolderPicker => {
                            let proxy = event_proxy.clone();
                            std::thread::spawn(move || {
                                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                    let _ = proxy.send_event(AppEvent::OpenFolderPath(folder));
                                }
                            });
                        }

                        UiAction::SearchNyaa { query, category, filter } => {
                            let proxy = event_proxy.clone();
                            let base_url = config.nyaa_base_url.clone();
                            info!("Initiating Nyaa search for: {query} [cat: {:?}, filter: {:?}]", category, filter);
                            runtime.spawn(async move {
                                match nyaa::search_nyaa(&base_url, &query, category, filter).await {
                                    Ok(items) => {
                                        info!("Nyaa query returned {} results", items.len());
                                        let _ = proxy.send_event(AppEvent::NyaaResultsLoaded(items));
                                    }
                                    Err(err) => {
                                        warn!("Nyaa search error: {err}");
                                        let _ = proxy.send_event(AppEvent::NyaaSearchError(err));
                                    }
                                }
                            });
                        }

                        UiAction::StreamNyaa(item) => {
                            let link = item.link.clone();
                            // Direct HTTP/HTTPS/HLS stream URL (e.g. mp4, mkv, m3u8)
                            if (link.starts_with("http://") || link.starts_with("https://"))
                                && !link.contains(".torrent")
                                && !link.contains("magnet:")
                            {
                                info!("Direct HTTP/HTTPS stream URL detected: {link}");
                                if let Some(p) = player.as_mut() {
                                    let _ = p.load(&link);
                                }
                                ui.set_video_visible(true);
                                current_file_path = None;
                                active_torrent_id = None;
                                cue_buffer.clear();
                                ui.handle_app_event(&AppEvent::FileLoaded {
                                    title: item.clean_title.clone(),
                                    duration: 0.0,
                                    path: None,
                                });
                                continue;
                            }

                            if let Some(engine) = torrent_engine.as_ref() {
                                let engine = engine.clone();
                                let proxy = event_proxy.clone();
                                info!("Starting stream for Nyaa torrent: {}", item.clean_title);

                                runtime.spawn(async move {
                                    let _ = proxy.send_event(AppEvent::TorrentStatusChanged(
                                        StreamStatus::Connecting { title: item.clean_title.clone() },
                                    ));

                                    match engine.add_torrent_stream(&link, None).await {
                                        Ok(status) => {
                                            let _ = proxy.send_event(AppEvent::TorrentStatusChanged(status));
                                        }
                                        Err(err) => {
                                            error!("Failed to add torrent stream: {err}");
                                            let _ = proxy.send_event(AppEvent::TorrentStatusChanged(
                                                StreamStatus::Error(err),
                                            ));
                                        }
                                    }
                                });
                            } else {
                                ui.handle_app_event(&AppEvent::StatusMessage(
                                    "Torrent streaming requires building with '--features torrent'. Direct HTTP/HTTPS streams supported.".to_string(),
                                ));
                            }
                        }

                        UiAction::SelectBatchTorrentFile { id, file_idx } => {
                            if let Some(engine) = torrent_engine.as_ref() {
                                let stream_url = engine.get_stream_url(id, file_idx);
                                let engine_clone = engine.clone();
                                let proxy = event_proxy.clone();
                                info!("Selecting batch file #{file_idx} for torrent #{id}");

                                runtime.spawn(async move {
                                    let status = engine_clone.poll_status(id);

                                    let _ = proxy.send_event(AppEvent::TorrentStatusChanged(status));
                                    let _ = proxy.send_event(AppEvent::StatusMessage(format!(
                                        "Streaming file #{file_idx} from torrent #{id}"
                                    )));

                                    let _ = proxy.send_event(AppEvent::FileLoaded {
                                        title: format!("Torrent #{id} File #{file_idx}"),
                                        duration: 0.0,
                                        path: None,
                                    });
                                });

                                if let Some(p) = player.as_mut() {
                                    let _ = p.load(&stream_url);
                                }
                                ui.set_video_visible(true);
                                active_torrent_id = Some(id);
                                cue_buffer.clear();
                            }
                        }

                        UiAction::ExtractSubtitle { track_id, destination } => {
                            if let Some(mkv_path) = current_file_path.clone() {
                                let proxy = event_proxy.clone();
                                info!("Extracting subtitle track (id={:?}) from: {}", track_id, mkv_path.display());
                                std::thread::spawn(move || {
                                    match extract_subtitle_track(&mkv_path, track_id, destination.as_deref()) {
                                        Ok(out_path) => {
                                            info!("Subtitle extracted to: {}", out_path.display());
                                            let _ = proxy.send_event(AppEvent::SubtitleExtracted {
                                                track_name: format!("Track {:?}", track_id.unwrap_or(1)),
                                                path: out_path,
                                            });
                                        }
                                        Err(err) => {
                                            error!("Subtitle extraction failed: {err}");
                                            let _ = proxy.send_event(AppEvent::SubtitleExtractionError(err));
                                        }
                                    }
                                });
                            } else {
                                ui.handle_app_event(&AppEvent::StatusMessage(
                                    "No local MKV file loaded to extract subtitles from".to_string(),
                                ));
                            }
                        }

                        UiAction::ToggleFullscreen => {
                            is_fullscreen = !is_fullscreen;
                            if is_fullscreen {
                                window.set_fullscreen(Some(Fullscreen::Borderless(None)));
                            } else {
                                window.set_fullscreen(None);
                            }
                        }
                    }
                }
            }

            Event::AboutToWait => {
                ui.check_inactivity();
            }

            _ => {}
        }
    })?;

    Ok(())
}
