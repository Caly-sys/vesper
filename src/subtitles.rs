#![allow(dead_code)]

use std::fs::File;
use std::path::{Path, PathBuf};
use crate::types::{AudioTrack, SubCue, SubTrack};

/// Live subtitle cue buffer that retains recent cues in chronological order.
#[derive(Debug, Clone)]
pub struct SubCueBuffer {
    cues: Vec<SubCue>,
    max_cues: usize,
}

impl Default for SubCueBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl SubCueBuffer {
    /// Creates a new buffer with the default capacity of 500 cues.
    pub fn new() -> Self {
        Self::with_capacity(500)
    }

    /// Creates a new buffer with a custom maximum cue limit.
    pub fn with_capacity(max_cues: usize) -> Self {
        Self {
            cues: Vec::with_capacity(max_cues.min(1000)),
            max_cues: max_cues.max(1),
        }
    }

    /// Returns a slice of all stored cues in chronological order.
    pub fn cues(&self) -> &[SubCue] {
        &self.cues
    }

    /// Number of cues currently stored.
    pub fn len(&self) -> usize {
        self.cues.len()
    }

    /// True if no cues are stored.
    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }

    /// Clears all cues from the buffer.
    pub fn clear(&mut self) {
        self.cues.clear();
    }

    /// Adds a live `sub-text` string received from the player at a given timestamp.
    /// Strips styling tags, filters duplicates, sets end times, and maintains order.
    pub fn add_sub_text(&mut self, raw_text: &str, timestamp: f64) {
        let cleaned = strip_sub_tags(raw_text);

        if cleaned.is_empty() {
            // Player emitted empty text: close the most recent active cue's end_time
            if let Some(last) = self.cues.last_mut() {
                if last.end_time.is_none() && timestamp >= last.timestamp {
                    last.end_time = Some(timestamp);
                }
            }
            return;
        }

        self.add_cue(SubCue {
            timestamp,
            text: cleaned,
            end_time: None,
        });
    }

    /// Adds a cue, filtering out duplicates and maintaining chronological order.
    pub fn add_cue(&mut self, cue: SubCue) {
        let text = cue.text.trim();
        if text.is_empty() {
            // Don't store empty cues, but close prior cue if applicable
            if let Some(last) = self.cues.last_mut() {
                if last.end_time.is_none() && cue.timestamp >= last.timestamp {
                    last.end_time = Some(cue.timestamp);
                }
            }
            return;
        }

        // Check for duplicates in recent cues (within last 5 cues)
        let recent_start = self.cues.len().saturating_sub(5);
        for existing in &mut self.cues[recent_start..] {
            if existing.text == text {
                // Same text within 0.5s is treated as duplicate broadcast
                if (existing.timestamp - cue.timestamp).abs() < 0.5 {
                    if cue.end_time.is_some() && existing.end_time.is_none() {
                        existing.end_time = cue.end_time;
                    }
                    return;
                }
            }
        }

        // If the last cue doesn't have an end time and this cue starts after it,
        // automatically set the previous cue's end time if within reasonable span (<= 10s)
        if let Some(last) = self.cues.last_mut() {
            if last.end_time.is_none() && cue.timestamp > last.timestamp {
                if cue.timestamp - last.timestamp <= 10.0 {
                    last.end_time = Some(cue.timestamp);
                }
            }
        }

        // Insert maintaining chronological order
        if self.cues.is_empty() || cue.timestamp >= self.cues.last().unwrap().timestamp {
            self.cues.push(cue);
        } else {
            let idx = self
                .cues
                .binary_search_by(|c| {
                    c.timestamp
                        .partial_cmp(&cue.timestamp)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or_else(|pos| pos);
            self.cues.insert(idx, cue);
        }

        // Enforce maximum buffer limit
        if self.cues.len() > self.max_cues {
            let excess = self.cues.len() - self.max_cues;
            self.cues.drain(0..excess);
        }
    }

    /// Looks up the currently active subtitle cue for the given playback timestamp.
    pub fn get_active_cue(&self, current_time: f64) -> Option<&SubCue> {
        if self.cues.is_empty() {
            return None;
        }

        // Scan backwards from the most recent cues before or at current_time
        for (i, cue) in self.cues.iter().enumerate().rev() {
            if cue.timestamp <= current_time {
                if let Some(end) = cue.end_time {
                    if current_time <= end {
                        return Some(cue);
                    }
                } else {
                    // If no explicit end_time, check next cue start or default 6s window
                    let next_start = self.cues.get(i + 1).map(|c| c.timestamp);
                    let max_end = match next_start {
                        Some(nxt) if nxt > cue.timestamp => nxt.min(cue.timestamp + 6.0),
                        _ => cue.timestamp + 6.0,
                    };

                    if current_time <= max_end {
                        return Some(cue);
                    }
                }
            }
        }

        None
    }
}

/// Strips ASS override tags (e.g. `{\an8}`, `{\b1}`), HTML tags, and line-break markers.
pub fn strip_sub_tags(raw: &str) -> String {
    // Replace ASS line breaks and hard spaces
    let s = raw
        .replace(r"\N", " ")
        .replace(r"\n", " ")
        .replace(r"\h", " ");

    // Remove ASS tags {...}
    let without_ass = if let Ok(re) = regex::Regex::new(r"\{[^}]*\}") {
        re.replace_all(&s, "").to_string()
    } else {
        s
    };

    // Remove HTML tags <...>
    let without_html = if let Ok(re) = regex::Regex::new(r"<[^>]*>") {
        re.replace_all(&without_ass, "").to_string()
    } else {
        without_ass
    };

    // Normalize whitespace
    if let Ok(re) = regex::Regex::new(r"\s+") {
        re.replace_all(&without_html, " ").trim().to_string()
    } else {
        without_html.trim().to_string()
    }
}

/// Copies the given text to the system clipboard using `arboard`.
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| format!("Clipboard error: {}", e))?;
    clipboard
        .set_text(text)
        .map_err(|e| format!("Failed to copy text: {}", e))?;
    Ok(())
}

