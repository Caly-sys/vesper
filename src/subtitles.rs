#![allow(dead_code)]

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
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

/// Computes standard 64-bit OpenSubtitles file hash.
/// Hash is the 64-bit sum of file size, first 64 KiB, and last 64 KiB.
pub fn compute_opensubtitles_hash(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open file: {e}"))?;
    let metadata = file.metadata().map_err(|e| format!("Failed to read metadata: {e}"))?;
    let file_size = metadata.len();

    const HASH_CHUNK_SIZE: usize = 65536; // 64 KiB

    if file_size < HASH_CHUNK_SIZE as u64 {
        return Err("File too small for OpenSubtitles hash (minimum 64 KiB required)".to_string());
    }

    let mut hash: u64 = file_size;

    // Sum first 64 KiB as 64-bit unsigned little-endian integers
    let mut buf = [0u8; HASH_CHUNK_SIZE];
    file.read_exact(&mut buf)
        .map_err(|e| format!("Failed reading first 64 KiB: {e}"))?;
    for chunk in buf.chunks_exact(8) {
        let val = u64::from_le_bytes(chunk.try_into().unwrap());
        hash = hash.wrapping_add(val);
    }

    // Sum last 64 KiB as 64-bit unsigned little-endian integers
    let seek_pos = file_size.saturating_sub(HASH_CHUNK_SIZE as u64);
    file.seek(SeekFrom::Start(seek_pos))
        .map_err(|e| format!("Failed seeking to last 64 KiB: {e}"))?;
    file.read_exact(&mut buf)
        .map_err(|e| format!("Failed reading last 64 KiB: {e}"))?;
    for chunk in buf.chunks_exact(8) {
        let val = u64::from_le_bytes(chunk.try_into().unwrap());
        hash = hash.wrapping_add(val);
    }

    Ok(format!("{:016x}", hash))
}

/// Extracts title and episode number from raw filename or release string.
pub fn extract_title_and_episode(raw: &str) -> (String, Option<f32>) {
    (
        crate::episodes::clean_nyaa_title(raw),
        crate::episodes::parse_episode_number(raw),
    )
}

pub use crate::types::SubtitleSearchResult;

/// Maps language name or ISO 639-1 code to ISO 639-2 three-letter code.
pub fn map_lang_to_iso639_2(lang: &str) -> &'static str {
    match lang.trim().to_ascii_lowercase().as_str() {
        "en" | "eng" | "english" => "eng",
        "id" | "ind" | "indonesian" => "ind",
        "ja" | "jpn" | "japanese" => "jpn",
        "es" | "spa" | "spanish" => "spa",
        "fr" | "fre" | "fra" | "french" => "fre",
        "de" | "ger" | "deu" | "german" => "ger",
        "pt" | "por" | "portuguese" | "pob" => "por",
        "it" | "ita" | "italian" => "ita",
        "ru" | "rus" | "russian" => "rus",
        "zh" | "zho" | "zht" | "chi" | "chinese" => "chi",
        "ko" | "kor" | "korean" => "kor",
        "vi" | "vie" | "vietnamese" => "vie",
        "th" | "tha" | "thai" => "tha",
        "ar" | "ara" | "arabic" => "ara",
        "hi" | "hin" | "hindi" => "hin",
        "pl" | "pol" | "polish" => "pol",
        "tr" | "tur" | "turkish" => "tur",
        _ => "eng",
    }
}

/// Checks whether a subtitle language matches a target user preferred language.
pub fn matches_language(target_lang: &str, sub_lang: &str) -> bool {
    let t_code = map_lang_to_iso639_2(target_lang);
    let s_lower = sub_lang.trim().to_ascii_lowercase();
    let s_code = map_lang_to_iso639_2(&s_lower);
    t_code == s_code || s_lower.starts_with(target_lang) || target_lang.starts_with(&s_lower)
}

