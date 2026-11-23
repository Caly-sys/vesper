use std::ffi::c_void;
use std::time::Duration;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
    RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::UI::Shell::{
    ITaskbarList3, TaskbarList, TBPF_NOPROGRESS, TBPF_NORMAL, TBPF_PAUSED,
    SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify,
};

use crate::types::{AppEvent, MediaKeyAction};

pub const VIDEO_EXTENSIONS: &[&str] = &[".mkv", ".mp4", ".webm", ".mov", ".avi", ".m4v"];
pub const PROG_ID: &str = "Vesper.VideoFile";

pub struct WindowsIntegration {
    controls: Option<MediaControls>,
    taskbar: Option<ITaskbarList3>,
    hwnd: HWND,
}

impl WindowsIntegration {
    pub fn new(
        hwnd: HWND,
        event_proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    ) -> Self {
        let mut controls = None;
        let config = PlatformConfig {
            display_name: "Vesper",
            dbus_name: "org.mpris.MediaPlayer2.vesper",
            hwnd: Some(hwnd.0 as *mut c_void),
        };

        if let Ok(mut ctrl) = MediaControls::new(config) {
            let proxy = event_proxy.clone();
            let attach_res = ctrl.attach(move |event| {
                let action = match event {
                    MediaControlEvent::Play => Some(MediaKeyAction::Play),
                    MediaControlEvent::Pause => Some(MediaKeyAction::Pause),
                    MediaControlEvent::Toggle => Some(MediaKeyAction::Toggle),
                    MediaControlEvent::Next => Some(MediaKeyAction::Next),
                    MediaControlEvent::Previous => Some(MediaKeyAction::Previous),
                    MediaControlEvent::Stop => Some(MediaKeyAction::Stop),
                    _ => None,
                };
                if let Some(act) = action {
                    let _ = proxy.send_event(AppEvent::MediaKey(act));
                }
            });

            if attach_res.is_ok() {
                controls = Some(ctrl);
            }
        }

        let taskbar = unsafe {
            CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER).ok().and_then(|tb| {
                if tb.HrInit().is_ok() {
                    Some(tb)
                } else {
                    None
                }
            })
        };

        Self {
            controls,
            taskbar,
            hwnd,
        }
    }

    pub fn update_metadata(&mut self, title: &str, duration_secs: f64) {
        if let Some(ref mut controls) = self.controls {
            let duration = if duration_secs > 0.0 {
                Some(Duration::from_secs_f64(duration_secs))
            } else {
                None
            };
            let metadata = MediaMetadata {
                title: Some(title),
                duration,
                ..Default::default()
            };
            let _ = controls.set_metadata(metadata);
        }
    }

    pub fn update_playback_state(&mut self, is_playing: bool, position_secs: f64, duration_secs: f64) {
        let pos = Duration::from_secs_f64(position_secs.max(0.0));
        let playback = if is_playing {
            MediaPlayback::Playing {
                progress: Some(MediaPosition(pos)),
            }
        } else {
            MediaPlayback::Paused {
                progress: Some(MediaPosition(pos)),
            }
        };

        if let Some(ref mut controls) = self.controls {
            let _ = controls.set_playback(playback);
        }

        if let Some(ref taskbar) = self.taskbar {
            unsafe {
                let _ = taskbar.SetProgressValue(
                    self.hwnd,
                    (position_secs.max(0.0) * 1000.0) as u64,
                    (duration_secs.max(0.0) * 1000.0) as u64,
                );
                let _ = taskbar.SetProgressState(
                    self.hwnd,
                    if is_playing { TBPF_NORMAL } else { TBPF_PAUSED },
                );
            }
        }
    }

    pub fn clear_progress(&mut self) {
        if let Some(ref taskbar) = self.taskbar {
            unsafe {
                let _ = taskbar.SetProgressState(self.hwnd, TBPF_NOPROGRESS);
            }
        }
        if let Some(ref mut controls) = self.controls {
            let _ = controls.set_playback(MediaPlayback::Stopped);
        }
    }

    pub fn register_file_associations() -> Result<(), String> {
        let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_str = exe_path.display().to_string();

        let prog_key = format!("Software\\Classes\\{}", PROG_ID);
        set_reg_sz(HKEY_CURRENT_USER, &prog_key, None, "Vesper Media File")?;

        let icon_key = format!("{}\\DefaultIcon", prog_key);
        set_reg_sz(HKEY_CURRENT_USER, &icon_key, None, &format!("\"{}\",0", exe_str))?;

        let cmd_key = format!("{}\\shell\\open\\command", prog_key);
        set_reg_sz(HKEY_CURRENT_USER, &cmd_key, None, &format!("\"{}\" \"%1\"", exe_str))?;

        for ext in VIDEO_EXTENSIONS {
            let ext_key = format!("Software\\Classes\\{}", ext);
            set_reg_sz(HKEY_CURRENT_USER, &ext_key, None, PROG_ID)?;

            let openwith_key = format!("{}\\OpenWithProgids", ext_key);
            set_reg_sz(HKEY_CURRENT_USER, &openwith_key, Some(PROG_ID), "")?;
        }

        unsafe {
            SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
        }

        Ok(())
    }

    pub fn unregister_file_associations() -> Result<(), String> {
        let prog_key = format!("Software\\Classes\\{}", PROG_ID);
        let _ = delete_reg_tree(HKEY_CURRENT_USER, &prog_key);

        for ext in VIDEO_EXTENSIONS {
            let ext_key = format!("Software\\Classes\\{}", ext);
            if get_reg_sz(HKEY_CURRENT_USER, &ext_key, None).as_deref() == Some(PROG_ID) {
                let _ = delete_reg_value(HKEY_CURRENT_USER, &ext_key, None);
            }

            let openwith_key = format!("{}\\OpenWithProgids", ext_key);
            let _ = delete_reg_value(HKEY_CURRENT_USER, &openwith_key, Some(PROG_ID));
        }

        unsafe {
            SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
        }

        Ok(())
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn set_reg_sz(hkey: HKEY, subkey: &str, value_name: Option<&str>, data: &str) -> Result<(), String> {
    let subkey_w = to_wide(subkey);
    let val_w = value_name.map(to_wide);
    let val_ptr = val_w.as_ref().map_or(PCWSTR::null(), |v| PCWSTR::from_raw(v.as_ptr()));
    let data_w = to_wide(data);

    let res = unsafe {
        RegSetKeyValueW(
            hkey,
            PCWSTR::from_raw(subkey_w.as_ptr()),
            val_ptr,
            REG_SZ.0,
            Some(data_w.as_ptr() as *const c_void),
            (data_w.len() * std::mem::size_of::<u16>()) as u32,
        )
    };
    if res == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("RegSetKeyValueW failed on {}: code {}", subkey, res.0))
    }
}

