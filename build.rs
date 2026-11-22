use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let libs_dir = PathBuf::from(&manifest_dir).join("libs");

    // Instruct rustc to link against mpv.lib in the libs directory
    println!("cargo:rustc-link-search=native={}", libs_dir.display());

    // Configure rebuild triggers
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=libs");
    println!("cargo:rerun-if-changed=libs/mpv.lib");
    println!("cargo:rerun-if-changed=libs/mpv.def");
    println!("cargo:rerun-if-changed=assets/vesper.ico");
    println!("cargo:rerun-if-changed=icon.ico");

    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set("ProductName", "Vesper");
        res.set("FileDescription", "Vesper Video Player");
        res.set("OriginalFilename", "vesper.exe");
        res.set("InternalName", "vesper");
        res.set("CompanyName", "Vesper Developers");
        res.set("LegalCopyright", "Copyright (c) 2026 Vesper Developers");

        let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".to_string());
        res.set("FileVersion", &version);
        res.set("ProductVersion", &version);

        let icon_path = if Path::new("assets/vesper.ico").exists() {
            Some("assets/vesper.ico")
        } else if Path::new("icon.ico").exists() {
            Some("icon.ico")
        } else {
            None
        };

        if let Some(icon) = icon_path {
            res.set_icon(icon);
        }

        // Standard Windows application manifest with asInvoker and OS compatibility GUIDs
        let app_manifest = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <!-- Windows 10 and 11 -->
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
      <!-- Windows 8.1 -->
      <supportedOS Id="{1f676c76-80e1-4239-95bb-83d0f6d0da78}"/>
      <!-- Windows 8 -->
      <supportedOS Id="{4a2f28e3-53b9-4441-ba9c-d69d4a4a6e38}"/>
      <!-- Windows 7 -->
      <supportedOS Id="{35138b9a-5d96-4fbd-8e2d-a2440225f93a}"/>
    </application>
  </compatibility>
</assembly>
"#;
        res.set_manifest(app_manifest);

        if let Err(e) = res.compile() {
            eprintln!("Failed to compile windows resource: {e}");
        }
    }
}
