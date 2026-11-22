#![allow(dead_code)]

use std::path::PathBuf;
use anyhow::{Context, Result};
use winit::event_loop::EventLoopProxy;

use crate::types::{AppEvent, AudioTrack, ChapterInfo, PlayerCommand, SubTrack};

// Internal property observation identifiers
const PROP_TIME_POS: u64 = 1;
const PROP_DURATION: u64 = 2;
const PROP_PAUSE: u64 = 3;
const PROP_VOLUME: u64 = 4;
const PROP_SUB_TEXT: u64 = 5;
const PROP_CHAPTER_LIST: u64 = 6;
const PROP_TRACK_LIST: u64 = 7;
const PROP_PAUSED_FOR_CACHE: u64 = 8;
const PROP_CACHE_BUFFERING_STATE: u64 = 9;

/// High-performance libmpv player wrapper embedded inside Win32 native window.
pub struct Player {
    mpv: libmpv2::Mpv,
    current_file: Option<PathBuf>,
    event_loop_proxy: EventLoopProxy<AppEvent>,
}

impl Player {
    /// Initialize a new Player instance embedded into a native Win32 window (HWND).
    ///
    /// Configures:
    /// - `wid`: native Win32 HWND handle
    /// - `vo`: "gpu-next"
    /// - `hwdec`: "auto-safe"
    /// - `embeddedfonts`: "yes"
    /// - `sub-auto`: "fuzzy"
    /// - `keep-open`: "yes"
    /// - `set_wakeup_callback`: sends `AppEvent::MpvWakeup` to `event_loop_proxy`
    /// - Property observation: "time-pos", "duration", "pause", "volume", "sub-text", "chapter-list", "track-list"
    pub fn new(wid: isize, event_loop_proxy: EventLoopProxy<AppEvent>) -> Result<Self> {
        let mut mpv = libmpv2::Mpv::with_initializer(|init| {
            // Set required options during pre-initialization
            let _ = init.set_option("wid", wid as i64);

            if let Err(e) = init.set_option("vo", "gpu-next") {
                tracing::warn!("Failed to set vo=gpu-next: {e}, falling back to vo=gpu");
                let _ = init.set_option("vo", "gpu");
            }

            let _ = init.set_option("hwdec", "auto-safe");
            let _ = init.set_option("embeddedfonts", "yes");
            let _ = init.set_option("sub-auto", "fuzzy");
            let _ = init.set_option("slang", "eng,en,ind,id,jpn,ja");
            let _ = init.set_option("alang", "jpn,ja,eng,en");
            let _ = init.set_option("keep-open", "yes");
            let _ = init.set_option("idle", "yes");
            let _ = init.set_option("background-color", "#0E0E11");
            let _ = init.set_option("background", "#0E0E11");

            Ok(())
        })
        .context("Failed to initialize libmpv")?;

        // Ensure options/properties are active post-initialization
        let _ = mpv.set_property("wid", wid as i64);
        let _ = mpv.set_property("hwdec", "auto-safe");
        let _ = mpv.set_property("embeddedfonts", "yes");
        let _ = mpv.set_property("sub-auto", "fuzzy");
        let _ = mpv.set_property("slang", "eng,en,ind,id,jpn,ja");
        let _ = mpv.set_property("alang", "jpn,ja,eng,en");
        let _ = mpv.set_property("keep-open", "yes");
        let _ = mpv.set_property("idle", "yes");
        let _ = mpv.set_property("background-color", "#0E0E11");

        // Wakeup callback to notify winit event loop
        let proxy = event_loop_proxy.clone();
        mpv.set_wakeup_callback(move || {
            let _ = proxy.send_event(AppEvent::MpvWakeup);
        });

        // Setup property observations
        mpv.observe_property("time-pos", libmpv2::Format::Double, PROP_TIME_POS)
            .context("Failed to observe time-pos property")?;
        mpv.observe_property("duration", libmpv2::Format::Double, PROP_DURATION)
            .context("Failed to observe duration property")?;
        mpv.observe_property("pause", libmpv2::Format::Flag, PROP_PAUSE)
            .context("Failed to observe pause property")?;
        mpv.observe_property("volume", libmpv2::Format::Double, PROP_VOLUME)
            .context("Failed to observe volume property")?;
        mpv.observe_property("sub-text", libmpv2::Format::String, PROP_SUB_TEXT)
            .context("Failed to observe sub-text property")?;
        mpv.observe_property("chapter-list", libmpv2::Format::String, PROP_CHAPTER_LIST)
            .context("Failed to observe chapter-list property")?;
        mpv.observe_property("track-list", libmpv2::Format::String, PROP_TRACK_LIST)
            .context("Failed to observe track-list property")?;
        mpv.observe_property("paused-for-cache", libmpv2::Format::Flag, PROP_PAUSED_FOR_CACHE)
            .context("Failed to observe paused-for-cache property")?;
        mpv.observe_property("cache-buffering-state", libmpv2::Format::Int64, PROP_CACHE_BUFFERING_STATE)
            .context("Failed to observe cache-buffering-state property")?;

        // Ensure FileLoaded, PlaybackRestart, and PropertyChange events are enabled
        let _ = mpv.enable_event(libmpv2::events::mpv_event_id::FileLoaded);
        let _ = mpv.enable_event(libmpv2::events::mpv_event_id::PlaybackRestart);
        let _ = mpv.enable_event(libmpv2::events::mpv_event_id::PropertyChange);

        Ok(Self {
            mpv,
            current_file: None,
            event_loop_proxy,
        })
    }