/// Lists all subtitle tracks discovered inside an MKV container.
pub fn list_mkv_subtitle_tracks(mkv_path: &Path) -> Result<Vec<SubTrack>, String> {
    // First attempt using matroska-demuxer
    if let Ok(file) = File::open(mkv_path) {
        if let Ok(mkv) = matroska_demuxer::MatroskaFile::open(file) {
            let mut tracks = Vec::new();
            let mut default_found = false;

            for track in mkv.tracks() {
                if track.track_type() == matroska_demuxer::TrackType::Subtitle {
                    let id = track.track_number().get() as i64;
                    let title = track
                        .name()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("Track {}", id));
                    let lang = track
                        .language_bcp47()
                        .or_else(|| track.language())
                        .unwrap_or("und")
                        .to_string();
                    let codec = track.codec_id().to_string();
                    let default = track.flag_default();
                    let forced = track.flag_forced();
                    let selected = default;
                    if selected {
                        default_found = true;
                    }

                    tracks.push(SubTrack {
                        id,
                        title,
                        lang,
                        codec,
                        default,
                        forced,
                        external: false,
                        selected,
                    });
                }
            }

            if !tracks.is_empty() {
                if !default_found {
                    if let Some(first) = tracks.first_mut() {
                        first.selected = true;
                    }
                }
                return Ok(tracks);
            }
        }
    }

    // Fallback attempt using matroska metadata parser
    let matroska = matroska::open(mkv_path)
        .map_err(|e| format!("Failed to parse Matroska headers in '{}': {}", mkv_path.display(), e))?;

    let mut tracks = Vec::new();
    let mut default_found = false;

    for track in matroska.tracks {
        if track.is_subtitle() {
            let id = track.number as i64;
            let title = track
                .name
                .unwrap_or_else(|| format!("Track {}", id));
            let lang = track
                .language
                .as_ref()
                .map(|l| l.to_string())
                .unwrap_or_else(|| "und".to_string());
            let codec = track.codec_id;
            let default = track.default;
            let forced = track.forced;
            let selected = default;
            if selected {
                default_found = true;
            }

            tracks.push(SubTrack {
                id,
                title,
                lang,
                codec,
                default,
                forced,
                external: false,
                selected,
            });
        }
    }

    if !default_found {
        if let Some(first) = tracks.first_mut() {
            first.selected = true;
        }
    }

    Ok(tracks)
}

