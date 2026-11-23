#![allow(dead_code)]

mod stream {
    pub fn build_magnet_uri(_hash: &str, _title: &str) -> String {
        String::new()
    }
}

#[path = "../src/types.rs"]
pub mod types;

#[path = "../src/windows_integration.rs"]
pub mod windows_integration;

#[test]
fn test_integration_module() {
    assert_eq!(windows_integration::PROG_ID, "Vesper.VideoFile");
    assert_eq!(windows_integration::VIDEO_EXTENSIONS.len(), 6);
}