    /// Alias for `Player::new`
    pub fn init(wid: isize, event_loop_proxy: EventLoopProxy<AppEvent>) -> Result<Self> {
        Self::new(wid, event_loop_proxy)
    }

    /// Load a media file path or stream URL
    pub fn load(&mut self, url_or_path: &str) -> Result<()> {
        self.current_file = Some(PathBuf::from(url_or_path));
        self.mpv
            .command("loadfile", &[url_or_path, "replace"])
            .context("Failed to execute mpv loadfile command")?;
        Ok(())
    }

    /// Resume / start playback
    pub fn play(&mut self) -> Result<()> {
        self.mpv
            .set_property("pause", false)
            .context("Failed to unpause playback")?;
        Ok(())
    }

    /// Pause playback
    pub fn pause(&mut self) -> Result<()> {
        self.mpv
            .set_property("pause", true)
            .context("Failed to pause playback")?;
        Ok(())
    }

    /// Toggle play / pause state
    pub fn toggle_pause(&mut self) -> Result<()> {
        self.mpv
            .command("cycle", &["pause"])
            .context("Failed to toggle pause")?;
        Ok(())
    }

    /// Seek relative to current position by `seconds` (negative seeks backward)
    pub fn seek_relative(&mut self, seconds: f64) -> Result<()> {
        let sec_str = seconds.to_string();
        self.mpv
            .command("seek", &[&sec_str, "relative"])
            .context("Failed to perform relative seek")?;
        Ok(())
    }

    /// Seek absolute to `seconds` position
    pub fn seek_absolute(&mut self, seconds: f64) -> Result<()> {
        let sec_str = seconds.to_string();
        self.mpv
            .command("seek", &[&sec_str, "absolute"])
            .context("Failed to perform absolute seek")?;
        Ok(())
    }

    /// Set playback volume (0.0 to 100.0+)
    pub fn set_volume(&mut self, vol: f64) -> Result<()> {
        self.mpv
            .set_property("volume", vol)
            .context("Failed to set volume")?;
        Ok(())
    }

    /// Cycle to next subtitle track
    pub fn cycle_subtitles(&mut self) -> Result<()> {
        self.mpv
            .command("cycle", &["sub"])
            .context("Failed to cycle subtitle track")?;
        Ok(())
    }

    /// Select specific subtitle track by ID (pass <= 0 to disable subtitles)
    pub fn set_subtitle_track(&mut self, id: i64) -> Result<()> {
        if id <= 0 {
            self.mpv
                .set_property("sid", "no")
                .context("Failed to disable subtitles")?;
        } else {
            self.mpv
                .set_property("sid", id)
                .context("Failed to set subtitle track")?;
        }
        Ok(())
    }

    /// Cycle to next audio track
    pub fn cycle_audio(&mut self) -> Result<()> {
        self.mpv
            .command("cycle", &["audio"])
            .context("Failed to cycle audio track")?;
        Ok(())
    }

    /// Select specific audio track by ID (pass <= 0 to disable audio)
    pub fn set_audio_track(&mut self, id: i64) -> Result<()> {
        if id <= 0 {
            self.mpv
                .set_property("aid", "no")
                .context("Failed to disable audio")?;
        } else {
            self.mpv
                .set_property("aid", id)
                .context("Failed to set audio track")?;
        }
        Ok(())
    }