/// Decompresses raw downloaded bytes if formatted as XZ or Gzip.
pub fn decompress_if_needed(data: &[u8], filename: &str) -> Vec<u8> {
    use std::io::Read;

    // 1. Check XZ magic bytes: FD 37 7A 58 5A 00
    if data.len() >= 6 && &data[0..6] == b"\xFD7zXZ\x00" {
        let mut out = Vec::new();
        if lzma_rs::xz_decompress(&mut &data[..], &mut out).is_ok() && !out.is_empty() {
            return out;
        }
    }

    // 2. Check GZ magic bytes: 1F 8B
    if data.len() >= 2 && data[0] == 0x1F && data[1] == 0x8B {
        let mut decoder = flate2::read::GzDecoder::new(&data[..]);
        let mut out = Vec::new();
        if decoder.read_to_end(&mut out).is_ok() && !out.is_empty() {
            return out;
        }
    }

    // 3. Fallback on file extension
    let lower_fn = filename.to_ascii_lowercase();
    if lower_fn.ends_with(".xz") {
        let mut out = Vec::new();
        if lzma_rs::xz_decompress(&mut &data[..], &mut out).is_ok() && !out.is_empty() {
            return out;
        }
    } else if lower_fn.ends_with(".gz") {
        let mut decoder = flate2::read::GzDecoder::new(&data[..]);
        let mut out = Vec::new();
        if decoder.read_to_end(&mut out).is_ok() && !out.is_empty() {
            return out;
        }
    }

    data.to_vec()
}

/// Strips trailing compression extensions (.xz, .gz).
pub fn clean_sub_filename(name: &str) -> String {
    let mut s = name.to_string();
    if s.ends_with(".xz") || s.ends_with(".gz") {
        s.truncate(s.len() - 3);
    }
    s
}

/// Extracts season and episode numbers from title string.
pub fn parse_season_and_episode(raw: &str) -> (u32, u32) {
    if let Ok(re) = regex::Regex::new(r"(?i)s(\d{1,2})\s*e(\d{1,3})") {
        if let Some(caps) = re.captures(raw) {
            let s = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok()).unwrap_or(1);
            let e = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok()).unwrap_or(1);
            return (s, e);
        }
    }

    let season = if let Ok(re_s) = regex::Regex::new(r"(?i)(?:season|\bs)\s*(\d{1,2})") {
        re_s.captures(raw).and_then(|caps| caps.get(1)).and_then(|m| m.as_str().parse::<u32>().ok()).unwrap_or(1)
    } else {
        1
    };

    let ep = crate::episodes::parse_episode_number(raw).map(|e| e as u32).unwrap_or(1);
    (season, ep)
}

/// Parses IMDb ID from Cinemeta top search JSON response.
pub fn parse_cinemeta_imdb_id(json_str: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(json_str).ok()?;
    json.get("metas")
        .and_then(|m| m.as_array())
        .and_then(|arr| arr.first())
        .and_then(|meta| meta.get("imdb_id").or_else(|| meta.get("id")))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string())
}

/// Parses subtitle attachments from AnimeTosho detail response JSON.
pub fn parse_animetosho_attachments(
    detail_json: &serde_json::Value,
    ep_num: Option<f32>,
    preferred_lang: &str,
    out: &mut Vec<SubtitleSearchResult>,
) {
    let files = match detail_json.get("files").and_then(|f| f.as_array()) {
        Some(f) => f,
        None => return,
    };

    let matching_files: Vec<&serde_json::Value> = if let Some(ep) = ep_num {
        let matched: Vec<&serde_json::Value> = files
            .iter()
            .filter(|f| {
                let fn_str = f.get("filename").and_then(|n| n.as_str()).unwrap_or("");
                crate::episodes::parse_episode_number(fn_str) == Some(ep)
            })
            .collect();
        if matched.is_empty() {
            files.iter().collect()
        } else {
            matched
        }
    } else {
        files.iter().collect()
    };

    for file in matching_files {
        let raw_fn = file.get("filename").and_then(|n| n.as_str()).unwrap_or("subtitle");
        let attachments = match file.get("attachments").and_then(|a| a.as_array()) {
            Some(a) => a,
            None => continue,
        };

        for att in attachments {
            let att_type = att.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if att_type != "subtitle" {
                continue;
            }

            let dl_url = match att.get("url").and_then(|u| u.as_str()) {
                Some(u) => u.to_string(),
                None => continue,
            };

            let info = att.get("info");
            let format = info.and_then(|i| i.get("format")).and_then(|f| f.as_str()).unwrap_or("ASS");
            let lang_code = info.and_then(|i| i.get("language_code")).and_then(|l| l.as_str()).unwrap_or("eng");
            let lang_name = info.and_then(|i| i.get("language")).and_then(|l| l.as_str()).unwrap_or("English");
            let title = info.and_then(|i| i.get("name")).and_then(|n| n.as_str()).unwrap_or(lang_name);

            let clean_name = format!("{}_{}.{}", clean_sub_filename(raw_fn), lang_code, format.to_ascii_lowercase());

            out.push(SubtitleSearchResult {
                provider: "AnimeTosho".to_string(),
                title: format!("{title} ({format})"),
                lang: map_lang_to_iso639_2(lang_code).to_string(),
                format: format.to_uppercase(),
                download_url: dl_url,
                file_name: clean_name,
            });
        }
    }

    let pref = preferred_lang.to_lowercase();
    out.sort_by_key(|s| if matches_language(&pref, &s.lang) { 0 } else { 1 });
}