/// Lists all audio tracks discovered inside an MKV container.
pub fn list_mkv_audio_tracks(mkv_path: &Path) -> Result<Vec<AudioTrack>, String> {
    if let Ok(file) = File::open(mkv_path) {
        if let Ok(mkv) = matroska_demuxer::MatroskaFile::open(file) {
            let mut tracks = Vec::new();
            let mut default_found = false;

            for track in mkv.tracks() {
                if track.track_type() == matroska_demuxer::TrackType::Audio {
                    let id = track.track_number().get() as i64;
                    let title = track
                        .name()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("Audio Track {}", id));
                    let lang = track
                        .language_bcp47()
                        .or_else(|| track.language())
                        .unwrap_or("und")
                        .to_string();
                    let codec = track.codec_id().to_string();
                    let default = track.flag_default();
                    let forced = track.flag_forced();
                    let selected = default;
                    if selected {
                        default_found = true;
                    }

                    tracks.push(AudioTrack {
                        id,
                        title,
                        lang,
                        codec,
                        channels: None,
                        default,
                        forced,
                        external: false,
                        selected,
                    });
                }
            }

            if !tracks.is_empty() {
                if !default_found {
                    if let Some(first) = tracks.first_mut() {
                        first.selected = true;
                    }
                }
                return Ok(tracks);
            }
        }
    }

    Ok(Vec::new())
}

