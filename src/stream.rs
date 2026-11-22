#![allow(dead_code)]

//! Magnet URI construction and URL stream helpers for Vesper

use std::path::Path;

/// Standard high-availability public anime trackers
pub const DEFAULT_TRACKERS: &[&str] = &[
    "http://nyaa.tracker.wf:7777/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://exodus.desync.com:6969/announce",
];
pub const STANDARD_TRACKERS: &[&str] = DEFAULT_TRACKERS;

/// Extracts all existing tracker URLs from a magnet URI (from &tr= parameters)
pub fn extract_magnet_trackers(magnet: &str) -> Vec<String> {
    let mut trackers = Vec::new();
    let query = match magnet.find('?') {
        Some(idx) => &magnet[idx + 1..],
        None => return trackers,
    };
    for part in query.split('&') {
        if let Some(encoded) = part.strip_prefix("tr=") {
            if let Ok(decoded) = urlencoding::decode(encoded) {
                let trimmed = decoded.trim();
                if !trimmed.is_empty() {
                    trackers.push(trimmed.to_string());
                }
            }
        }
    }
    trackers
}

/// Appends missing tracker URLs to a magnet URI if not already present
pub fn append_trackers_to_magnet(magnet: &str, trackers: &[String]) -> String {
    let existing = extract_magnet_trackers(magnet);
    let existing_set: std::collections::HashSet<String> = existing
        .iter()
        .map(|t| t.trim().to_ascii_lowercase())
        .collect();

    let mut result = magnet.to_string();
    for tr in trackers {
        let trimmed = tr.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if !existing_set.contains(&lower) {
            result.push_str("&tr=");
            result.push_str(&urlencoding::encode(trimmed));
        }
    }
    result
}

/// Combines trackers in priority order while preserving uniqueness (case-insensitive / trimmed deduplication):
/// 1. All original trackers already inside the .torrent file or magnet:? URI (always preserved first)
/// 2. Built-in Nyaa & fallback trackers (http://nyaa.tracker.wf:7777/announce, etc.)
/// 3. config.custom_trackers (user-added trackers)
/// 4. config.cached_remote_trackers (auto-fetched trackers_best.txt)
pub fn merge_trackers(existing: &[String], config: &crate::config::Config) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    let mut add = |s: &str| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return;
        }
        let lower = trimmed.to_ascii_lowercase();
        if (lower.starts_with("udp://")
            || lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("wss://"))
            && seen.insert(lower)
        {
            result.push(trimmed.to_string());
        }
    };

    // 1. Existing trackers already inside .torrent or magnet (always preserved first)
    for tr in existing {
        add(tr);
    }
    // 2. Built-in Nyaa & standard fallback trackers
    for tr in DEFAULT_TRACKERS {
        add(tr);
    }
    // 3. User custom trackers
    for tr in &config.custom_trackers {
        add(tr);
    }
    // 4. Auto-fetched remote trackers
    for tr in &config.cached_remote_trackers {
        add(tr);
    }

    result
}

/// Fetches a remote tracker list (e.g. ngosang/trackerslist trackers_best.txt)
/// and parses it line-by-line into valid tracker URLs.
pub async fn fetch_remote_trackers(url: &str) -> Result<Vec<String>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Tracker sync request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Tracker sync failed with HTTP {}", resp.status()));
    }

    let text = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    let mut seen = std::collections::HashSet::new();
    let mut list = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if (lower.starts_with("udp://")
            || lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("wss://"))
            && seen.insert(lower)
        {
            list.push(trimmed.to_string());
        }
    }

    Ok(list)
}

/// Constructs a valid magnet URI from a 40-character hex or 32-character base32 infohash
/// and title, embedding standard trackers alongside the display name.
pub fn build_magnet_uri(info_hash: &str, title: &str) -> String {
    let hash = info_hash.trim();
    let dn = urlencoding::encode(title.trim());
    let mut uri = format!("magnet:?xt=urn:btih:{}&dn={}", hash, dn);
    for tr in DEFAULT_TRACKERS {
        uri.push_str("&tr=");
        uri.push_str(&urlencoding::encode(tr));
    }
    uri
}

/// Returns true if the string is a valid magnet URI
pub fn is_magnet(s: &str) -> bool {
    s.trim().starts_with("magnet:?")
}

/// Returns true if the string is a remote HTTP/HTTPS .torrent file link
pub fn is_torrent_url(s: &str) -> bool {
    let trimmed = s.trim();
    (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        && (trimmed.contains(".torrent") || trimmed.contains("/download/"))
}

/// Returns true if the string is a direct video/media stream URL (e.g. mp4, mkv, m3u8)
pub fn is_direct_stream_url(s: &str) -> bool {
    let trimmed = s.trim();
    (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        && !is_torrent_url(trimmed)
        && !is_magnet(trimmed)
}

/// Returns true if the file path has a .torrent extension
pub fn is_torrent_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("torrent"))
        .unwrap_or(false)
}

/// Launch an external URL, magnet link, or file in the default Windows application
pub fn open_external(target: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/c", "start", "", target])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| format!("Failed to launch external command: {e}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map_err(|e| format!("Failed to open target: {e}"))?;
        Ok(())
    }
}