/// Queries AnimeTosho keyless API for matching subtitle attachments.
pub async fn fetch_animetosho(
    query: &str,
    info_hash: Option<&str>,
    ep_num: Option<f32>,
    preferred_lang: &str,
) -> Result<Vec<SubtitleSearchResult>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    let search_term = if let Some(h) = info_hash {
        h.trim().to_string()
    } else {
        query.trim().to_string()
    };

    if search_term.is_empty() {
        return Ok(Vec::new());
    }

    let base_urls = ["https://feed.animetosho.org", "https://feed.animetosho.net"];
    let mut entries_json = None;

    for base in &base_urls {
        let url = format!("{base}/json?q={}", urlencoding::encode(&search_term));
        if let Ok(res) = client.get(&url).header("User-Agent", "Vesper v0.3.0").send().await {
            if res.status().is_success() {
                if let Ok(json) = res.json::<serde_json::Value>().await {
                    if json.as_array().map_or(false, |a| !a.is_empty()) {
                        entries_json = Some((base, json));
                        break;
                    }
                }
            }
        }
    }

    let (base_domain, json) = match entries_json {
        Some(pair) => pair,
        None => return Ok(Vec::new()),
    };

    let entries = json.as_array().unwrap();
    let mut results = Vec::new();

    for entry in entries.iter().take(3) {
        let id = match entry.get("id").and_then(|v| v.as_i64()) {
            Some(id) => id,
            None => continue,
        };

        let detail_url = format!("{base_domain}/json?show=torrent&id={id}");
        if let Ok(res) = client.get(&detail_url).header("User-Agent", "Vesper v0.3.0").send().await {
            if res.status().is_success() {
                if let Ok(detail_json) = res.json::<serde_json::Value>().await {
                    parse_animetosho_attachments(&detail_json, ep_num, preferred_lang, &mut results);
                }
            }
        }
    }

    Ok(results)
}

