# Vesper

Vesper is a lightweight, native Windows video player and subtitle extractor written in Rust and powered by `libmpv` (Direct3D 11).

Designed specifically for anime and media enthusiasts, Vesper combines zero-overhead hardware-accelerated playback with pure-Rust subtitle demuxing, multi-track audio selection, and instant subtitle cue extraction.

---

## Features

- **Hardware-Accelerated Video Engine:** Powered by `libmpv` with Direct3D 11 embedding (`vo=gpu-next`, `hwdec=auto-safe`, `keep-open=yes`) and embedded MKV font rendering.
- **Multi-Track Metadata Handling:** Clean detection and selection of multi-track audio and subtitle streams (including `ASS`, `SRT`, `PGS`, `OPUS`, `FLAC`, `AAC`, and surround sound channels).
- **Collapsible Sidebar:**
  - **Live Subtitle Stream:** Real-time timestamped subtitle cues with click-to-seek and one-click clipboard copying.
  - **Track Extraction:** Extract embedded `.ass`, `.srt`, or `.sup` (PGS) subtitle streams directly to disk without external tools.
  - **Folder Playlist:** Automatic sibling episode discovery and playlist management.
  - **Direct URL Streaming:** Built-in stream client for HTTP, HTTPS, and HLS media feeds.
- **Auto-Hiding Playback Bar:** Floating borderless Win32 overlay that docks above the viewport and automatically fades during playback after 2.5 seconds of mouse inactivity, preserving video aspect ratios with zero viewport resizing.
- **Chapter Navigation & Drag-and-Drop:** Timeline tick markers for embedded MKV chapters, plus drag-and-drop support for local media files.

---

## Keyboard Shortcuts

| Shortcut | Action |
| :--- | :--- |
| `Space` | Toggle Play / Pause |
| `Left` / `Right` | Seek backward / forward 5 seconds |
| `S` | Cycle subtitle tracks |
| `A` | Cycle audio tracks |
| `D` | Copy active subtitle cue to clipboard |
| `Tab` | Toggle right collapsible sidebar |
| `F` | Toggle borderless fullscreen |
| `Ctrl + O` | Open file dialog |
| `Ctrl + Shift + O` | Open folder playlist dialog |

---

## Quick Start (Releases)

1. Download the latest `vesper-v<version>-windows-x64.zip` from [GitHub Releases](https://github.com/kelvinsyahputra25-cyber/vesper/releases).
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
git clone https://github.com/kelvinsyahputra25-cyber/vesper.git
cd vesper

# Build optimized release binary
cargo build --release
```

The compiled binary will be placed at `target/release/vesper.exe`. Ensure `libmpv-2.dll` is present in `target/release/` before executing.

---

## Windows SmartScreen & Defender Notice

Because release binaries are not code-signed with an expensive commercial EV certificate, Windows SmartScreen or Defender heuristics may display an "Unrecognized app" prompt upon initial launch. If preferred, you can verify the code and build directly from source using the steps above.

---

## License

This project is licensed under the MIT License. Embedded `libmpv` components are distributed under LGPLv2.1 / GPLv3.