fn get_reg_sz(hkey: HKEY, subkey: &str, value_name: Option<&str>) -> Option<String> {
    let subkey_w = to_wide(subkey);
    let val_w = value_name.map(to_wide);
    let val_ptr = val_w.as_ref().map_or(PCWSTR::null(), |v| PCWSTR::from_raw(v.as_ptr()));

    let mut buf = [0u16; 512];
    let mut cb_data = (buf.len() * std::mem::size_of::<u16>()) as u32;

    let res = unsafe {
        RegGetValueW(
            hkey,
            PCWSTR::from_raw(subkey_w.as_ptr()),
            val_ptr,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut cb_data),
        )
    };

    if res == ERROR_SUCCESS {
        let char_len = (cb_data as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buf[..char_len]))
    } else {
        None
    }
}

fn delete_reg_value(hkey: HKEY, subkey: &str, value_name: Option<&str>) -> Result<(), String> {
    let subkey_w = to_wide(subkey);
    let val_w = value_name.map(to_wide);
    let val_ptr = val_w.as_ref().map_or(PCWSTR::null(), |v| PCWSTR::from_raw(v.as_ptr()));

    let res = unsafe {
        RegDeleteKeyValueW(
            hkey,
            PCWSTR::from_raw(subkey_w.as_ptr()),
            val_ptr,
        )
    };
    if res == ERROR_SUCCESS || res.0 == 2 {
        Ok(())
    } else {
        Err(format!("RegDeleteKeyValueW failed on {}: code {}", subkey, res.0))
    }
}

fn delete_reg_tree(hkey: HKEY, subkey: &str) -> Result<(), String> {
    let subkey_w = to_wide(subkey);
    let res = unsafe {
        RegDeleteTreeW(
            hkey,
            PCWSTR::from_raw(subkey_w.as_ptr()),
        )
    };
    if res == ERROR_SUCCESS || res.0 == 2 {
        Ok(())
    } else {
        Err(format!("RegDeleteTreeW failed on {}: code {}", subkey, res.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_path_safety_and_generation() {
        assert_eq!(PROG_ID, "Vesper.VideoFile");

        let prog_key = format!("Software\\Classes\\{}", PROG_ID);
        assert!(prog_key.starts_with("Software\\Classes\\"));
        assert!(!prog_key.contains(".."));

        let icon_key = format!("{}\\DefaultIcon", prog_key);
        let cmd_key = format!("{}\\shell\\open\\command", prog_key);
        assert_eq!(icon_key, "Software\\Classes\\Vesper.VideoFile\\DefaultIcon");
        assert_eq!(cmd_key, "Software\\Classes\\Vesper.VideoFile\\shell\\open\\command");

        let dummy_exe = "C:\\Program Files\\Vesper\\vesper.exe";
        let icon_val = format!("\"{}\",0", dummy_exe);
        let cmd_val = format!("\"{}\" \"%1\"", dummy_exe);
        assert_eq!(icon_val, "\"C:\\Program Files\\Vesper\\vesper.exe\",0");
        assert_eq!(cmd_val, "\"C:\\Program Files\\Vesper\\vesper.exe\" \"%1\"");

        for ext in VIDEO_EXTENSIONS {
            assert!(ext.starts_with('.'));
            assert_eq!(ext.to_lowercase(), *ext);
            let ext_key = format!("Software\\Classes\\{}", ext);
            let openwith_key = format!("{}\\OpenWithProgids", ext_key);
            assert!(ext_key.starts_with("Software\\Classes\\."));
            assert_eq!(openwith_key, format!("Software\\Classes\\{}\\OpenWithProgids", ext));
        }
    }
}
