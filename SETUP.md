# Vesper Setup & Build Guide (Windows MSVC)

This document provides setup instructions for building and running Vesper on Windows using the MSVC toolchain (`x86_64-pc-windows-msvc`).

---

## 1. Prerequisites

1. **Rust & Cargo**:
   - Install Rust with the MSVC toolchain:
     ```bash
     rustup default stable-x86_64-pc-windows-msvc
     ```
2. **Visual Studio C++ Build Tools**:
   - Required for the MSVC linker (`link.exe`) and library archiver (`lib.exe`).
   - If not already installed, download [Visual Studio Community or Build Tools](https://visualstudio.microsoft.com/downloads/) with the "Desktop development with C++" workload.

---

## 2. Obtaining `libmpv-2.dll` and Headers

Vesper embeds `mpv` via `libmpv2`. At compile-time, it requires an import library (`mpv.lib`), and at run-time, it requires the dynamic library (`libmpv-2.dll`).

1. Download the latest Windows 64-bit developer archive of `libmpv`:
   - [mpv Windows builds by shinchiro (SourceForge)](https://sourceforge.net/projects/mpv-player-windows/files/libmpv/)
   - Choose a package named like `mpv-dev-x86_64-*.7z`.
2. Extract the archive using 7-Zip.
3. Locate `libmpv-2.dll`.

---

## 3. Placement of Runtime DLLs

Place `libmpv-2.dll` in either:
- The same directory as the built executable (`C:\VD\Vesper\target\debug\` or `C:\VD\Vesper\target\release\`).
- Or any directory registered in your Windows `%PATH%` environment variable.

If `libmpv-2.dll` is missing at runtime, the application will terminate with Windows error code `0xc0000135` (STATUS_DLL_NOT_FOUND).

---

## 4. Import Library Generation (`mpv.lib`)

The `libs` directory in `C:\VD\Vesper\libs` already includes:
- `mpv.def`: Module definition file exporting all required libmpv C API functions.
- `mpv.lib`: Pre-generated 64-bit MSVC import library.

### Re-generating `mpv.lib` (Optional / Updates)

If you update `libmpv-2.dll` and wish to regenerate `mpv.lib`:

#### Option A: Using MSVC `lib.exe`
Open a **x64 Native Tools Command Prompt for VS** and run:
```cmd
cd C:\VD\Vesper\libs
lib /def:mpv.def /name:libmpv-2.dll /out:mpv.lib /machine:x64
```

#### Option B: Using LLVM `llvm-lib` / `rust-lld`
If using an LLVM toolchain:
```cmd
cd C:\VD\Vesper\libs
llvm-lib /def:mpv.def /name:libmpv-2.dll /out:mpv.lib /machine:x64
```

---

## 5. Build Script Integration (`build.rs`)

Vesper's `build.rs` automatically instructs Cargo and the MSVC linker where to find `mpv.lib`:

```rust
println!("cargo:rustc-link-search=native={}", libs_dir.display());
```

It also watches `build.rs` and the `libs` folder for any changes:
```rust
println!("cargo:rerun-if-changed=build.rs");
println!("cargo:rerun-if-changed=libs");
println!("cargo:rerun-if-changed=libs/mpv.lib");
println!("cargo:rerun-if-changed=libs/mpv.def");
```

---

## 6. Building and Running

Compile the project:
```bash
cargo build
```

Run in debug mode (ensure `libmpv-2.dll` is placed in `target\debug\`):
```bash
cargo run
```

Build optimized release binary:
```bash
cargo build --release
```