/// Pure-Rust MKV Subtitle & Font Extractor.
///
/// Reads track info from `mkv_path`, extracts the selected subtitle track (matching
/// `target_track_id` or first ASS/SRT track). Supports `S_TEXT/ASS` (reconstructing
/// ASS headers and dialogue lines) and `S_TEXT/UTF8` (SRT format).
/// Extracts embedded `.ttf` and `.otf` fonts found in MKV attachments to a sibling `fonts/` directory.
/// Returns the path to the saved subtitle file.
pub fn extract_subtitle_track(
    mkv_path: &Path,
    target_track_id: Option<i64>,
    output_path: Option<&Path>,
) -> Result<PathBuf, String> {
    if !mkv_path.exists() {
        return Err(format!("File does not exist: {}", mkv_path.display()));
    }

    let file = File::open(mkv_path)
        .map_err(|e| format!("Failed to open MKV file '{}': {}", mkv_path.display(), e))?;
    let mut mkv = matroska_demuxer::MatroskaFile::open(file)
        .map_err(|e| format!("Failed to demux MKV file '{}': {}", mkv_path.display(), e))?;

    // Find the target subtitle track
    let (target_track_num, codec_id, codec_private) = {
        let tracks = mkv.tracks();
        let sub_tracks: Vec<_> = tracks
            .iter()
            .filter(|t| t.track_type() == matroska_demuxer::TrackType::Subtitle)
            .collect();

        if sub_tracks.is_empty() {
            return Err(format!("No subtitle tracks found in '{}'", mkv_path.display()));
        }

        let selected = if let Some(id) = target_track_id {
            sub_tracks
                .into_iter()
                .find(|t| t.track_number().get() as i64 == id)
                .ok_or_else(|| format!("Subtitle track ID {} not found in '{}'", id, mkv_path.display()))?
        } else {
            // Prefer ASS, then UTF8, else first subtitle track
            sub_tracks
                .iter()
                .copied()
                .find(|t| t.codec_id() == "S_TEXT/ASS" || t.codec_id() == "S_TEXT/SSA")
                .or_else(|| sub_tracks.iter().copied().find(|t| t.codec_id() == "S_TEXT/UTF8"))
                .or_else(|| sub_tracks.first().copied())
                .unwrap()
        };

        (
            selected.track_number().get(),
            selected.codec_id().to_string(),
            selected.codec_private().map(|b| b.to_vec()),
        )
    };

    let is_srt = codec_id == "S_TEXT/UTF8";
    let is_sup = codec_id == "S_HDMV/PGS" || codec_id == "S_VOBSUB";
    let timestamp_scale = mkv.info().timestamp_scale().get();

    // Demux all frames for the chosen track
    struct RawSubFrame {
        timestamp: u64,
        duration: Option<u64>,
        data: Vec<u8>,
    }

    let mut frames: Vec<RawSubFrame> = Vec::new();
    let mut frame = matroska_demuxer::Frame::default();

    while mkv
        .next_frame(&mut frame)
        .map_err(|e| format!("Demux error in '{}': {}", mkv_path.display(), e))?
    {
        if frame.track == target_track_num {
            frames.push(RawSubFrame {
                timestamp: frame.timestamp,
                duration: frame.duration,
                data: std::mem::take(&mut frame.data),
            });
        }
    }

    // Determine target output path
    let final_output = if let Some(p) = output_path {
        p.to_path_buf()
    } else {
        let parent = mkv_path.parent().unwrap_or(Path::new("."));
        let stem = mkv_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("subtitle");
        let ext = if is_srt {
            "srt"
        } else if is_sup {
            "sup"
        } else {
            "ass"
        };
        parent.join(format!("{}.{}", stem, ext))
    };

    if let Some(parent) = final_output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create output directory '{}': {}", parent.display(), e))?;
    }

    let out_dir = final_output.parent().unwrap_or(Path::new("."));

    // Timestamp conversion helper
    let to_ms = |ts: u64| -> u64 {
        ((ts as u128 * timestamp_scale as u128) / 1_000_000) as u64
    };

    if is_srt {
        // SRT reconstruction
        struct SrtEntry {
            start_ms: u64,
            end_ms: u64,
            text: String,
        }

        let mut srt_entries: Vec<SrtEntry> = Vec::with_capacity(frames.len());
        for f in &frames {
            let start_ms = to_ms(f.timestamp);
            let duration_ms = f.duration.map(to_ms).unwrap_or(2500);
            let end_ms = start_ms + duration_ms;
            let text = String::from_utf8_lossy(&f.data).trim().to_string();
            if !text.is_empty() {
                srt_entries.push(SrtEntry {
                    start_ms,
                    end_ms,
                    text,
                });
            }
        }

        srt_entries.sort_by_key(|e| e.start_ms);

        let mut srt_content = String::new();
        for (idx, e) in srt_entries.iter().enumerate() {
            srt_content.push_str(&format!(
                "{}\n{} --> {}\n{}\n\n",
                idx + 1,
                format_srt_time(e.start_ms),
                format_srt_time(e.end_ms),
                e.text
            ));
        }

        std::fs::write(&final_output, srt_content.as_bytes())
            .map_err(|e| format!("Failed to write SRT file '{}': {}", final_output.display(), e))?;
    } else if is_sup {
        // PGS (.SUP) Blu-ray bitmap subtitle reconstruction
        let mut sup_bytes = Vec::new();
        for f in &frames {
            let pts_90k = (to_ms(f.timestamp) * 90) as u32;
            let dts_90k = 0u32;
            if f.data.len() >= 2 && f.data[0] == 0x50 && f.data[1] == 0x47 {
                sup_bytes.extend_from_slice(&f.data);
            } else {
                let mut offset = 0;
                while offset + 3 <= f.data.len() {
                    let seg_len = u16::from_be_bytes([f.data[offset + 1], f.data[offset + 2]]) as usize;
                    let total_seg = 3 + seg_len;
                    if offset + total_seg > f.data.len() {
                        break;
                    }
                    sup_bytes.extend_from_slice(&[0x50, 0x47]);
                    sup_bytes.extend_from_slice(&pts_90k.to_be_bytes());
                    sup_bytes.extend_from_slice(&dts_90k.to_be_bytes());
                    sup_bytes.extend_from_slice(&f.data[offset..offset + total_seg]);
                    offset += total_seg;
                }
            }
        }
        std::fs::write(&final_output, &sup_bytes)
            .map_err(|e| format!("Failed to write SUP file '{}': {}", final_output.display(), e))?;
    } else {
        // ASS / SSA reconstruction
        struct AssDialogue {
            read_order: i64,
            start_ms: u64,
            line: String,
        }

        let mut dialogues: Vec<AssDialogue> = Vec::with_capacity(frames.len());

        for (idx, f) in frames.iter().enumerate() {
            let start_ms = to_ms(f.timestamp);
            let duration_ms = f.duration.map(to_ms).unwrap_or(2500);
            let end_ms = start_ms + duration_ms;

            let text = String::from_utf8_lossy(&f.data);
            let trimmed = text.trim_end_matches(['\r', '\n']);

            let parts: Vec<&str> = trimmed.splitn(3, ',').collect();
            if parts.len() >= 3 && parts[0].trim().parse::<i64>().is_ok() {
                let read_order = parts[0].trim().parse::<i64>().unwrap_or(idx as i64);
                let layer = parts[1].trim();
                let rest = parts[2];
                let line = format!(
                    "Dialogue: {},{},{},{}",
                    layer,
                    format_ass_time(start_ms),
                    format_ass_time(end_ms),
                    rest
                );
                dialogues.push(AssDialogue {
                    read_order,
                    start_ms,
                    line,
                });
            } else if trimmed.starts_with("Dialogue:") || trimmed.starts_with("Comment:") {
                dialogues.push(AssDialogue {
                    read_order: idx as i64,
                    start_ms,
                    line: trimmed.to_string(),
                });
            } else {
                let line = format!(
                    "Dialogue: 0,{},{},Default,,0,0,0,,{}",
                    format_ass_time(start_ms),
                    format_ass_time(end_ms),
                    trimmed
                );
                dialogues.push(AssDialogue {
                    read_order: idx as i64,
                    start_ms,
                    line,
                });
            }
        }

        dialogues.sort_by(|a, b| {
            a.read_order
                .cmp(&b.read_order)
                .then_with(|| a.start_ms.cmp(&b.start_ms))
        });

        let mut ass_content = String::new();
        let private_str = codec_private
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default();

        if !private_str.trim().is_empty() {
            let trimmed_private = private_str.trim();
            ass_content.push_str(trimmed_private);
            ass_content.push('\n');

            if !ass_content.contains("[Events]") {
                ass_content.push_str("\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
            } else if !ass_content.contains("Format:") {
                ass_content.push_str("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
            }
        } else {
            ass_content.push_str("[Script Info]\n");
            ass_content.push_str("Title: Extracted Subtitles\n");
            ass_content.push_str("ScriptType: v4.00+\n");
            ass_content.push_str("WrapStyle: 0\n");
            ass_content.push_str("ScaledBorderAndShadow: yes\n");
            ass_content.push_str("PlayResX: 1920\n");
            ass_content.push_str("PlayResY: 1080\n\n");
            ass_content.push_str("[V4+ Styles]\n");
            ass_content.push_str("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
            ass_content.push_str("Style: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,0,2,10,10,10,1\n\n");
            ass_content.push_str("[Events]\n");
            ass_content.push_str("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
        }

        for d in dialogues {
            ass_content.push_str(&d.line);
            ass_content.push('\n');
        }

        std::fs::write(&final_output, ass_content.as_bytes())
            .map_err(|e| format!("Failed to write ASS file '{}': {}", final_output.display(), e))?;
    }

    if !is_sup {
        // Extract embedded fonts from MKV attachments (only needed for ASS/SSA)
        extract_embedded_fonts(mkv_path, out_dir);
    }

    Ok(final_output)
}

/// Formats milliseconds into ASS timestamp format `H:MM:SS.cs`.
fn format_ass_time(ms: u64) -> String {
    let total_cs = ms / 10;
    let cs = total_cs % 100;
    let total_s = total_cs / 100;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    format!("{}:{:02}:{:02}.{:02}", h, m, s, cs)
}

/// Formats milliseconds into SRT timestamp format `HH:MM:SS,mmm`.
fn format_srt_time(ms: u64) -> String {
    let millis = ms % 1000;
    let total_s = ms / 1000;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, millis)
}

/// Extracts embedded font attachments (.ttf, .otf, .ttc, .woff) into a sibling `fonts/` directory.
fn extract_embedded_fonts(mkv_path: &Path, base_dir: &Path) {
    let matroska_file = match matroska::open(mkv_path) {
        Ok(m) => m,
        Err(_) => return,
    };

    if matroska_file.attachments.is_empty() {
        return;
    }

    let fonts_dir = base_dir.join("fonts");

    for attachment in &matroska_file.attachments {
        if is_font_attachment(&attachment.name, &attachment.mime_type) && !attachment.data.is_empty() {
            let _ = std::fs::create_dir_all(&fonts_dir);
            let safe_name = Path::new(&attachment.name)
                .file_name()
                .unwrap_or(std::ffi::OsStr::new("font.ttf"));
            let dest_path = fonts_dir.join(safe_name);
            let _ = std::fs::write(dest_path, &attachment.data);
        }
    }
}

/// Checks whether an attachment filename or MIME type indicates a font.
fn is_font_attachment(name: &str, mime_type: &str) -> bool {
    let name_lower = name.to_ascii_lowercase();
    if name_lower.ends_with(".ttf")
        || name_lower.ends_with(".otf")
        || name_lower.ends_with(".ttc")
        || name_lower.ends_with(".woff")
        || name_lower.ends_with(".woff2")
    {
        return true;
    }

    let mime_lower = mime_type.to_ascii_lowercase();
    mime_lower.contains("font")
        || mime_lower.contains("truetype")
        || mime_lower.contains("opentype")
}
