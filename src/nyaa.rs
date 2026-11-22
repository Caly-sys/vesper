use quick_xml::events::Event;
use quick_xml::reader::Reader;
use tracing::{debug, info};

use crate::types::{NyaaCategory, NyaaFilter, NyaaItem};

/// Cleans anime release titles by stripping release group brackets, file extensions,
/// resolution tags, audio/video codec tags, CRC checksums, and normalization.
pub fn clean_anime_title(raw: &str) -> String {
    let mut title = raw.trim();

    // 1. Strip file extension: .mkv, .mp4, .avi, .webm, .ts, .m4v (case-insensitive)
    for ext in &[".mkv", ".mp4", ".avi", ".webm", ".ts", ".m4v"] {
        if let Some(stripped) = title.strip_suffix(ext) {
            title = stripped.trim();
            break;
        }
        if title.len() >= ext.len() && title[title.len() - ext.len()..].eq_ignore_ascii_case(ext) {
            title = title[..title.len() - ext.len()].trim();
            break;
        }
    }

    // 2. Strip leading release group tag: ^(\[[^\]]+\]|\([^\)]+\))\s*
    // Only strip if there is title content following the group tag
    if (title.starts_with('[') && title.contains(']'))
        || (title.starts_with('(') && title.contains(')'))
    {
        let close_char = if title.starts_with('[') { ']' } else { ')' };
        if let Some(close_pos) = title.find(close_char) {
            let after = title[close_pos + 1..].trim();
            if !after.is_empty() {
                title = after;
            }
        }
    }

    // 3. Repeatedly strip trailing bracket/parenthesis metadata tags from the end
    // e.g. [1080p], (1080p), [93392DAE], [Multiple Subtitle], [Dual-Audio], etc.
    let mut title_owned = title.to_string();
    loop {
        let trimmed = title_owned.trim_end();
        if trimmed.ends_with(']') {
            if let Some(open_pos) = trimmed.rfind('[') {
                let candidate = trimmed[..open_pos].trim_end();
                if !candidate.is_empty() {
                    title_owned = candidate.to_string();
                    continue;
                }
            }
        } else if trimmed.ends_with(')') {
            if let Some(open_pos) = trimmed.rfind('(') {
                let candidate = trimmed[..open_pos].trim_end();
                if !candidate.is_empty() {
                    title_owned = candidate.to_string();
                    continue;
                }
            }
        }
        break;
    }

    // 4. If title uses underscores as word separators without spaces, convert to spaces
    if title_owned.contains('_') && !title_owned.contains(' ') {
        title_owned = title_owned.replace('_', " ");
    }

    // 5. Clean up multiple spaces and dangling hyphens
    let parts: Vec<&str> = title_owned.split_whitespace().collect();
    let cleaned = parts.join(" ");
    let cleaned = cleaned.trim_end_matches('-').trim().to_string();

    if cleaned.is_empty() {
        raw.trim().to_string()
    } else {
        cleaned
    }
}

/// Helper to parse fields within an RSS `<item>`
fn append_item_field(
    tag: &str,
    text: &str,
    title: &mut String,
    link: &mut String,
    info_hash: &mut Option<String>,
    seeders: &mut u32,
    leechers: &mut u32,
    size: &mut String,
    pub_date: &mut String,
) {
    match tag {
        "title" => title.push_str(text),
        "link" => link.push_str(text),
        "nyaa:infohash" | "infohash" => *info_hash = Some(text.trim().to_string()),
        "nyaa:seeders" | "seeders" => {
            if let Ok(v) = text.trim().parse::<u32>() {
                *seeders = v;
            }
        }
        "nyaa:leechers" | "leechers" => {
            if let Ok(v) = text.trim().parse::<u32>() {
                *leechers = v;
            }
        }
        "nyaa:size" | "size" => size.push_str(text),
        "pubdate" => pub_date.push_str(text),
        _ => {}
    }
}