/// Queries Cinemeta to resolve IMDb ID, then queries OpenSubtitles v3 (keyless Stremio addon).
pub async fn fetch_cinemeta_opensubtitles(
    raw_title: &str,
    preferred_lang: &str,
) -> Result<Vec<SubtitleSearchResult>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    let (clean_title, ep_num) = extract_title_and_episode(raw_title);
    let (season, episode) = parse_season_and_episode(raw_title);
    let ep = ep_num.map(|n| n as u32).unwrap_or(episode);

    let mut imdb_id = None;
    let mut is_series = true;

    // 1. Cinemeta series search
    let series_url = format!(
        "https://v3-cinemeta.strem.io/catalog/series/top/search={}.json",
        urlencoding::encode(&clean_title)
    );
    if let Ok(res) = client.get(&series_url).header("User-Agent", "Vesper v0.3.0").send().await {
        if res.status().is_success() {
            if let Ok(text) = res.text().await {
                imdb_id = parse_cinemeta_imdb_id(&text);
            }
        }
    }

    // 2. Cinemeta movie search fallback
    if imdb_id.is_none() {
        let movie_url = format!(
            "https://v3-cinemeta.strem.io/catalog/movie/top/search={}.json",
            urlencoding::encode(&clean_title)
        );
        if let Ok(res) = client.get(&movie_url).header("User-Agent", "Vesper v0.3.0").send().await {
            if res.status().is_success() {
                if let Ok(text) = res.text().await {
                    imdb_id = parse_cinemeta_imdb_id(&text);
                    is_series = false;
                }
            }
        }
    }

    let imdb = match imdb_id {
        Some(id) => id,
        None => return Ok(Vec::new()),
    };

    // 3. OpenSubtitles v3 query
    let sub_url = if is_series {
        format!("https://opensubtitles-v3.strem.io/subtitles/series/{imdb}:{season}:{ep}.json")
    } else {
        format!("https://opensubtitles-v3.strem.io/subtitles/movie/{imdb}.json")
    };

    let sub_res = client
        .get(&sub_url)
        .header("User-Agent", "Vesper v0.3.0")
        .send()
        .await
        .map_err(|e| format!("OpenSubtitles v3 error: {e}"))?;

    if !sub_res.status().is_success() {
        return Ok(Vec::new());
    }

    let sub_json: serde_json::Value = sub_res
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenSubtitles v3 json: {e}"))?;

    let subtitles = match sub_json.get("subtitles").and_then(|s| s.as_array()) {
        Some(s) => s,
        None => return Ok(Vec::new()),
    };

    let mut results = Vec::new();
    for sub in subtitles {
        let dl_url = match sub.get("url").and_then(|u| u.as_str()) {
            Some(u) => u.to_string(),
            None => continue,
        };
        let lang = sub.get("lang").and_then(|l| l.as_str()).unwrap_or("eng");
        let sub_fn = sub.get("subtitleFileName").and_then(|f| f.as_str()).unwrap_or("subtitles.srt");

        results.push(SubtitleSearchResult {
            provider: "OpenSubtitles".to_string(),
            title: sub_fn.to_string(),
            lang: lang.to_string(),
            format: "SRT".to_string(),
            download_url: dl_url,
            file_name: format!("{}_{}.srt", clean_title.replace(' ', "_"), lang),
        });
    }

    let pref = preferred_lang.to_lowercase();
    results.sort_by_key(|s| if matches_language(&pref, &s.lang) { 0 } else { 1 });

    Ok(results)
}

