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

/// Open Windows Explorer focused on a specific file or folder
pub fn open_folder(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("explorer");
        if path.is_file() {
            cmd.arg(format!("/select,\"{}\"", path.display()));
        } else {
            cmd.arg(path.as_os_str());
        }
        cmd.creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Failed to open Explorer: {e}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        open_external(&path.to_string_lossy())
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
}