    /// Stop playback
    pub fn stop(&mut self) -> Result<()> {
        self.mpv
            .command("stop", &[])
            .context("Failed to stop playback")?;
        Ok(())
    }

    /// Automatically optimizes subtitle track selection when media loads:
    /// - If currently selected subtitle track is a signs/songs or forced track, but a full dialogue
    ///   track exists for the same language, select the full dialogue track.
    /// - If no subtitle track is selected (`sid` is off/0), select the best matching subtitle track
    ///   (preferring full dialogue in English/Indonesian/Japanese).
    pub fn auto_select_best_subtitle(&mut self, tracks: &[SubTrack]) {
        if tracks.is_empty() {
            return;
        }

        let is_signs_or_forced = |t: &SubTrack| -> bool {
            t.forced
                || t.title.to_lowercase().contains("sign")
                || t.title.to_lowercase().contains("song")
        };

        let active_track = tracks.iter().find(|t| t.selected);

        if let Some(active) = active_track {
            if is_signs_or_forced(active) {
                let active_lang = active.lang.to_lowercase();
                if let Some(full) = tracks.iter().find(|t| {
                    t.id != active.id
                        && (t.lang.to_lowercase() == active_lang || active_lang == "und")
                        && !is_signs_or_forced(t)
                }) {
                    let _ = self.set_subtitle_track(full.id);
                }
            }
        } else {
            // Nothing selected: find best dialogue track (eng, en, ind, id, jpn, ja)
            let pref_langs = ["eng", "en", "ind", "id", "jpn", "ja"];
            for pref in pref_langs {
                if let Some(candidate) = tracks.iter().find(|t| {
                    t.lang.to_lowercase().starts_with(pref) && !is_signs_or_forced(t)
                }) {
                    let _ = self.set_subtitle_track(candidate.id);
                    return;
                }
            }
            if let Some(first_full) = tracks.iter().find(|t| !is_signs_or_forced(t)) {
                let _ = self.set_subtitle_track(first_full.id);
            }
        }
    }

    /// Execute a typed `PlayerCommand`
    pub fn handle_command(&mut self, cmd: PlayerCommand) -> Result<()> {
        match cmd {
            PlayerCommand::Load(url) => self.load(&url),
            PlayerCommand::Play => self.play(),
            PlayerCommand::Pause => self.pause(),
            PlayerCommand::TogglePause => self.toggle_pause(),
            PlayerCommand::SeekRelative(sec) => self.seek_relative(sec),
            PlayerCommand::SeekAbsolute(sec) => self.seek_absolute(sec),
            PlayerCommand::SetVolume(vol) => self.set_volume(vol),
            PlayerCommand::CycleSubtitle => self.cycle_subtitles(),
            PlayerCommand::SetSubtitleTrack(id) => self.set_subtitle_track(id),
            PlayerCommand::CycleAudio => self.cycle_audio(),
            PlayerCommand::SetAudioTrack(id) => self.set_audio_track(id),
            PlayerCommand::Stop => self.stop(),
        }
    }