/// Extracts a 40-character hex or 32-character base32 hash from a magnet link if present
fn extract_hash_from_magnet(link: &str) -> Option<String> {
    if let Some(pos) = link.find("xt=urn:btih:") {
        let after = &link[pos + "xt=urn:btih:".len()..];
        let hash: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if hash.len() == 40 || hash.len() == 32 {
            return Some(hash);
        }
    }
    None
}

/// Parses raw Nyaa RSS XML feed content into a list of `NyaaItem`s
pub fn parse_nyaa_rss(xml: &str) -> Result<Vec<NyaaItem>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut items = Vec::new();
    let mut in_item = false;
    let mut current_tag = String::new();

    let mut title = String::new();
    let mut link = String::new();
    let mut info_hash = None;
    let mut seeders = 0u32;
    let mut leechers = 0u32;
    let mut size = String::new();
    let mut pub_date = String::new();

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag_raw = e.name().into_inner();
                let tag_name = String::from_utf8_lossy(tag_raw).to_ascii_lowercase();
                if tag_name == "item" {
                    in_item = true;
                    title.clear();
                    link.clear();
                    info_hash = None;
                    seeders = 0;
                    leechers = 0;
                    size.clear();
                    pub_date.clear();
                } else if in_item {
                    current_tag = tag_name;
                }
            }
            Ok(Event::End(ref e)) => {
                let tag_raw = e.name().into_inner();
                let tag_name = String::from_utf8_lossy(tag_raw).to_ascii_lowercase();
                if tag_name == "item" {
                    if in_item {
                        in_item = false;
                        let trimmed_title = title.trim().to_string();
                        let clean = clean_anime_title(&trimmed_title);

                        // If info_hash was not provided in <nyaa:infoHash>, attempt to extract from link
                        let resolved_info_hash = info_hash.take().or_else(|| {
                            extract_hash_from_magnet(&link)
                        });

                        items.push(NyaaItem {
                            clean_title: clean,
                            title: trimmed_title,
                            link: link.trim().to_string(),
                            info_hash: resolved_info_hash,
                            seeders,
                            leechers,
                            size: size.trim().to_string(),
                            date: pub_date.trim().to_string(),
                        });
                    }
                } else if in_item {
                    current_tag.clear();
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_item && !current_tag.is_empty() {
                    let text = e
                        .unescape()
                        .unwrap_or_else(|_| String::from_utf8_lossy(e.as_ref()))
                        .to_string();
                    append_item_field(
                        &current_tag,
                        &text,
                        &mut title,
                        &mut link,
                        &mut info_hash,
                        &mut seeders,
                        &mut leechers,
                        &mut size,
                        &mut pub_date,
                    );
                }
            }
            Ok(Event::CData(ref e)) => {
                if in_item && !current_tag.is_empty() {
                    let text = String::from_utf8_lossy(e.as_ref()).to_string();
                    append_item_field(
                        &current_tag,
                        &text,
                        &mut title,
                        &mut link,
                        &mut info_hash,
                        &mut seeders,
                        &mut leechers,
                        &mut size,
                        &mut pub_date,
                    );
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(format!(
                    "Error parsing Nyaa XML at position {}: {e}",
                    reader.error_position()
                ));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(items)
}

/// Asynchronous Nyaa RSS Search Client
///
/// Fetches the RSS XML feed from `{base_url}/?page=rss&c={category.as_code()}&f={filter as u8}&q={encoded_query}`
/// and parses it into strongly-typed `NyaaItem` structures.
pub async fn search_nyaa(
    base_url: &str,
    query: &str,
    category: NyaaCategory,
    filter: NyaaFilter,
) -> Result<Vec<NyaaItem>, String> {
    let base = base_url.trim_end_matches('/');
    let encoded_query = urlencoding::encode(query);
    let url = format!(
        "{}/?page=rss&c={}&f={}&q={}",
        base,
        category.as_code(),
        filter as u8,
        encoded_query
    );

    debug!("Fetching Nyaa RSS feed: {url}");

    let client = reqwest::Client::builder()
        .user_agent("Vesper/0.1.0")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to create reqwest HTTP client: {e}"))?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error fetching Nyaa RSS feed from {url}: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "Nyaa RSS feed HTTP request failed with status code {status}"
        ));
    }

    let xml_bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read Nyaa RSS response bytes: {e}"))?;

    let xml_text = String::from_utf8_lossy(&xml_bytes);
    let items = parse_nyaa_rss(&xml_text)?;

    info!(
        "Nyaa search for '{}' completed: found {} items",
        query,
        items.len()
    );
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_anime_title_various_formats() {
        assert_eq!(
            clean_anime_title("[SubsPlease] Sousou no Frieren - 28 (1080p) [93392DAE].mkv"),
            "Sousou no Frieren - 28"
        );
        assert_eq!(
            clean_anime_title("[Erai-raws] Jujutsu Kaisen 2nd Season - 23 [1080p][Multiple Subtitle] [ENG, JPN...].mkv"),
            "Jujutsu Kaisen 2nd Season - 23"
        );
        assert_eq!(
            clean_anime_title("[ASW] Dungeon Meshi - 12 [1080p x265 10Bit] [E-AC3]"),
            "Dungeon Meshi - 12"
        );
        assert_eq!(
            clean_anime_title("[Judas] Bleach - S01E01 [1080p][HEVC x265 10bit][Dual-Audio][Multi-Subs]"),
            "Bleach - S01E01"
        );
        assert_eq!(
            clean_anime_title("Sousou_no_Frieren_-_01_(1080p).mkv"),
            "Sousou no Frieren - 01"
        );
        assert_eq!(
            clean_anime_title("Solo Leveling - 01 (1080p).mkv"),
            "Solo Leveling - 01"
        );
    }

    #[test]
    fn test_parse_nyaa_rss_xml() {
        let sample_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom" xmlns:nyaa="https://nyaa.si/xmlns/nyaa">
    <channel>
        <title>Nyaa - Anime - English-translated</title>
        <description>RSS Feed</description>
        <link>https://nyaa.si/</link>
        <item>
            <title>[SubsPlease] Frieren - 28 (1080p) [93392DAE].mkv</title>
            <link>https://nyaa.si/download/1792942.torrent</link>
            <guid isPermaLink="true">https://nyaa.si/view/1792942</guid>
            <pubDate>Fri, 22 Mar 2024 15:00:00 -0000</pubDate>
            <nyaa:seeders>1250</nyaa:seeders>
            <nyaa:leechers>120</nyaa:leechers>
            <nyaa:downloads>5432</nyaa:downloads>
            <nyaa:infoHash>93392dae1234567890abcdef1234567890abcdef</nyaa:infoHash>
            <nyaa:categoryId>1_2</nyaa:categoryId>
            <nyaa:category>Anime - English-translated</nyaa:category>
            <nyaa:size>1.4 GiB</nyaa:size>
            <nyaa:comments>0</nyaa:comments>
            <nyaa:trusted>No</nyaa:trusted>
            <nyaa:remake>No</nyaa:remake>
            <description><![CDATA[ Torrent description here ]]></description>
        </item>
    </channel>
</rss>"#;

        let items = parse_nyaa_rss(sample_xml).expect("Parsing sample Nyaa XML should succeed");
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.title, "[SubsPlease] Frieren - 28 (1080p) [93392DAE].mkv");
        assert_eq!(item.clean_title, "Frieren - 28");
        assert_eq!(item.link, "https://nyaa.si/download/1792942.torrent");
        assert_eq!(
            item.info_hash,
            Some("93392dae1234567890abcdef1234567890abcdef".to_string())
        );
        assert_eq!(item.seeders, 1250);
        assert_eq!(item.leechers, 120);
        assert_eq!(item.size, "1.4 GiB");
        assert_eq!(item.date, "Fri, 22 Mar 2024 15:00:00 -0000");
    }
}