/// Open Windows Explorer focused on a specific file or folder.
/// If the file exists, it is selected using `/select,"path"`.
/// If the file does not exist yet, its parent or the fallback directory is opened directly.
pub fn open_folder(path: &Path, fallback_dir: Option<&Path>) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let clean_str = |p: &Path| -> String {
            let s = p.to_string_lossy();
            let stripped = s.strip_prefix(r"\\?\").unwrap_or(&s);
            stripped.replace('/', "\\")
        };

        let mut cmd = std::process::Command::new("explorer.exe");
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

        if path.exists() {
            let clean = clean_str(path);
            if path.is_file() {
                cmd.raw_arg(format!("/select,\"{}\"", clean));
            } else {
                cmd.raw_arg(format!("\"{}\"", clean));
            }
        } else {
            let target_dir = path
                .parent()
                .filter(|p| p.exists())
                .or_else(|| fallback_dir.filter(|p| p.exists()));

            let dir_to_open = match target_dir {
                Some(dir) => dir.to_path_buf(),
                None => {
                    let fallback = fallback_dir.unwrap_or(path);
                    let _ = std::fs::create_dir_all(fallback);
                    fallback.to_path_buf()
                }
            };
            let clean = clean_str(&dir_to_open);
            cmd.raw_arg(format!("\"{}\"", clean));
        }

        cmd.spawn()
            .map_err(|e| format!("Failed to open Explorer: {e}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let target = if path.exists() {
            path
        } else if let Some(fb) = fallback_dir {
            fb
        } else {
            path
        };
        open_external(&target.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_magnet_uri() {
        let hash = "93392dae1234567890abcdef1234567890abcdef";
        let title = "[SubsPlease] Sousou no Frieren - 28 (1080p)";
        let magnet = build_magnet_uri(hash, title);
        assert!(magnet.starts_with("magnet:?xt=urn:btih:93392dae1234567890abcdef1234567890abcdef"));
        assert!(magnet.contains("&dn="));
        assert!(magnet.contains("nyaa.tracker.wf"));
        assert!(magnet.contains("open.stealth.si"));
    }

    #[test]
    fn test_url_classification() {
        assert!(is_magnet("magnet:?xt=urn:btih:12345"));
        assert!(!is_magnet("https://nyaa.si/view/12345"));

        assert!(is_torrent_url("https://nyaa.si/download/12345.torrent"));
        assert!(!is_torrent_url("https://example.com/video.mp4"));

        assert!(is_direct_stream_url("https://example.com/video.mp4"));
        assert!(is_direct_stream_url("http://stream.example.com/live.m3u8"));
        assert!(!is_direct_stream_url("magnet:?xt=urn:btih:123"));

        assert!(is_torrent_file(Path::new("test.torrent")));
        assert!(is_torrent_file(Path::new("C:/Downloads/anime.TORRENT")));
        assert!(!is_torrent_file(Path::new("test.mkv")));
    }

    #[test]
    fn test_merge_trackers_preserves_nyaa_and_deduplicates() {
        let mut config = crate::config::Config::default();
        config.custom_trackers = vec![
            "udp://tracker.custom.org:1337/announce".to_string(),
            "HTTP://NYAA.TRACKER.WF:7777/ANNOUNCE".to_string(), // duplicate of standard with different casing
            "not-a-valid-tracker-url".to_string(),
        ];
        config.cached_remote_trackers = vec![
            "udp://tracker.opentrackr.org:1337/announce".to_string(), // duplicate of standard
            "wss://tracker.webtorrent.dev".to_string(),
        ];

        let existing = vec![
            "udp://tracker.original.io:6969/announce".to_string(),
        ];

        let merged = merge_trackers(&existing, &config);

        // 1. Existing tracker preserved first
        assert_eq!(merged[0], "udp://tracker.original.io:6969/announce");
        // 2. Built-in Nyaa tracker preserved
        assert!(merged.iter().any(|t| t == "http://nyaa.tracker.wf:7777/announce"));
        // 3. User custom tracker included
        assert!(merged.iter().any(|t| t == "udp://tracker.custom.org:1337/announce"));
        // 4. Remote cached tracker included
        assert!(merged.iter().any(|t| t == "wss://tracker.webtorrent.dev"));
        // 5. Invalid URL filtered
        assert!(!merged.iter().any(|t| t.contains("not-a-valid")));

        // 6. No duplicates (case-insensitive check)
        let mut lower_set = std::collections::HashSet::new();
        for tr in &merged {
            assert!(lower_set.insert(tr.to_ascii_lowercase()), "Duplicate tracker found: {}", tr);
        }
    }

    #[test]
    fn test_append_trackers_to_magnet() {
        let base = "magnet:?xt=urn:btih:1234567890123456789012345678901234567890&dn=Test";
        let trackers = vec![
            "http://nyaa.tracker.wf:7777/announce".to_string(),
            "udp://tracker.opentrackr.org:1337/announce".to_string(),
        ];
        let enriched = append_trackers_to_magnet(base, &trackers);
        assert!(enriched.contains("&tr=http%3A%2F%2Fnyaa.tracker.wf%3A7777%2Fannounce"));
        assert!(enriched.contains("&tr=udp%3A%2F%2Ftracker.opentrackr.org%3A1337%2Fannounce"));

        // Appending again does not duplicate
        let double_enriched = append_trackers_to_magnet(&enriched, &trackers);
        assert_eq!(enriched, double_enriched);
    }
}