    /// Drain all pending events from libmpv and return typed `AppEvent`s.
    ///
    /// Handles:
    /// - `FileLoaded` -> `AppEvent::FileLoaded`, initial tracks and chapters
    /// - `PropertyChange`:
    ///   - "time-pos" -> `AppEvent::TimePosChanged`
    ///   - "duration" -> `AppEvent::DurationChanged`
    ///   - "pause" -> `AppEvent::PauseChanged`
    ///   - "volume" -> `AppEvent::VolumeChanged`
    ///   - "sub-text" -> `AppEvent::SubTextChanged`
    ///   - "track-list" -> `AppEvent::TracksLoaded` & `AppEvent::AudioTracksLoaded`
    ///   - "chapter-list" -> `AppEvent::ChaptersLoaded`
    pub fn poll_events(&mut self) -> Vec<AppEvent> {
        let mut out_events = Vec::new();

        while let Some(ev_res) = self.mpv.wait_event(0.0) {
            match ev_res {
                Ok(event) => match event {
                    libmpv2::events::Event::FileLoaded => {
                        let title = self.get_media_title();
                        let duration = self.get_duration();
                        let path = self.current_file.clone();

                        out_events.push(AppEvent::FileLoaded {
                            title,
                            duration,
                            path,
                        });

                        // Smart subtitle auto-selection & track emission
                        let tracks = self.get_sub_tracks();
                        self.auto_select_best_subtitle(&tracks);
                        let updated_tracks = self.get_sub_tracks();
                        if !updated_tracks.is_empty() {
                            out_events.push(AppEvent::TracksLoaded(updated_tracks));
                        }

                        // Emit audio tracks
                        let audio_tracks = self.get_audio_tracks();
                        if !audio_tracks.is_empty() {
                            out_events.push(AppEvent::AudioTracksLoaded(audio_tracks));
                        }

                        let chapters = self.get_chapters();
                        if !chapters.is_empty() {
                            out_events.push(AppEvent::ChaptersLoaded(chapters));
                        }
                    }
                    libmpv2::events::Event::PropertyChange { name, change, .. } => {
                        match name {
                            "time-pos" => {
                                if let libmpv2::events::PropertyData::Double(pos) = change {
                                    out_events.push(AppEvent::TimePosChanged(pos));
                                } else if let libmpv2::events::PropertyData::Int64(pos) = change {
                                    out_events.push(AppEvent::TimePosChanged(pos as f64));
                                }
                            }
                            "duration" => {
                                if let libmpv2::events::PropertyData::Double(dur) = change {
                                    out_events.push(AppEvent::DurationChanged(dur));
                                } else if let libmpv2::events::PropertyData::Int64(dur) = change {
                                    out_events.push(AppEvent::DurationChanged(dur as f64));
                                }
                            }
                            "pause" => {
                                if let libmpv2::events::PropertyData::Flag(paused) = change {
                                    out_events.push(AppEvent::PauseChanged(paused));
                                }
                            }
                            "volume" => {
                                if let libmpv2::events::PropertyData::Double(vol) = change {
                                    out_events.push(AppEvent::VolumeChanged(vol));
                                } else if let libmpv2::events::PropertyData::Int64(vol) = change {
                                    out_events.push(AppEvent::VolumeChanged(vol as f64));
                                }
                            }
                            "sub-text" => match change {
                                libmpv2::events::PropertyData::Str(text) => {
                                    out_events.push(AppEvent::SubTextChanged(text.to_string()));
                                }
                                libmpv2::events::PropertyData::OsdStr(text) => {
                                    out_events.push(AppEvent::SubTextChanged(text.to_string()));
                                }
                                libmpv2::events::PropertyData::None => {
                                    out_events.push(AppEvent::SubTextChanged(String::new()));
                                }
                                _ => {}
                            },
                            "track-list" => {
                                let tracks = self.get_sub_tracks();
                                out_events.push(AppEvent::TracksLoaded(tracks));
                                let audio = self.get_audio_tracks();
                                out_events.push(AppEvent::AudioTracksLoaded(audio));
                            }
                            "chapter-list" => {
                                let chapters = self.get_chapters();
                                out_events.push(AppEvent::ChaptersLoaded(chapters));
                            }
                            "paused-for-cache" => {
                                if let libmpv2::events::PropertyData::Flag(is_buffering) = change {
                                    let percent = self.mpv.get_property::<i64>("cache-buffering-state").unwrap_or(0);
                                    out_events.push(AppEvent::CacheBuffering { is_buffering, percent });
                                }
                            }
                            "cache-buffering-state" => {
                                if let libmpv2::events::PropertyData::Int64(percent) = change {
                                    let is_buffering = self.mpv.get_property::<bool>("paused-for-cache").unwrap_or(false);
                                    out_events.push(AppEvent::CacheBuffering { is_buffering, percent });
                                }
                            }
                            _ => {}
                        }
                    }
                    libmpv2::events::Event::PlaybackRestart => {
                        out_events.push(AppEvent::PlaybackRestart);
                    }
                    libmpv2::events::Event::EndFile(reason) => {
                        if reason == libmpv2::mpv_end_file_reason::Error {
                            out_events.push(AppEvent::StatusMessage(
                                "Playback error: file corrupted or unsupported codec".to_string(),
                            ));
                        }
                    }
                    _ => {}
                },
                Err(e) => {
                    tracing::warn!("mpv wait_event returned error: {:?}", e);
                    out_events.push(AppEvent::StatusMessage(format!("Playback error: {e}")));
                }
            }
        }

        out_events
    }

