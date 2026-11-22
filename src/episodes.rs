use std::path::Path;
use regex::Regex;
use crate::types::EpisodeEntry;

/// Strips Nyaa release group tags, video/audio encoding parameters, CRC hashes,
/// file extensions, and extra whitespace to generate a clean human-readable title.
pub fn clean_nyaa_title(raw: &str) -> String {
    // If a full path was passed, isolate just the file name
    let filename = Path::new(raw)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(raw);

    // Strip case-insensitive media extensions (.mkv, .mp4, .avi, etc.)
    let mut s = if let Ok(ext_re) = Regex::new(r"(?i)\.(mkv|mp4|avi|webm|ts|m4v)$") {
        ext_re.replace(filename, "").to_string()
    } else {
        filename.to_string()
    };

    // Replace underscores with spaces
    s = s.replace('_', " ");

    // Remove square bracket tags like [SubsPlease], [Erai-raws], [1080p], [HEVC], [CRC32]
    if let Ok(bracket_re) = Regex::new(r"\[[^\]]*\]") {
        s = bracket_re.replace_all(&s, "").to_string();
    }

    // Remove technical parentheses tags like (1080p), (720p), (HEVC), (x265), (10bit), (Dual-Audio), (CRC)
    if let Ok(paren_re) = Regex::new(r"\((?i:[^)]*(?:1080|720|480|2160|4k|hevc|x26[45]|av1|bit|audio|flac|aac|opus|bd|bluray|blu-ray|dvd|web|remux|crc)[^)]*)\)") {
        s = paren_re.replace_all(&s, "").to_string();
    }

    // Remove 8-digit hexadecimal CRC tags in parentheses like (ABCD1234)
    if let Ok(crc_re) = Regex::new(r"\([0-9a-fA-F]{8}\)") {
        s = crc_re.replace_all(&s, "").to_string();
    }

    // Clean up multiple spaces, dangling hyphens, and whitespace
    if let Ok(space_re) = Regex::new(r"\s+") {
        s = space_re.replace_all(&s, " ").to_string();
    }

    let mut trimmed = s.trim().to_string();

    // Strip trailing hyphens or separators left behind from removing trailing tags
    while trimmed.ends_with('-') || trimmed.ends_with('_') {
        trimmed.pop();
        trimmed = trimmed.trim().to_string();
    }

    trimmed
}

/// Parses episode number from title or filename (e.g. " - 01", " E05", " 12 ", "Ep 03", "12.5").
pub fn parse_episode_number(raw: &str) -> Option<f32> {
    // 1. Explicit SxxExx or Exx or Ep xx
    if let Ok(re_ep) = Regex::new(r"(?i)(?:s\d+\s*)?(?:e|ep|ep\.|episode)\s*(\d+(?:\.\d+)?)") {
        if let Some(caps) = re_ep.captures(raw) {
            if let Some(m) = caps.get(1) {
                if let Ok(val) = m.as_str().parse::<f32>() {
                    return Some(val);
                }
            }
        }
    }

    // 2. Standard anime hyphen separator: " - 01", " - 01.5", " - 12v2"
    if let Ok(re_hyphen) = Regex::new(r"(?:^|\s)-\s*(\d+(?:\.\d+)?)(?:v\d+)?(?:\s|$|\[|\(|\.)") {
        if let Some(caps) = re_hyphen.captures(raw) {
            if let Some(m) = caps.get(1) {
                if let Ok(val) = m.as_str().parse::<f32>() {
                    return Some(val);
                }
            }
        }
    }

    // 3. Hash notation: "#05", "# 05"
    if let Ok(re_hash) = Regex::new(r"(?:^|\s)#\s*(\d+(?:\.\d+)?)(?:v\d+)?(?:\s|$|\[|\(|\.)") {
        if let Some(caps) = re_hash.captures(raw) {
            if let Some(m) = caps.get(1) {
                if let Ok(val) = m.as_str().parse::<f32>() {
                    return Some(val);
                }
            }
        }
    }

    // 4. Trailing number or isolated number before tags (e.g. "Show Name 12", "Title 01.5")
    if let Ok(re_num) = Regex::new(r"(?:\s|^)(\d{1,4}(?:\.\d+)?)(?:v\d+)?(?:\s*$|\s+(?:\[|\())") {
        if let Some(caps) = re_num.captures(raw) {
            if let Some(m) = caps.get(1) {
                if let Ok(val) = m.as_str().parse::<f32>() {
                    // Filter out 4-digit years like 1999, 2024 unless clearly an episode
                    if !(val >= 1950.0 && val <= 2040.0) {
                        return Some(val);
                    }
                }
            }
        }
    }

    None
}