/// Unified provider search running keyless default chain and optional API keys.
pub async fn fetch_all_keyless_subtitles(
    video_path: Option<&Path>,
    raw_title: &str,
    info_hash: Option<&str>,
    ep_num: Option<f32>,
    config: &crate::config::VesperConfig,
) -> Vec<SubtitleSearchResult> {
    let mut all = Vec::new();
    let (clean_title, extracted_ep) = extract_title_and_episode(raw_title);
    let ep = ep_num.or(extracted_ep);

    // 1. AnimeTosho (Keyless - Primary for Anime)
    if let Ok(mut at_subs) = fetch_animetosho(&clean_title, info_hash, ep, &config.preferred_sub_lang).await {
        all.append(&mut at_subs);
    }

    // 2. Cinemeta + OpenSubtitles v3 (Keyless - Primary for TV & Movies)
    if let Ok(mut os_subs) = fetch_cinemeta_opensubtitles(raw_title, &config.preferred_sub_lang).await {
        all.append(&mut os_subs);
    }

    // 3. Optional Jimaku API key override
    if let Some(ref j_key) = config.jimaku_api_key {
        if !j_key.trim().is_empty() {
            if let Ok(Some((fn_name, bytes))) = fetch_jimaku(&clean_title, ep, j_key).await {
                // If downloaded directly, provide a data URI / memory marker
                all.push(SubtitleSearchResult {
                    provider: "Jimaku".to_string(),
                    title: fn_name.clone(),
                    lang: "jpn".to_string(),
                    format: if fn_name.ends_with(".srt") { "SRT".to_string() } else { "ASS".to_string() },
                    download_url: format!("data:application/octet-stream;base64,{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes)),
                    file_name: fn_name,
                });
            }
        }
    }

    // 4. Optional OpenSubtitles REST API key override
    if let Some(ref os_key) = config.opensubtitles_api_key {
        if !os_key.trim().is_empty() {
            if let Ok(Some((fn_name, bytes))) = fetch_opensubtitles(video_path, &clean_title, ep, &config.preferred_sub_lang, Some(os_key)).await {
                all.push(SubtitleSearchResult {
                    provider: "OpenSubtitles REST".to_string(),
                    title: fn_name.clone(),
                    lang: map_lang_to_iso639_2(&config.preferred_sub_lang).to_string(),
                    format: if fn_name.ends_with(".ass") { "ASS".to_string() } else { "SRT".to_string() },
                    download_url: format!("data:application/octet-stream;base64,{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes)),
                    file_name: fn_name,
                });
            }
        }
    }

    // Prioritize preferred language matches first
    let pref_3 = map_lang_to_iso639_2(&config.preferred_sub_lang);
    all.sort_by_key(|s| {
        if s.lang == pref_3 || matches_language(&config.preferred_sub_lang, &s.lang) {
            0
        } else {
            1
        }
    });

    all
}

/// Downloads subtitle bytes from HTTP URL or data URI, applying decompression.
pub async fn download_subtitle_bytes(url: &str, filename: &str) -> Result<(String, Vec<u8>), String> {
    if let Some(b64) = url.strip_prefix("data:application/octet-stream;base64,") {
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
            .map_err(|e| format!("Base64 decode error: {e}"))?;
        let decompressed = decompress_if_needed(&bytes, filename);
        let clean_name = clean_sub_filename(filename);
        return Ok((clean_name, decompressed));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|e| e.to_string())?;

    let res = client
        .get(url)
        .header("User-Agent", "Vesper v0.3.0")
        .send()
        .await
        .map_err(|e| format!("Failed to download subtitle from {url}: {e}"))?;

    if !res.status().is_success() {
        return Err(format!("Subtitle download returned status {}", res.status()));
    }

    let bytes = res.bytes().await.map_err(|e| format!("Failed to read subtitle bytes: {e}"))?;
    let decompressed = decompress_if_needed(&bytes, filename);
    let clean_name = clean_sub_filename(filename);

    Ok((clean_name, decompressed))
}

/// Fetches subtitles from OpenSubtitles.com API.
/// Tries moviehash query first if path is provided, falling back to title & episode query.
pub async fn fetch_opensubtitles(
    path: Option<&Path>,
    title: &str,
    ep_num: Option<f32>,
    lang: &str,
    api_key: Option<&str>,
) -> Result<Option<(String, Vec<u8>)>, String> {
    let client = reqwest::Client::new();
    let mut search_json: Option<serde_json::Value> = None;

    // 1. Try moviehash query if path is provided
    if let Some(p) = path {
        if let Ok(hash) = compute_opensubtitles_hash(p) {
            let url = format!(
                "https://api.opensubtitles.com/api/v1/subtitles?languages={}&moviehash={}",
                urlencoding::encode(lang),
                urlencoding::encode(&hash)
            );
            let mut req = client.get(&url).header("User-Agent", "Vesper v0.3.0");
            if let Some(key) = api_key {
                if !key.trim().is_empty() {
                    req = req.header("Api-Key", key.trim());
                }
            }

            if let Ok(res) = req.send().await {
                if res.status().is_success() {
                    if let Ok(json) = res.json::<serde_json::Value>().await {
                        if json
                            .get("data")
                            .and_then(|d| d.as_array())
                            .map_or(false, |arr| !arr.is_empty())
                        {
                            search_json = Some(json);
                        }
                    }
                }
            }
        }
    }

    // 2. Fallback to query by title and episode number
    if search_json.is_none() {
        let mut url = format!(
            "https://api.opensubtitles.com/api/v1/subtitles?languages={}&query={}",
            urlencoding::encode(lang),
            urlencoding::encode(title)
        );
        if let Some(ep) = ep_num {
            let ep_str = if ep.fract() == 0.0 {
                (ep as u32).to_string()
            } else {
                ep.to_string()
            };
            url.push_str(&format!("&episode_number={}", urlencoding::encode(&ep_str)));
        }

        let mut req = client.get(&url).header("User-Agent", "Vesper v0.3.0");
        if let Some(key) = api_key {
            if !key.trim().is_empty() {
                req = req.header("Api-Key", key.trim());
            }
        }

        let res = req
            .send()
            .await
            .map_err(|e| format!("OpenSubtitles search request failed: {e}"))?;

        if !res.status().is_success() {
            return Err(format!(
                "OpenSubtitles search failed with status: {}",
                res.status()
            ));
        }

        let json: serde_json::Value = res
            .json()
            .await
            .map_err(|e| format!("Failed to parse OpenSubtitles search response: {e}"))?;

        if json
            .get("data")
            .and_then(|d| d.as_array())
            .map_or(false, |arr| !arr.is_empty())
        {
            search_json = Some(json);
        }
    }

    let search_json = match search_json {
        Some(j) => j,
        None => return Ok(None),
    };

    let first_file = match search_json
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("attributes"))
        .and_then(|attr| attr.get("files"))
        .and_then(|files| files.as_array())
        .and_then(|files| files.first())
    {
        Some(f) => f,
        None => return Ok(None),
    };

    let file_id = match first_file.get("file_id").and_then(|id| id.as_i64()) {
        Some(id) => serde_json::Value::Number(id.into()),
        None => match first_file.get("file_id").and_then(|id| id.as_str()) {
            Some(s) => match s.parse::<i64>() {
                Ok(id) => serde_json::Value::Number(id.into()),
                Err(_) => serde_json::Value::String(s.to_string()),
            },
            None => return Ok(None),
        },
    };

    // 3. Request download URL
    let mut dl_req = client
        .post("https://api.opensubtitles.com/api/v1/download")
        .header("User-Agent", "Vesper v0.3.0");
    if let Some(key) = api_key {
        if !key.trim().is_empty() {
            dl_req = dl_req.header("Api-Key", key.trim());
        }
    }

    let dl_res = dl_req
        .json(&serde_json::json!({ "file_id": file_id }))
        .send()
        .await
        .map_err(|e| format!("OpenSubtitles download request failed: {e}"))?;

    if !dl_res.status().is_success() {
        return Err(format!(
            "OpenSubtitles download request returned status: {}",
            dl_res.status()
        ));
    }

    let dl_json: serde_json::Value = dl_res
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenSubtitles download response: {e}"))?;

    let link = dl_json
        .get("link")
        .and_then(|l| l.as_str())
        .ok_or_else(|| "OpenSubtitles download response missing 'link'".to_string())?;

    let file_name = dl_json
        .get("file_name")
        .and_then(|n| n.as_str())
        .or_else(|| first_file.get("file_name").and_then(|n| n.as_str()))
        .unwrap_or("subtitles.srt")
        .to_string();

    let bytes = reqwest::get(link)
        .await
        .map_err(|e| format!("Failed to download subtitle file: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to read subtitle bytes: {e}"))?;

    Ok(Some((file_name, bytes.to_vec())))
}

/// Fetches Japanese/anime subtitles from Jimaku.cc API.
pub async fn fetch_jimaku(
    title: &str,
    ep_num: Option<f32>,
    api_key: &str,
) -> Result<Option<(String, Vec<u8>)>, String> {
    if api_key.trim().is_empty() {
        return Ok(None);
    }

    let client = reqwest::Client::new();

    // 1. Search entries by title
    let search_url = format!(
        "https://jimaku.cc/api/entries/search?query={}",
        urlencoding::encode(title)
    );
    let search_res = client
        .get(&search_url)
        .header("Authorization", api_key.trim())
        .header("User-Agent", "Vesper v0.3.0")
        .send()
        .await
        .map_err(|e| format!("Jimaku search request failed: {e}"))?;

    if !search_res.status().is_success() {
        return Err(format!(
            "Jimaku search failed with status: {}",
            search_res.status()
        ));
    }

    let search_json: serde_json::Value = search_res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Jimaku search JSON: {e}"))?;

    let empty_vec = Vec::new();
    let entries = search_json
        .as_array()
        .or_else(|| search_json.get("data").and_then(|d| d.as_array()))
        .unwrap_or(&empty_vec);

    if entries.is_empty() {
        return Ok(None);
    }

    let entry_id = entries[0]
        .get("id")
        .and_then(|id| id.as_i64())
        .map(|id| id.to_string())
        .or_else(|| {
            entries[0]
                .get("id")
                .and_then(|id| id.as_str())
                .map(|s| s.to_string())
        })
        .ok_or_else(|| "Jimaku entry missing 'id'".to_string())?;

    // 2. Fetch files for entry
    let files_url = format!("https://jimaku.cc/api/entries/{entry_id}/files");
    let files_res = client
        .get(&files_url)
        .header("Authorization", api_key.trim())
        .header("User-Agent", "Vesper v0.3.0")
        .send()
        .await
        .map_err(|e| format!("Jimaku files request failed: {e}"))?;

    if !files_res.status().is_success() {
        return Err(format!(
            "Jimaku files request failed with status: {}",
            files_res.status()
        ));
    }

    let files_json: serde_json::Value = files_res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Jimaku files JSON: {e}"))?;

    let files = files_json
        .as_array()
        .or_else(|| files_json.get("data").and_then(|d| d.as_array()))
        .or_else(|| files_json.get("files").and_then(|f| f.as_array()))
        .unwrap_or(&empty_vec);

    if files.is_empty() {
        return Ok(None);
    }

    // 3. Select matching episode file or fallback to first file
    let target_file = if let Some(ep) = ep_num {
        files
            .iter()
            .find(|f| {
                if let Some(f_ep) = f.get("episode").and_then(|e| e.as_f64()) {
                    if (f_ep as f32 - ep).abs() < 0.01 {
                        return true;
                    }
                }
                let name = f
                    .get("name")
                    .and_then(|n| n.as_str())
                    .or_else(|| f.get("file_name").and_then(|n| n.as_str()))
                    .or_else(|| f.get("filename").and_then(|n| n.as_str()))
                    .unwrap_or_default();
                if let Some(parsed_ep) = crate::episodes::parse_episode_number(name) {
                    if (parsed_ep - ep).abs() < 0.01 {
                        return true;
                    }
                }
                false
            })
            .unwrap_or(&files[0])
    } else {
        &files[0]
    };

    let file_name = target_file
        .get("name")
        .and_then(|n| n.as_str())
        .or_else(|| target_file.get("file_name").and_then(|n| n.as_str()))
        .or_else(|| target_file.get("filename").and_then(|n| n.as_str()))
        .unwrap_or("subtitle.ass")
        .to_string();

    let dl_url = target_file
        .get("url")
        .and_then(|u| u.as_str())
        .or_else(|| target_file.get("download_url").and_then(|u| u.as_str()))
        .or_else(|| target_file.get("link").and_then(|u| u.as_str()))
        .map(|u| {
            if u.starts_with("http://") || u.starts_with("https://") {
                u.to_string()
            } else if u.starts_with('/') {
                format!("https://jimaku.cc{u}")
            } else {
                format!("https://jimaku.cc/{u}")
            }
        })
        .or_else(|| {
            target_file
                .get("id")
                .and_then(|id| id.as_i64())
                .map(|id| format!("https://jimaku.cc/api/files/{id}"))
        })
        .ok_or_else(|| "Could not determine download URL for Jimaku file".to_string())?;

    let dl_res = client
        .get(&dl_url)
        .header("Authorization", api_key.trim())
        .header("User-Agent", "Vesper v0.3.0")
        .send()
        .await
        .map_err(|e| format!("Jimaku file download error: {e}"))?;

    if !dl_res.status().is_success() {
        return Err(format!(
            "Jimaku file download failed with status: {}",
            dl_res.status()
        ));
    }

    let bytes = dl_res
        .bytes()
        .await
        .map_err(|e| format!("Failed to read Jimaku file bytes: {e}"))?;

    Ok(Some((file_name, bytes.to_vec())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_opensubtitles_hash() {
        let temp_dir = std::env::temp_dir().join(format!(
            "vesper_sub_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        // 1. File smaller than 64 KiB should return Err
        let small_file = temp_dir.join("small.bin");
        std::fs::write(&small_file, vec![0u8; 1024]).unwrap();
        assert!(compute_opensubtitles_hash(&small_file).is_err());

        // 2. 128 KiB dummy file with all zeroes:
        // Size = 131072 (0x20000). Chunks sum to 0. Hash = 0x20000.
        let zero_file = temp_dir.join("zeroes.bin");
        std::fs::write(&zero_file, vec![0u8; 131072]).unwrap();
        let hash = compute_opensubtitles_hash(&zero_file).expect("hash computed");
        assert_eq!(hash, "0000000000020000");

        // 3. 128 KiB file with non-zero bytes
        let test_file = temp_dir.join("test.bin");
        let mut data = vec![0u8; 131072];
        data[0..8].copy_from_slice(&1u64.to_le_bytes());
        data[131072 - 8..131072].copy_from_slice(&2u64.to_le_bytes());
        std::fs::write(&test_file, &data).unwrap();

        let hash_val = compute_opensubtitles_hash(&test_file).expect("hash computed");
        // Hash = 131072 + 1 + 2 = 131075 = 0x20003
        assert_eq!(hash_val, "0000000000020003");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_extract_title_and_episode() {
        let raw1 = "[SubsPlease] Sousou no Frieren - 01 (1080p) [ABCD1234].mkv";
        let (title1, ep1) = extract_title_and_episode(raw1);
        assert_eq!(title1, "Sousou no Frieren - 01");
        assert_eq!(ep1, Some(1.0));

        let raw2 = "Spy x Family S01E05 (720p).mp4";
        let (title2, ep2) = extract_title_and_episode(raw2);
        assert_eq!(title2, "Spy x Family S01E05");
        assert_eq!(ep2, Some(5.0));

        let raw3 = "Movie Name (2024) [BD 1080p].mkv";
        let (title3, ep3) = extract_title_and_episode(raw3);
        assert_eq!(title3, "Movie Name (2024)");
        assert_eq!(ep3, None);
    }

    #[test]
    fn test_language_code_mapping() {
        assert_eq!(map_lang_to_iso639_2("en"), "eng");
        assert_eq!(map_lang_to_iso639_2("id"), "ind");
        assert_eq!(map_lang_to_iso639_2("ja"), "jpn");
        assert_eq!(map_lang_to_iso639_2("english"), "eng");
        assert_eq!(map_lang_to_iso639_2("indonesian"), "ind");
        assert_eq!(map_lang_to_iso639_2("japanese"), "jpn");
        assert_eq!(map_lang_to_iso639_2("es"), "spa");

        assert!(matches_language("en", "eng"));
        assert!(matches_language("en", "English"));
        assert!(matches_language("id", "ind"));
        assert!(matches_language("ja", "jpn"));
    }

    #[test]
    fn test_cinemeta_imdb_extraction() {
        let json = r#"{"metas":[{"id":"tt22248376","imdb_id":"tt22248376","type":"series","name":"Frieren: Beyond Journey's End"}]}"#;
        let imdb_id = parse_cinemeta_imdb_id(json);
        assert_eq!(imdb_id, Some("tt22248376".to_string()));

        let fallback_json = r#"{"metas":[{"id":"tt0185906","type":"movie","name":"Band of Brothers"}]}"#;
        let fallback_id = parse_cinemeta_imdb_id(fallback_json);
        assert_eq!(fallback_id, Some("tt0185906".to_string()));

        let empty_json = r#"{"metas":[]}"#;
        assert_eq!(parse_cinemeta_imdb_id(empty_json), None);
    }

    #[test]
    fn test_animetosho_json_parsing() {
        let sample = serde_json::json!({
            "id": 687355,
            "files": [
                {
                    "filename": "Frieren - Beyond Journey's End - S01E01.mkv",
                    "attachments": [
                        {
                            "id": 12345,
                            "type": "subtitle",
                            "url": "https://storage.animetosho.net/attachments/10003/sample.eng.ass.xz",
                            "info": {
                                "format": "ASS",
                                "language": "English",
                                "language_code": "eng",
                                "name": "Full Subtitles [9volt]"
                            }
                        },
                        {
                            "id": 12346,
                            "type": "font",
                            "url": "https://storage.animetosho.net/attachments/font.ttf"
                        }
                    ]
                }
            ]
        });

        let mut results = Vec::new();
        parse_animetosho_attachments(&sample, Some(1.0), "en", &mut results);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider, "AnimeTosho");
        assert_eq!(results[0].lang, "eng");
        assert_eq!(results[0].format, "ASS");
        assert_eq!(results[0].download_url, "https://storage.animetosho.net/attachments/10003/sample.eng.ass.xz");
    }

    #[test]
    fn test_decompress_gzip() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let original = b"1\n00:00:01,000 --> 00:00:02,000\nHello World\n";
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original).unwrap();
        let gzipped = encoder.finish().unwrap();

        let decompressed = decompress_if_needed(&gzipped, "test.srt.gz");
        assert_eq!(decompressed, original);
    }
}