    /// Query current media title or fallback to filename
    pub fn get_media_title(&self) -> String {
        if let Ok(title) = self.mpv.get_property::<String>("media-title") {
            if !title.trim().is_empty() {
                return title;
            }
        }
        if let Ok(filename) = self.mpv.get_property::<String>("filename") {
            if !filename.trim().is_empty() {
                return filename;
            }
        }
        if let Some(ref path) = self.current_file {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                return name.to_string();
            }
        }
        "Unknown".to_string()
    }

    /// Get current duration in seconds (0.0 if unknown or stopped)
    pub fn get_duration(&self) -> f64 {
        self.mpv.get_property::<f64>("duration").unwrap_or(0.0)
    }

    /// Get current playback position in seconds
    pub fn get_time_pos(&self) -> f64 {
        self.mpv.get_property::<f64>("time-pos").unwrap_or(0.0)
    }

    /// Returns true if player is currently paused
    pub fn is_paused(&self) -> bool {
        self.mpv.get_property::<bool>("pause").unwrap_or(false)
    }

    /// Query all subtitle tracks currently available in media
    pub fn get_sub_tracks(&self) -> Vec<SubTrack> {
        let mut tracks = Vec::new();
        let count: i64 = self.mpv.get_property::<i64>("track-list/count").unwrap_or(0);

        for i in 0..count {
            let track_type: String = self
                .mpv
                .get_property::<String>(&format!("track-list/{i}/type"))
                .unwrap_or_default();

            if track_type == "sub" {
                let id: i64 = self
                    .mpv
                    .get_property::<i64>(&format!("track-list/{i}/id"))
                    .unwrap_or(0);
                let title: String = self
                    .mpv
                    .get_property::<String>(&format!("track-list/{i}/title"))
                    .unwrap_or_default();
                let lang: String = self
                    .mpv
                    .get_property::<String>(&format!("track-list/{i}/lang"))
                    .unwrap_or_default();
                let raw_codec: String = self
                    .mpv
                    .get_property::<String>(&format!("track-list/{i}/codec"))
                    .unwrap_or_default();
                let codec = clean_sub_codec(&raw_codec);
                let default: bool = self
                    .mpv
                    .get_property::<bool>(&format!("track-list/{i}/default"))
                    .unwrap_or(false);
                let forced: bool = self
                    .mpv
                    .get_property::<bool>(&format!("track-list/{i}/forced"))
                    .unwrap_or(false);
                let external: bool = self
                    .mpv
                    .get_property::<bool>(&format!("track-list/{i}/external"))
                    .unwrap_or(false);
                let selected: bool = self
                    .mpv
                    .get_property::<bool>(&format!("track-list/{i}/selected"))
                    .unwrap_or(false);

                tracks.push(SubTrack {
                    id,
                    title,
                    lang,
                    codec,
                    default,
                    forced,
                    external,
                    selected,
                });
            }
        }

        tracks
    }

    /// Query all audio tracks currently available in media
    pub fn get_audio_tracks(&self) -> Vec<AudioTrack> {
        let mut tracks = Vec::new();
        let count: i64 = self.mpv.get_property::<i64>("track-list/count").unwrap_or(0);

        for i in 0..count {
            let track_type: String = self
                .mpv
                .get_property::<String>(&format!("track-list/{i}/type"))
                .unwrap_or_default();

            if track_type == "audio" {
                let id: i64 = self
                    .mpv
                    .get_property::<i64>(&format!("track-list/{i}/id"))
                    .unwrap_or(0);
                let title: String = self
                    .mpv
                    .get_property::<String>(&format!("track-list/{i}/title"))
                    .unwrap_or_default();
                let lang: String = self
                    .mpv
                    .get_property::<String>(&format!("track-list/{i}/lang"))
                    .unwrap_or_default();
                let raw_codec: String = self
                    .mpv
                    .get_property::<String>(&format!("track-list/{i}/codec"))
                    .unwrap_or_default();
                let codec = clean_audio_codec(&raw_codec);
                let channels: Option<i64> = self
                    .mpv
                    .get_property::<i64>(&format!("track-list/{i}/audio-channels"))
                    .ok();
                let default: bool = self
                    .mpv
                    .get_property::<bool>(&format!("track-list/{i}/default"))
                    .unwrap_or(false);
                let forced: bool = self
                    .mpv
                    .get_property::<bool>(&format!("track-list/{i}/forced"))
                    .unwrap_or(false);
                let external: bool = self
                    .mpv
                    .get_property::<bool>(&format!("track-list/{i}/external"))
                    .unwrap_or(false);
                let selected: bool = self
                    .mpv
                    .get_property::<bool>(&format!("track-list/{i}/selected"))
                    .unwrap_or(false);

                tracks.push(AudioTrack {
                    id,
                    title,
                    lang,
                    codec,
                    channels,
                    default,
                    forced,
                    external,
                    selected,
                });
            }
        }

        tracks
    }

    /// Query all chapters in media
    pub fn get_chapters(&self) -> Vec<ChapterInfo> {
        let mut chapters = Vec::new();
        let count: i64 = self.mpv.get_property::<i64>("chapter-list/count").unwrap_or(0);

        for i in 0..count {
            let title: String = self
                .mpv
                .get_property::<String>(&format!("chapter-list/{i}/title"))
                .unwrap_or_default();
            let time: f64 = self
                .mpv
                .get_property::<f64>(&format!("chapter-list/{i}/time"))
                .unwrap_or(0.0);

            chapters.push(ChapterInfo {
                index: i as usize,
                title,
                time,
            });
        }

        chapters
    }

    /// Reference to internal libmpv handle
    pub fn mpv(&self) -> &libmpv2::Mpv {
        &self.mpv
    }

    /// Mutable reference to internal libmpv handle
    pub fn mpv_mut(&mut self) -> &mut libmpv2::Mpv {
        &mut self.mpv
    }

    /// Event loop proxy handle
    pub fn event_loop_proxy(&self) -> &EventLoopProxy<AppEvent> {
        &self.event_loop_proxy
    }
}

