# Vesper

Vesper is a lightweight, native Windows video player and subtitle extractor written in Rust and powered by `libmpv` (Direct3D 11).

Designed specifically for anime and media enthusiasts, Vesper combines zero-overhead hardware-accelerated playback with pure-Rust subtitle demuxing, multi-track audio selection, instant subtitle cue extraction, and automated keyless external subtitle discovery.

---

## Features

- **Hardware-Accelerated Video Engine:** Powered by `libmpv` with Direct3D 11 embedding (`vo=gpu-next`, `hwdec=auto-safe`, `keep-open=yes`) and embedded MKV font rendering. Supports all major video formats (`.mkv`, `.mp4`, `.avi`, `.mov`, `.webm`, `.flv`).
- **Keyless External Subtitle Discovery:** Automatic background fetching and attachment of subtitles with zero API keys required:
  - **AnimeTosho:** Keyless lookup by torrent `info_hash` or title/episode number with decompression (`.xz`, `.gz`) into native `.ass`/`.srt`.
  - **Cinemeta + OpenSubtitles v3:** Keyless IMDb resolution and subtitle lookup for movies and TV series.
  - **Optional Overrides:** Seamlessly supports Jimaku (`jimaku.cc`) and OpenSubtitles REST API keys via in-app settings.
  - **Manual Search & Attach:** Browse and attach online subtitles directly from the Subtitles sidebar drawer.
- **Multi-Track Metadata Handling:** Clean detection and selection of multi-track audio and subtitle streams (`ASS`, `SRT`, `PGS`, `OPUS`, `FLAC`, `AAC`, and surround sound channels).
- **Windows Integration:**
  - **System Media Transport Controls (SMTC):** Hardware media key support (Play/Pause, Next, Previous, Stop) and Windows 10/11 taskbar thumbnail transport controls.
  - **Native File Associations:** One-click registration for video file extensions in Windows Registry (`HKEY_CURRENT_USER\Software\Classes`) with shell context menu integration.
- **BitTorrent Streaming & Library Management:**
  - **Live Torrent Streaming:** Embedded `librqbit` client for streaming media directly from magnet links, `.torrent` files, and Nyaa RSS search results.
  - **Batch Season Pack Support:** Multi-file batch picker modal with selective episode downloading and instant single-episode streaming.
  - **Automatic Next-Episode Pre-Buffering:** Seamlessly starts pre-buffering the next batch episode when playback passes 70% or the current episode finishes downloading.
  - **Persistent Downloads & Resume:** Completed torrents and playback progress survive restarts; already-downloaded files are detected and played locally without re-downloading.
  - **Safe Disk Deletion:** Two-click safety confirmation (`Delete` -> `Confirm?` within 3 seconds) that safely unloads video and deletes files from disk.
  - **Custom & Auto-Synced Trackers:** qBittorrent-style live tracker list synchronization and custom tracker editor.
- **Collapsible Sidebar:**
  - **Live Subtitle Stream:** Real-time timestamped subtitle cues with click-to-seek and one-click clipboard copying.
  - **Track Extraction:** Extract embedded `.ass`, `.srt`, or `.sup` (PGS) subtitle streams directly to disk without external tools.
  - **Folder Playlist:** Automatic sibling episode discovery and playlist management.
  - **Network & Torrent Streaming:** Built-in stream and download client for RSS anime feeds, BitTorrent/Magnet URIs, and direct media links.
- **Auto-Hiding Playback Bar:** Floating borderless Win32 overlay that docks above the viewport and automatically fades during playback after 2.5 seconds of mouse inactivity, preserving video aspect ratios with zero viewport resizing.
- **Chapter Navigation & Drag-and-Drop:** Timeline tick markers for embedded MKV chapters, plus drag-and-drop support for local media files.

---

## Keyboard & Media Shortcuts

| Shortcut | Action |
| :--- | :--- |
| `Space` / `MediaPlayPause` | Toggle Play / Pause |
| `Left` / `Right` | Seek backward / forward 5 seconds |
| `MediaNext` | Play next episode |
| `MediaPrev` | Play previous episode / seek to start |
| `MediaStop` | Stop playback |
| `S` | Cycle subtitle tracks |
| `A` | Cycle audio tracks |
| `D` | Copy active subtitle cue to clipboard |
| `Tab` | Toggle right collapsible sidebar |
| `F` | Toggle borderless fullscreen |
| `Ctrl + O` | Open file dialog |
| `Ctrl + Shift + O` | Open folder playlist dialog |

---

## Quick Start (Releases)

1. Download the latest `vesper-v<version>-windows-x64.zip` (or standalone `vesper.exe`) from [GitHub Releases](https://github.com/Caly-sys/vesper/releases).
2. Extract the archive contents into any local folder.
3. Launch `vesper.exe`.

> Note: `libmpv-2.dll` must remain in the same directory as `vesper.exe` (or located in your system `%PATH%`). The release archive includes a pre-packaged 64-bit runtime DLL for immediate out-of-the-box usage.

---

## Building from Source

### Prerequisites

- **Rust toolchain:** `stable-x86_64-pc-windows-msvc`
- **Visual Studio C++ Build Tools:** Desktop development with C++ (`link.exe`, `lib.exe`)
- **libmpv Windows x64:** `libmpv-2.dll` and `mpv.lib` (see [SETUP.md](SETUP.md) for detailed build instructions)

### Build Commands

```powershell
# Clone the repository
git clone https://github.com/Caly-sys/vesper.git
cd vesper

# Build optimized release binary with BitTorrent streaming feature
cargo build --release --features torrent
```

The compiled binary will be placed at `target/release/vesper.exe`. Ensure `libmpv-2.dll` is present in `target/release/` before executing.

---

## Windows SmartScreen & Defender Notice

Because release binaries are not code-signed with an expensive commercial EV certificate, Windows SmartScreen or Defender heuristics may display an "Unrecognized app" prompt upon initial launch. If preferred, you can verify the code and build directly from source using the steps above.

---

## License

This project is licensed under the MIT License. Embedded `libmpv` components are distributed under LGPLv2.1 / GPLv3.