/// Natural comparator comparing alphanumeric strings with numerical awareness
/// (e.g., "Ep 2" sorts before "Ep 10").
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut chars_a = a.chars().peekable();
    let mut chars_b = b.chars().peekable();

    while let (Some(&ca), Some(&cb)) = (chars_a.peek(), chars_b.peek()) {
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let mut num_str_a = String::new();
            while let Some(&c) = chars_a.peek() {
                if c.is_ascii_digit() {
                    num_str_a.push(c);
                    chars_a.next();
                } else {
                    break;
                }
            }

            let mut num_str_b = String::new();
            while let Some(&c) = chars_b.peek() {
                if c.is_ascii_digit() {
                    num_str_b.push(c);
                    chars_b.next();
                } else {
                    break;
                }
            }

            let num_a = num_str_a.parse::<u64>().unwrap_or(0);
            let num_b = num_str_b.parse::<u64>().unwrap_or(0);

            match num_a.cmp(&num_b) {
                std::cmp::Ordering::Equal => {
                    if num_str_a.len() != num_str_b.len() {
                        return num_str_a.len().cmp(&num_str_b.len());
                    }
                }
                ord => return ord,
            }
        } else {
            let ca_lower = ca.to_ascii_lowercase();
            let cb_lower = cb.to_ascii_lowercase();
            match ca_lower.cmp(&cb_lower) {
                std::cmp::Ordering::Equal => {
                    chars_a.next();
                    chars_b.next();
                }
                ord => return ord,
            }
        }
    }

    match (chars_a.peek(), chars_b.peek()) {
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => std::cmp::Ordering::Equal,
        _ => unreachable!(),
    }
}

/// Scans any folder for video files (.mkv, .mp4, .webm, .avi).
/// Cleans display titles, parses episode numbers, and naturally sorts the episode list.
pub fn scan_folder_episodes(folder: &Path) -> Vec<EpisodeEntry> {
    let read_dir = match std::fs::read_dir(folder) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    let mut episodes = Vec::new();

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_file() {
            let is_video = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| {
                    let ext = ext.to_ascii_lowercase();
                    ext == "mkv" || ext == "mp4" || ext == "webm" || ext == "avi" || ext == "mov" || ext == "m4v"
                })
                .unwrap_or(false);

            if is_video {
                let raw_filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                let clean_title = clean_nyaa_title(&raw_filename);
                let episode_number = parse_episode_number(&clean_title)
                    .or_else(|| parse_episode_number(&raw_filename));

                episodes.push(EpisodeEntry {
                    path,
                    raw_filename,
                    clean_title,
                    episode_number,
                    torrent_info: None,
                });
            }
        }
    }

    // Sort episodes naturally: first by episode_number if both present, then natural string order
    episodes.sort_by(|a, b| {
        match (a.episode_number, b.episode_number) {
            (Some(na), Some(nb)) => {
                na.partial_cmp(&nb)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| natural_cmp(&a.clean_title, &b.clean_title))
            }
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => natural_cmp(&a.clean_title, &b.clean_title),
        }
    });

    episodes
}

/// Scans the sibling directory containing `current_mkv` for all video files.
/// Cleans display titles, parses episode numbers, and naturally sorts the episode list.
pub fn scan_sibling_episodes(current_mkv: &Path) -> Vec<EpisodeEntry> {
    current_mkv.parent().map(scan_folder_episodes).unwrap_or_default()
}