/// Compact display tag for subtitle codecs
pub fn clean_sub_codec(raw: &str) -> String {
    let s = raw.trim().to_lowercase();
    if s.contains("pgs") || s.contains("hdmv") {
        "PGS".to_string()
    } else if s.contains("subrip") || s.contains("srt") || s.contains("utf8") {
        "SRT".to_string()
    } else if s.contains("ass") || s.contains("ssa") {
        "ASS".to_string()
    } else if s.contains("dvd") || s.contains("vobsub") {
        "VOB".to_string()
    } else if s.contains("webvtt") || s.contains("vtt") {
        "VTT".to_string()
    } else if s.contains("mov_text") || s.contains("tx3g") {
        "TX3G".to_string()
    } else if raw.trim().is_empty() {
        "SUB".to_string()
    } else {
        raw.trim().to_uppercase()
    }
}

/// Compact display tag for audio codecs
pub fn clean_audio_codec(raw: &str) -> String {
    let s = raw.trim().to_lowercase();
    if s.contains("opus") {
        "OPUS".to_string()
    } else if s.contains("flac") {
        "FLAC".to_string()
    } else if s.contains("aac") {
        "AAC".to_string()
    } else if s.contains("eac3") {
        "EAC3".to_string()
    } else if s.contains("ac3") {
        "AC3".to_string()
    } else if s.contains("truehd") {
        "TrueHD".to_string()
    } else if s.contains("dts") {
        "DTS".to_string()
    } else if s.contains("vorbis") {
        "OGG".to_string()
    } else if s.contains("mp3") {
        "MP3".to_string()
    } else if raw.trim().is_empty() {
        "AUD".to_string()
    } else {
        raw.trim().to_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_sub_codec() {
        assert_eq!(clean_sub_codec("hdmv_pgs_subtitle"), "PGS");
        assert_eq!(clean_sub_codec("subrip"), "SRT");
        assert_eq!(clean_sub_codec("ass"), "ASS");
        assert_eq!(clean_sub_codec("ssa"), "ASS");
        assert_eq!(clean_sub_codec("dvd_subtitle"), "VOB");
        assert_eq!(clean_sub_codec("webvtt"), "VTT");
        assert_eq!(clean_sub_codec("mov_text"), "TX3G");
    }

    #[test]
    fn test_clean_audio_codec() {
        assert_eq!(clean_audio_codec("opus"), "OPUS");
        assert_eq!(clean_audio_codec("flac"), "FLAC");
        assert_eq!(clean_audio_codec("aac"), "AAC");
        assert_eq!(clean_audio_codec("ac3"), "AC3");
        assert_eq!(clean_audio_codec("eac3"), "EAC3");
        assert_eq!(clean_audio_codec("dts"), "DTS");
        assert_eq!(clean_audio_codec("truehd"), "TrueHD");
        assert_eq!(clean_audio_codec("vorbis"), "OGG");
        assert_eq!(clean_audio_codec("mp3"), "MP3");
    }
}
