#![allow(dead_code)]

pub mod controls;
pub mod layout;
pub mod theme;

use std::collections::VecDeque;
use std::ffi::c_void;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, CreateSolidBrush, DeleteObject, EndPaint, FillRect, GetDC,
    InvalidateRect, ReleaseDC, HDC, HGDIOBJ, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TRACKMOUSEEVENT, TME_LEAVE,
};
use windows::Win32::UI::Shell::DragAcceptFiles;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, GetClientRect,
    GetCursorPos, GetParent, GetWindowLongPtrW, GetWindowLongW, GetWindowRect, LoadCursorW,
    PostMessageW, RegisterClassExW, SetWindowLongPtrW, SetWindowLongW, SetWindowPos, SetWindowTextW,
    ShowWindow, TrackPopupMenuEx, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, GWL_STYLE, HICON,
    IDC_ARROW, MF_CHECKED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, SWP_NOACTIVATE, SWP_NOZORDER,
    SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE, TPM_BOTTOMALIGN, TPM_RETURNCMD, WINDOW_EX_STYLE,
    WNDCLASSEXW, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};

const WM_MOUSELEAVE: u32 = 0x02A3;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::EventLoopProxy;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::types::{
    AppEvent, BatchMode, BufferingStatus, NyaaCategory, NyaaFilter, NyaaItem, PlayerCommand,
    StreamStatus, SubCue,
};
use crate::ui::controls::{format_time, render_ui, HoverControl, Tab, UiState};
use crate::ui::layout::{LayoutGeometry, Rect};
use crate::ui::theme::{COLOR_SURFACE, COLOR_VIEWPORT_BG, DoubleBuffer, ThemeFonts};

/// Actions emitted by the UI for the main application / orchestrator loop to process
#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
    Player(PlayerCommand),
    LoadFile(PathBuf),
    OpenFilePicker,
    OpenFolderPicker,
    SearchNyaa {
        query: String,
        category: NyaaCategory,
        filter: NyaaFilter,
    },
    StreamNyaa(NyaaItem),
    ExtractSubtitle {
        track_id: Option<i64>,
        destination: Option<PathBuf>,
    },
    SelectBatchTorrentFile {
        id: usize,
        file_idx: usize,
    },
    StartBatchDownload {
        id: usize,
        file_indices: Vec<usize>,
    },
    ToggleFullscreen,
    StartTorrentStream {
        source: String,
        title: String,
    },
    StartTorrentDownload {
        source: String,
        fallback_source: Option<String>,
        title: String,
    },
    PauseTorrent(usize),
    ResumeTorrent(usize),
    CancelTorrent {
        id: usize,
        delete_files: bool,
    },
    DeleteTorrent(usize),
    OpenTorrentFolder(PathBuf),
    OpenBatchPicker(usize),
    ChangeDownloadDir,
    SyncTrackersNow(String),
    SaveTrackers {
        auto_sync: bool,
        url: String,
        custom_trackers: Vec<String>,
    },
    ResetTrackerDefaults,
    CopyText(String),
    OpenExternal(String),
    FetchSubtitles,
    AttachExternalSubtitle(crate::subtitles::SubtitleSearchResult),
    SaveSubtitleConfig {
        auto_fetch: bool,
        preferred_lang: String,
        opensubtitles_key: Option<String>,
        jimaku_key: Option<String>,
    },
    RegisterFileAssociations,
}

unsafe extern "system" fn video_host_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // Prevent background flicker behind libmpv video surface and fill with #0E0E11
        windows::Win32::UI::WindowsAndMessaging::WM_ERASEBKGND => {
            let hdc = HDC(wparam.0 as *mut c_void);
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            let brush = CreateSolidBrush(COLOR_VIEWPORT_BG);
            let _ = FillRect(hdc, &rect, brush);
            let _ = DeleteObject(HGDIOBJ(brush.0));
            LRESULT(1)
        }
        // Forward dropped files from child video window to parent winit window
        windows::Win32::UI::WindowsAndMessaging::WM_DROPFILES => {
            let parent = GetParent(hwnd).unwrap_or_default();
            if !parent.is_invalid() {
                let _ = PostMessageW(Some(parent), msg, wparam, lparam);
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        // Forward mouse move to parent so winit / UI immediately wakes up
        windows::Win32::UI::WindowsAndMessaging::WM_MOUSEMOVE => {
            let parent = GetParent(hwnd).unwrap_or_default();
            if !parent.is_invalid() {
                let _ = PostMessageW(Some(parent), msg, wparam, lparam);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn bottom_bar_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut VesperUi;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ui = &mut *ptr;

    match msg {
        windows::Win32::UI::WindowsAndMessaging::WM_ERASEBKGND => LRESULT(1),

        windows::Win32::UI::WindowsAndMessaging::WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            ui.render_bar_overlay(hdc);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }

        windows::Win32::UI::WindowsAndMessaging::WM_MOUSEMOVE => {
            let px = (lparam.0 & 0xffff) as i16 as i32;
            let py = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tme);
            ui.handle_bar_mouse_move(px, py);
            LRESULT(0)
        }

        windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONDOWN => {
            let px = (lparam.0 & 0xffff) as i16 as i32;
            let py = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            ui.handle_bar_mouse_down(px, py);
            LRESULT(0)
        }

        windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONUP => {
            ui.handle_bar_mouse_up();
            LRESULT(0)
        }

        WM_MOUSELEAVE => {
            ui.handle_bar_mouse_leave();
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn pill_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        windows::Win32::UI::WindowsAndMessaging::WM_NCHITTEST => LRESULT(-1), // HTTRANSPARENT: let clicks pass straight through
        windows::Win32::UI::WindowsAndMessaging::WM_ERASEBKGND => LRESULT(1),
        windows::Win32::UI::WindowsAndMessaging::WM_PAINT => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut VesperUi;
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !ptr.is_null() {
                let ui = &mut *ptr;
                ui.render_pill_overlay(hdc);
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub struct VesperUi {
    parent_hwnd: HWND,
    video_child_hwnd: HWND,
    pub bar_hwnd: HWND,
    pub pill_hwnd: HWND,
    event_proxy: EventLoopProxy<AppEvent>,
    action_queue: VecDeque<UiAction>,
    geom: LayoutGeometry,
    pub state: UiState,
    fonts: ThemeFonts,
    double_buffer: DoubleBuffer,
    bar_double_buffer: DoubleBuffer,
    pill_double_buffer: DoubleBuffer,
    pub width: u32,
    pub height: u32,
    modifiers: ModifiersState,
    last_cursor_screen_pos: (i32, i32),
}

impl VesperUi {
    /// Initialize native Win32 dark theme UI, create child video HWND for libmpv, and setup drag-and-drop.
    pub fn new(window: &winit::window::Window, event_proxy: EventLoopProxy<AppEvent>) -> Self {
        // Extract native HWND from winit window
        let raw_hwnd_isize = if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Win32(w) = handle.as_raw() {
                w.hwnd.get() as isize
            } else {
                0
            }
        } else {
            0
        };

        let parent_hwnd = HWND(raw_hwnd_isize as *mut c_void);

        // 1. Titlebar: Immersive dark mode via DwmSetWindowAttribute
        let dark_val: i32 = 1;
        unsafe {
            let _ = DwmSetWindowAttribute(
                parent_hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark_val as *const _ as *const c_void,
                std::mem::size_of::<i32>() as u32,
            );
        }

        // Set initial window title
        let title_utf16: Vec<u16> = "Vesper\0".encode_utf16().collect();
        unsafe {
            let _ = SetWindowTextW(parent_hwnd, PCWSTR::from_raw(title_utf16.as_ptr()));
        }

        // 2. Add WS_CLIPCHILDREN to parent window style so double-buffer painting clips child video
        unsafe {
            let current_style = GetWindowLongW(parent_hwnd, GWL_STYLE);
            let _ = SetWindowLongW(
                parent_hwnd,
                GWL_STYLE,
                current_style | WS_CLIPCHILDREN.0 as i32,
            );
        }

        // 3. Enable drag-and-drop of .mkv files
        unsafe {
            DragAcceptFiles(parent_hwnd, true);
        }

        // 4. Register child window class for hosting libmpv with dark solid viewport brush (#0E0E11)
        let class_name: Vec<u16> = "VesperVideoHostWindow\0".encode_utf16().collect();
        let hmodule = unsafe { GetModuleHandleW(None).unwrap_or_default() };
        let hinst = HINSTANCE(hmodule.0);
        let bg_brush = unsafe { CreateSolidBrush(COLOR_VIEWPORT_BG) };

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(video_host_wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: HICON::default(),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
            hbrBackground: bg_brush,
            lpszMenuName: PCWSTR::null(),
            lpszClassName: PCWSTR::from_raw(class_name.as_ptr()),
            hIconSm: HICON::default(),
        };
        unsafe {
            let _ = RegisterClassExW(&wc);
        }

        let inner_size = window.inner_size();
        let width = inner_size.width.max(640);
        let height = inner_size.height.max(480);

        let state = UiState::default();
        let sub_lbl = controls::sub_button_label(&state);
        let aud_lbl = controls::audio_button_label(&state);
        let geom = LayoutGeometry::calculate(
            width as i32,
            height as i32,
            state.drawer_open,
            state.duration,
            &sub_lbl,
            &aud_lbl,
        );

        // 5. Create native child HWND bound to libmpv (starts hidden until video is loaded)
        let video_child_hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR::from_raw(class_name.as_ptr()),
                PCWSTR::null(),
                WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                geom.video_rect.x,
                geom.video_rect.y,
                geom.video_rect.w,
                geom.video_rect.h,
                Some(parent_hwnd),
                None,
                Some(hinst),
                None,
            )
            .unwrap_or_default()
        };

        unsafe {
            let _ = ShowWindow(video_child_hwnd, SW_HIDE);
            DragAcceptFiles(video_child_hwnd, true);
        }

        // 6. Register & create borderless owned WS_POPUP bottom playback bar overlay
        let bar_class_name: Vec<u16> = "VesperBottomBarOverlay\0".encode_utf16().collect();
        let bar_brush = unsafe { CreateSolidBrush(theme::COLOR_SURFACE) };
        let bar_wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(bottom_bar_wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: HICON::default(),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
            hbrBackground: bar_brush,
            lpszMenuName: PCWSTR::null(),
            lpszClassName: PCWSTR::from_raw(bar_class_name.as_ptr()),
            hIconSm: HICON::default(),
        };
        unsafe {
            let _ = RegisterClassExW(&bar_wc);
        }

        let mut screen_pt = POINT {
            x: 0,
            y: (height as i32 - 54).max(0),
        };
        unsafe {
            let _ = ClientToScreen(parent_hwnd, &mut screen_pt);
        }

        let bar_hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                PCWSTR::from_raw(bar_class_name.as_ptr()),
                PCWSTR::null(),
                WS_POPUP | WS_VISIBLE,
                screen_pt.x,
                screen_pt.y,
                geom.video_rect.w,
                54,
                Some(parent_hwnd),
                None,
                Some(hinst),
                None,
            )
            .unwrap_or_default()
        };

        let pill_class_name: Vec<u16> = "VesperPillOverlay\0".encode_utf16().collect();
        let pill_brush = unsafe { CreateSolidBrush(COLOR_SURFACE) };
        let pill_wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(pill_wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: HICON::default(),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
            hbrBackground: pill_brush,
            lpszMenuName: PCWSTR::null(),
            lpszClassName: PCWSTR::from_raw(pill_class_name.as_ptr()),
            hIconSm: HICON::default(),
        };
        unsafe {
            let _ = RegisterClassExW(&pill_wc);
        }

        let pill_w = 240;
        let pill_h = 38;
        let mut pill_screen_pt = POINT {
            x: geom.video_rect.x + (geom.video_rect.w - pill_w) / 2,
            y: geom.video_rect.y + (geom.video_rect.h - pill_h) / 2,
        };
        unsafe {
            let _ = ClientToScreen(parent_hwnd, &mut pill_screen_pt);
        }

        let pill_hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                PCWSTR::from_raw(pill_class_name.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                pill_screen_pt.x,
                pill_screen_pt.y,
                pill_w,
                pill_h,
                Some(parent_hwnd),
                None,
                Some(hinst),
                None,
            )
            .unwrap_or_default()
        };

        Self {
            parent_hwnd,
            video_child_hwnd,
            bar_hwnd,
            pill_hwnd,
            event_proxy,
            action_queue: VecDeque::new(),
            geom,
            state,
            fonts: ThemeFonts::new(),
            double_buffer: DoubleBuffer::new(),
            bar_double_buffer: DoubleBuffer::new(),
            pill_double_buffer: DoubleBuffer::new(),
            width,
            height,
            modifiers: ModifiersState::empty(),
            last_cursor_screen_pos: (0, 0),
        }
    }

    /// Link heap pointer to overlay HWND user data
    pub fn init_overlay(&mut self) {
        unsafe {
            SetWindowLongPtrW(
                self.bar_hwnd,
                GWLP_USERDATA,
                self as *mut VesperUi as isize,
            );
            SetWindowLongPtrW(
                self.pill_hwnd,
                GWLP_USERDATA,
                self as *mut VesperUi as isize,
            );
        }
        self.sync_overlay_position();
    }

    /// Sync overlay screen position and width with main window
    pub fn sync_overlay_position(&mut self) {
        unsafe {
            if self.parent_hwnd.is_invalid() {
                return;
            }
            if !self.bar_hwnd.is_invalid() {
                let mut screen_pt = POINT {
                    x: 0,
                    y: (self.height as i32 - 54).max(0),
                };
                let _ = ClientToScreen(self.parent_hwnd, &mut screen_pt);
                let bar_w = self.geom.video_rect.w;
                let _ = SetWindowPos(
                    self.bar_hwnd,
                    None,
                    screen_pt.x,
                    screen_pt.y,
                    bar_w,
                    54,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            if !self.pill_hwnd.is_invalid() {
                let pill_w = 240;
                let pill_h = 38;
                let mut pill_screen_pt = POINT {
                    x: self.geom.video_rect.x + (self.geom.video_rect.w - pill_w) / 2,
                    y: self.geom.video_rect.y + (self.geom.video_rect.h - pill_h) / 2,
                };
                let _ = ClientToScreen(self.parent_hwnd, &mut pill_screen_pt);
                let _ = SetWindowPos(
                    self.pill_hwnd,
                    None,
                    pill_screen_pt.x,
                    pill_screen_pt.y,
                    pill_w,
                    pill_h,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        }
    }

    /// Toggle visibility of overlay bottom bar
    pub fn set_bar_visible(&mut self, visible: bool) {
        self.state.bar_visible = visible;
        unsafe {
            let cmd = if visible { SW_SHOWNOACTIVATE } else { SW_HIDE };
            let _ = ShowWindow(self.bar_hwnd, cmd);
            if visible {
                let _ = InvalidateRect(Some(self.bar_hwnd), None, false);
            }
        }
    }

    /// Returns true if a video is actively loaded and playing
    pub fn is_video_playing(&self) -> bool {
        self.state.current_path.is_some() && self.state.duration > 0.0 && !self.state.paused
    }

    /// Periodic mouse activity and auto-hide checker
    pub fn check_inactivity(&mut self) {
        unsafe {
            let mut pt = POINT::default();
            if GetCursorPos(&mut pt).is_ok() {
                let mut rect = RECT::default();
                let _ = GetWindowRect(self.parent_hwnd, &mut rect);
                if pt.x >= rect.left && pt.x < rect.right && pt.y >= rect.top && pt.y < rect.bottom {
                    if (pt.x - self.last_cursor_screen_pos.0).abs() >= 2
                        || (pt.y - self.last_cursor_screen_pos.1).abs() >= 2
                    {
                        self.last_cursor_screen_pos = (pt.x, pt.y);
                        self.state.last_mouse_activity = Instant::now();
                        if !self.state.bar_visible {
                            self.set_bar_visible(true);
                        }
                    }
                }
            }
        }

        let is_playing = self.is_video_playing();
        let is_dragging = self.state.is_dragging_seekbar || self.state.is_dragging_volume;

        let mouse_over_bar = unsafe {
            let mut pt = POINT::default();
            if GetCursorPos(&mut pt).is_ok() {
                let mut bar_rect = RECT::default();
                let _ = GetWindowRect(self.bar_hwnd, &mut bar_rect);
                pt.x >= bar_rect.left && pt.x < bar_rect.right && pt.y >= bar_rect.top && pt.y < bar_rect.bottom
            } else {
                false
            }
        };

        if is_playing && !is_dragging && !mouse_over_bar {
            if self.state.last_mouse_activity.elapsed() >= std::time::Duration::from_millis(2500) {
                if self.state.bar_visible {
                    self.set_bar_visible(false);
                }
            }
        } else if !self.state.bar_visible {
            self.set_bar_visible(true);
        }
    }

    /// Render overlay bottom bar double buffer
    pub fn render_bar_overlay(&mut self, hdc: HDC) {
        let bar_w = self.geom.video_rect.w;
        let bar_h = 54;
        self.bar_double_buffer.ensure_size(hdc, bar_w, bar_h);

        if let Some(mem_dc) = self.bar_double_buffer.mem_dc {
            controls::render_bottom_bar(mem_dc, &self.geom, &self.state, &self.fonts);
            self.bar_double_buffer.present(hdc, 0, 0, bar_w, bar_h);
        }
    }

    /// Render floating mid-playback rebuffering pill
    pub fn render_pill_overlay(&mut self, hdc: HDC) {
        let pill_w = 240;
        let pill_h = 38;
        self.pill_double_buffer.ensure_size(hdc, pill_w, pill_h);

        if let Some(mem_dc) = self.pill_double_buffer.mem_dc {
            controls::render_rebuffering_pill(mem_dc, pill_w, pill_h, &self.state, &self.fonts);
            self.pill_double_buffer.present(hdc, 0, 0, pill_w, pill_h);
        }
    }

    /// Show or hide mid-playback rebuffering pill overlay
    pub fn set_rebuffering(&mut self, is_buffering: bool, percent: i64) {
        self.state.is_rebuffering = is_buffering;
        self.state.rebuffering_percent = percent;
        unsafe {
            let should_show = is_buffering && self.state.stream_buffering.is_none();
            let cmd = if should_show { SW_SHOWNOACTIVATE } else { SW_HIDE };
            let _ = ShowWindow(self.pill_hwnd, cmd);
            if should_show {
                let _ = InvalidateRect(Some(self.pill_hwnd), None, false);
            }
            if !self.bar_hwnd.is_invalid() && self.state.bar_visible {
                let _ = InvalidateRect(Some(self.bar_hwnd), None, false);
            }
        }
        self.request_redraw();
    }

    /// Redraw rebuffering indicators when speed/percent updates
    pub fn update_rebuffering_display(&mut self) {
        unsafe {
            if self.state.is_rebuffering && self.state.stream_buffering.is_none() {
                let _ = InvalidateRect(Some(self.pill_hwnd), None, false);
            }
            if !self.bar_hwnd.is_invalid() && self.state.bar_visible {
                let _ = InvalidateRect(Some(self.bar_hwnd), None, false);
            }
        }
    }

    /// Set initial stream buffering status (centers waiting card and hides direct3d child surface)
    pub fn set_stream_buffering(&mut self, buf: Option<BufferingStatus>) {
        let is_some = buf.is_some();
        self.state.stream_buffering = buf;
        if is_some {
            self.set_video_visible(false);
            unsafe {
                let _ = ShowWindow(self.pill_hwnd, SW_HIDE);
            }
        } else {
            self.set_video_visible(true);
            if self.state.is_rebuffering {
                unsafe {
                    let _ = ShowWindow(self.pill_hwnd, SW_SHOWNOACTIVATE);
                    let _ = InvalidateRect(Some(self.pill_hwnd), None, false);
                }
            }
        }
        self.request_redraw();
    }

    pub fn handle_bar_mouse_move(&mut self, px: i32, py: i32) {
        self.state.last_mouse_activity = Instant::now();
        if !self.state.bar_visible {
            self.set_bar_visible(true);
        }

        if self.state.is_dragging_seekbar {
            let sb = &self.geom.seekbar_rect;
            if sb.w > 0 && self.state.duration > 0.0 {
                let pct = ((px - sb.x) as f64 / sb.w as f64).clamp(0.0, 1.0);
                let target_time = pct * self.state.duration;
                self.state.time_pos = target_time;
                self.action_queue
                    .push_back(UiAction::Player(PlayerCommand::SeekAbsolute(target_time)));
            }
            self.request_redraw();
            return;
        }

        if self.state.is_dragging_volume {
            let vs = &self.geom.vol_slider_rect;
            if vs.w > 0 {
                let pct = ((px - vs.x) as f64 / vs.w as f64).clamp(0.0, 1.0);
                let target_vol = pct * 100.0;
                self.state.volume = target_vol;
                self.state.muted = false;
                self.action_queue
                    .push_back(UiAction::Player(PlayerCommand::SetVolume(target_vol)));
            }
            self.request_redraw();
            return;
        }

        let mut new_hover = None;
        if self.geom.btn_open.contains(px, py) {
            new_hover = Some(HoverControl::BottomBarOpen);
        } else if self.geom.btn_play_pause.contains(px, py) {
            new_hover = Some(HoverControl::PlayPause);
        } else if self.geom.btn_next.contains(px, py) {
            new_hover = Some(HoverControl::Next);
        } else if self.geom.seekbar_rect.contains(px, py) {
            new_hover = Some(HoverControl::Seekbar);
        } else if self.geom.btn_vol_icon.contains(px, py) {
            new_hover = Some(HoverControl::VolumeIcon);
        } else if self.geom.vol_slider_rect.contains(px, py) {
            new_hover = Some(HoverControl::VolumeSlider);
        } else if self.geom.btn_audio_toggle.contains(px, py) {
            new_hover = Some(HoverControl::AudioToggle);
        } else if self.geom.btn_sub_toggle.contains(px, py) {
            new_hover = Some(HoverControl::SubToggle);
        } else if self.geom.btn_fullscreen.contains(px, py) {
            new_hover = Some(HoverControl::Fullscreen);
        } else if self.geom.btn_drawer_toggle.contains(px, py) {
            new_hover = Some(HoverControl::DrawerToggle);
        }

        if self.state.hovered_control != new_hover {
            self.state.hovered_control = new_hover;
            self.request_redraw();
        }
    }

    pub fn handle_bar_mouse_down(&mut self, px: i32, py: i32) {
        self.state.last_mouse_activity = Instant::now();

        if self.geom.btn_open.contains(px, py) {
            self.action_queue.push_back(UiAction::OpenFilePicker);
        } else if self.geom.btn_play_pause.contains(px, py) {
            self.action_queue.push_back(UiAction::Player(PlayerCommand::TogglePause));
        } else if self.geom.btn_next.contains(px, py) {
            self.play_next_episode();
        } else if self.geom.seekbar_rect.contains(px, py) {
            self.state.is_dragging_seekbar = true;
            unsafe { SetCapture(self.bar_hwnd); }
            if self.geom.seekbar_rect.w > 0 && self.state.duration > 0.0 {
                let pct = ((px - self.geom.seekbar_rect.x) as f64 / self.geom.seekbar_rect.w as f64).clamp(0.0, 1.0);
                let target_time = pct * self.state.duration;
                self.state.time_pos = target_time;
                self.action_queue.push_back(UiAction::Player(PlayerCommand::SeekAbsolute(target_time)));
            }
        } else if self.geom.btn_vol_icon.contains(px, py) {
            if self.state.muted || self.state.volume <= 0.0 {
                self.state.volume = if self.state.unmuted_volume > 0.0 {
                    self.state.unmuted_volume
                } else {
                    100.0
                };
                self.state.muted = false;
            } else {
                self.state.unmuted_volume = self.state.volume;
                self.state.volume = 0.0;
                self.state.muted = true;
            }
            self.action_queue.push_back(UiAction::Player(PlayerCommand::SetVolume(self.state.volume)));
        } else if self.geom.vol_slider_rect.contains(px, py) {
            self.state.is_dragging_volume = true;
            unsafe { SetCapture(self.bar_hwnd); }
            let pct = ((px - self.geom.vol_slider_rect.x) as f64 / self.geom.vol_slider_rect.w as f64).clamp(0.0, 1.0);
            self.state.volume = pct * 100.0;
            self.state.muted = false;
            self.action_queue.push_back(UiAction::Player(PlayerCommand::SetVolume(self.state.volume)));
        } else if self.geom.btn_audio_toggle.contains(px, py) {
            self.show_audio_track_menu();
        } else if self.geom.btn_sub_toggle.contains(px, py) {
            self.show_sub_track_menu();
        } else if self.geom.btn_fullscreen.contains(px, py) {
            self.state.is_fullscreen = !self.state.is_fullscreen;
            self.action_queue.push_back(UiAction::ToggleFullscreen);
        } else if self.geom.btn_drawer_toggle.contains(px, py) {
            self.state.drawer_open = !self.state.drawer_open;
            self.update_layout();
        }

        self.request_redraw();
    }

    pub fn handle_bar_mouse_up(&mut self) {
        if self.state.is_dragging_seekbar || self.state.is_dragging_volume {
            unsafe { let _ = ReleaseCapture(); }
            self.state.is_dragging_seekbar = false;
            self.state.is_dragging_volume = false;
            self.request_redraw();
        }
    }

    pub fn handle_bar_mouse_leave(&mut self) {
        if !self.state.is_dragging_seekbar && !self.state.is_dragging_volume {
            self.state.hovered_control = None;
            self.request_redraw();
        }
    }

    /// Toggle visibility of native libmpv child HWND (shown when media plays, hidden when idle)
    pub fn set_video_visible(&mut self, visible: bool) {
        unsafe {
            let cmd = if visible { SW_SHOW } else { SW_HIDE };
            let _ = ShowWindow(self.video_child_hwnd, cmd);
        }
        self.request_redraw();
    }

    /// Expose native child HWND for libmpv embedding
    pub fn video_hwnd(&self) -> isize {
        self.video_child_hwnd.0 as isize
    }

    /// Expose top-level parent HWND
    pub fn main_hwnd(&self) -> windows::Win32::Foundation::HWND {
        self.parent_hwnd
    }

    /// Window resize handler: updates layout geometry and resizes child video HWND
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        self.update_layout();
        self.request_redraw();
    }

    /// Update layout and reposition native video child window
    pub fn update_layout(&mut self) {
        let sub_lbl = controls::sub_button_label(&self.state);
        let aud_lbl = controls::audio_button_label(&self.state);
        self.geom = LayoutGeometry::calculate(
            self.width as i32,
            self.height as i32,
            self.state.drawer_open,
            self.state.duration,
            &sub_lbl,
            &aud_lbl,
        );

        unsafe {
            let _ = SetWindowPos(
                self.video_child_hwnd,
                None,
                self.geom.video_rect.x,
                self.geom.video_rect.y,
                self.geom.video_rect.w,
                self.geom.video_rect.h,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        self.sync_overlay_position();
    }

    /// Handle winit WindowEvent: mouse, keyboard, drag-and-drop, redrawing.
    /// Returns true if the event was handled by the UI.
    pub fn handle_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
                false
            }

            WindowEvent::Resized(physical_size) => {
                self.resize(physical_size.width, physical_size.height);
                true
            }

            WindowEvent::RedrawRequested => {
                self.render();
                true
            }

            WindowEvent::DroppedFile(path) => {
                let p = path.clone();
                if crate::stream::is_torrent_file(&p) {
                    let filename = p
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Torrent")
                        .to_string();
                    if cfg!(feature = "torrent") {
                        self.set_stream_buffering(Some(BufferingStatus {
                            title: filename.clone(),
                            stage: "Fetching torrent metadata...".to_string(),
                            buffered_bytes: 0,
                            total_bytes: 0,
                            speed_bytes: 0,
                            peers: 0,
                        }));
                        self.state.torrent_status_text =
                            "Fetching torrent file list...".to_string();
                        self.state.status_message = Some((
                            "Fetching torrent file list...".to_string(),
                            Instant::now(),
                        ));
                        self.action_queue.push_back(UiAction::StartTorrentStream {
                            source: p.to_string_lossy().to_string(),
                            title: filename,
                        });
                        self.state.active_tab = Tab::Nyaa;
                    } else {
                        self.action_queue
                            .push_back(UiAction::OpenExternal(p.to_string_lossy().to_string()));
                    }
                    self.request_redraw();
                    return true;
                }

                self.set_video_visible(true);
                let filename = p
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Media")
                    .to_string();

                self.set_title(&filename);
                self.state.current_path = Some(p.clone());
                self.state.media_title = filename;

                // Enqueue load action and player command
                self.action_queue.push_back(UiAction::LoadFile(p.clone()));
                self.action_queue
                    .push_back(UiAction::Player(PlayerCommand::Load(
                        p.display().to_string(),
                    )));

                self.request_redraw();
                true
            }

            WindowEvent::CursorMoved { position, .. } => {
                let (px, py) = (position.x as i32, position.y as i32);
                self.state.mouse_pos = (px, py);
                self.state.last_mouse_activity = Instant::now();
                if !self.state.bar_visible {
                    self.set_bar_visible(true);
                }

                // Handle active dragging of seekbar
                if self.state.is_dragging_seekbar {
                    let sb = &self.geom.seekbar_rect;
                    if sb.w > 0 && self.state.duration > 0.0 {
                        let pct = ((px - sb.x) as f64 / sb.w as f64).clamp(0.0, 1.0);
                        let target_time = pct * self.state.duration;
                        self.state.time_pos = target_time;
                        self.action_queue
                            .push_back(UiAction::Player(PlayerCommand::SeekAbsolute(target_time)));
                    }
                    self.request_redraw();
                    return true;
                }

                // Handle active dragging of volume slider
                if self.state.is_dragging_volume {
                    let vs = &self.geom.vol_slider_rect;
                    if vs.w > 0 {
                        let pct = ((px - vs.x) as f64 / vs.w as f64).clamp(0.0, 1.0);
                        let target_vol = pct * 100.0;
                        self.state.volume = target_vol;
                        self.state.muted = false;
                        self.action_queue
                            .push_back(UiAction::Player(PlayerCommand::SetVolume(target_vol)));
                    }
                    self.request_redraw();
                    return true;
                }

                // Hit-testing hovered control
                let prev_hover = self.state.hovered_control.clone();
                self.update_hover(px, py);

                if prev_hover != self.state.hovered_control || self.state.tooltip.is_some() {
                    self.request_redraw();
                }
                true
            }

            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let (px, py) = self.state.mouse_pos;
                self.handle_mouse_down(px, py)
            }

            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                let was_dragging =
                    self.state.is_dragging_seekbar || self.state.is_dragging_volume;
                self.state.is_dragging_seekbar = false;
                self.state.is_dragging_volume = false;
                if was_dragging {
                    self.request_redraw();
                }
                true
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let scroll_delta = match delta {
                    MouseScrollDelta::LineDelta(_x, y) => (*y * 36.0) as i32,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as i32,
                };

                // Scroll modal list if batch modal is open
                if let Some(ref mut modal) = self.state.batch_modal {
                    let total_content_h = (modal.files.len() as i32) * 36;
                    let list_h = (440.min(self.geom.total_height - 60) - 78 - 52).max(1);
                    let max_scroll = (total_content_h - list_h).max(0);
                    modal.scroll_y = (modal.scroll_y - scroll_delta).clamp(0, max_scroll);
                    self.request_redraw();
                    return true;
                }

                let (px, _py) = self.state.mouse_pos;
                if self.geom.drawer_rect.contains(px, self.state.mouse_pos.1) {
                    match self.state.active_tab {
                        Tab::Subtitles => {
                            self.state.sub_scroll_y =
                                (self.state.sub_scroll_y - scroll_delta).max(0);
                        }
                        Tab::Episodes => {
                            self.state.ep_scroll_y =
                                (self.state.ep_scroll_y - scroll_delta).max(0);
                        }
                        Tab::Nyaa => {
                            self.state.nyaa_scroll_y =
                                (self.state.nyaa_scroll_y - scroll_delta).max(0);
                        }
                    }
                    self.request_redraw();
                    true
                } else if self.geom.video_rect.contains(px, self.state.mouse_pos.1) {
                    // Scroll over video adjusts volume by ±5
                    let new_vol = (self.state.volume + if scroll_delta > 0 { 5.0 } else { -5.0 })
                        .clamp(0.0, 100.0);
                    self.state.volume = new_vol;
                    self.state.muted = false;
                    self.action_queue
                        .push_back(UiAction::Player(PlayerCommand::SetVolume(new_vol)));
                    self.request_redraw();
                    true
                } else {
                    false
                }
            }

            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        logical_key,
                        ..
                    },
                ..
            } => self.handle_key_down(logical_key),

            _ => false,
        }
    }

    /// Convenience wrapper to handle winit Event
    pub fn handle_winit_event(&mut self, event: &winit::event::Event<AppEvent>) -> bool {
        match event {
            winit::event::Event::WindowEvent { event, .. } => self.handle_event(event),
            winit::event::Event::UserEvent(app_event) => {
                self.handle_app_event(app_event);
                true
            }
            _ => false,
        }
    }

    /// Process application-wide events emitted by player or background tasks
    pub fn handle_app_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::FileLoaded {
                title,
                duration,
                path,
            } => {
                if path.is_some() {
                    self.set_stream_buffering(None);
                }
                self.state.media_title = title.clone();
                self.state.duration = *duration;
                self.state.current_path = path.clone();
                self.set_title(title);
                self.state.cues.clear();
                self.state.fetched_subtitles.clear();
                self.state.is_searching_subs = false;
                self.state.active_cue_idx = None;
                self.state.sub_popover_open = false;
                self.state.audio_popover_open = false;
                self.update_layout();
                self.request_redraw();
            }

            AppEvent::TimePosChanged(pos) => {
                self.state.time_pos = *pos;
                if *pos > 0.0 && self.state.stream_buffering.is_some() {
                    self.set_stream_buffering(None);
                }
                // Highlight active cue
                if !self.state.cues.is_empty() {
                    let mut found = None;
                    for (i, cue) in self.state.cues.iter().enumerate() {
                        if cue.timestamp <= *pos {
                            if let Some(end) = cue.end_time {
                                if *pos <= end {
                                    found = Some(i);
                                }
                            } else {
                                found = Some(i);
                            }
                        }
                    }
                    if self.state.active_cue_idx != found {
                        self.state.active_cue_idx = found;
                        // Auto-scroll slightly to active cue if needed
                        if let Some(idx) = found {
                            let card_y = (idx as i32) * 78;
                            let view_h = self.geom.subtitle_list_rect.h;
                            if card_y < self.state.sub_scroll_y
                                || card_y > self.state.sub_scroll_y + view_h - 90
                            {
                                self.state.sub_scroll_y = (card_y - view_h / 3).max(0);
                            }
                        }
                    }
                }
                self.request_redraw();
            }

            AppEvent::DurationChanged(dur) => {
                self.state.duration = *dur;
                self.update_layout();
                self.request_redraw();
            }

            AppEvent::PauseChanged(paused) => {
                self.state.paused = *paused;
                self.request_redraw();
            }

            AppEvent::VolumeChanged(vol) => {
                self.state.volume = *vol;
                self.request_redraw();
            }

            AppEvent::ChaptersLoaded(chapters) => {
                self.state.chapters = chapters.clone();
                self.request_redraw();
            }

            AppEvent::TracksLoaded(tracks) => {
                self.state.tracks = tracks.clone();
                if self.state.selected_extract_sub_id.is_none() {
                    self.state.selected_extract_sub_id = tracks
                        .iter()
                        .find(|t| t.selected)
                        .map(|t| t.id)
                        .or_else(|| tracks.first().map(|t| t.id));
                }
                self.update_layout();
                self.request_redraw();
            }

            AppEvent::AudioTracksLoaded(tracks) => {
                self.state.audio_tracks = tracks.clone();
                self.update_layout();
                self.request_redraw();
            }

            AppEvent::SubTextChanged(text) => {
                if !text.trim().is_empty() {
                    let should_add = match self.state.cues.last() {
                        Some(last) => {
                            last.text.trim() != text.trim()
                                || (self.state.time_pos - last.timestamp).abs() > 1.5
                        }
                        None => true,
                    };
                    if should_add {
                        self.state.cues.push(SubCue {
                            timestamp: self.state.time_pos,
                            text: text.trim().to_string(),
                            end_time: None,
                        });
                        self.state.active_cue_idx = Some(self.state.cues.len() - 1);
                        self.request_redraw();
                    }
                }
            }

            AppEvent::SubCueEmitted(cue) => {
                if !self
                    .state
                    .cues
                    .iter()
                    .any(|c| (c.timestamp - cue.timestamp).abs() < 0.2 && c.text == cue.text)
                {
                    self.state.cues.push(cue.clone());
                    self.state.cues.sort_by(|a, b| {
                        a.timestamp
                            .partial_cmp(&b.timestamp)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    self.request_redraw();
                }
            }

            AppEvent::EpisodesLoaded(episodes) => {
                self.state.episodes = episodes.clone();
                self.request_redraw();
            }

            AppEvent::NyaaResultsLoaded(results) => {
                self.state.searching = false;
                self.state.nyaa_error = None;
                self.state.nyaa_results = results.clone();
                self.request_redraw();
            }

            AppEvent::NyaaSearchError(err) => {
                self.state.searching = false;
                self.state.nyaa_error = Some(err.clone());
                self.request_redraw();
            }

            AppEvent::TorrentStatusChanged(status) => {
                match status {
                    StreamStatus::Connecting { title }
                    | StreamStatus::DownloadingMetadata { title } => {
                        self.state.torrent_status_text =
                            "Fetching torrent file list...".to_string();
                        if self.state.stream_buffering.is_none() {
                            self.set_stream_buffering(Some(BufferingStatus {
                                title: title.clone(),
                                stage: "Fetching torrent metadata...".to_string(),
                                buffered_bytes: 0,
                                total_bytes: 0,
                                speed_bytes: 0,
                                peers: 0,
                            }));
                        }
                    }
                    StreamStatus::SelectingBatch { .. } => {
                        self.state.torrent_status_text.clear();
                    }
                    StreamStatus::Streaming {
                        title,
                        is_complete,
                        progress_pct,
                        ..
                    } => {
                        self.state.is_streaming_torrent = true;
                        self.state.is_torrent_complete = *is_complete;
                        self.state.torrent_status_text =
                            format!("Streaming: {} ({:.1}%)", title, progress_pct);
                        if let Some(ref mut buf) = self.state.stream_buffering {
                            buf.title = title.clone();
                            if buf.speed_bytes > 0 {
                                buf.stage = "Buffering stream...".to_string();
                            }
                        }
                    }
                    StreamStatus::Completed { title, .. } => {
                        self.state.is_streaming_torrent = true;
                        self.state.is_torrent_complete = true;
                        self.state.torrent_status_text = format!("Completed: {}", title);
                    }
                    StreamStatus::Idle => {
                        self.state.is_streaming_torrent = false;
                        self.state.is_torrent_complete = false;
                        self.set_stream_buffering(None);
                    }
                    StreamStatus::Error(e) => {
                        self.state.torrent_status_text = format!("Torrent error: {}", e);
                        self.set_stream_buffering(None);
                    }
                }
                self.request_redraw();
            }

            AppEvent::BatchTorrentResolved {
                torrent_id,
                title,
                mode,
                files,
            } => {
                self.state.torrent_status_text.clear();
                let initial_selection = if *mode == BatchMode::Download {
                    files.iter().map(|f| f.index).collect()
                } else {
                    std::collections::HashSet::new()
                };
                self.state.batch_modal = Some(controls::BatchModalState {
                    torrent_id: *torrent_id,
                    title: title.clone(),
                    mode: *mode,
                    files: files.clone(),
                    selected_indices: initial_selection,
                    scroll_y: 0,
                });
                self.set_video_visible(false);
                self.request_redraw();
            }

            AppEvent::TorrentBatchDiscovered { id, files } => {
                self.state.batch_modal = Some(controls::BatchModalState {
                    torrent_id: *id,
                    title: format!("Torrent #{}", id),
                    mode: BatchMode::Stream,
                    files: files.clone(),
                    selected_indices: std::collections::HashSet::new(),
                    scroll_y: 0,
                });
                self.set_video_visible(false);
                self.request_redraw();
            }

            AppEvent::TorrentDownloadsUpdated(downloads) => {
                self.state.downloads = downloads.clone();
                self.request_redraw();
            }

            AppEvent::DownloadCompleted(path) => {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Download")
                    .to_string();
                self.state.status_message =
                    Some((format!("Download completed: {}", name), Instant::now()));
                self.request_redraw();
            }

            AppEvent::DownloadDirChanged(dir) => {
                self.state.download_dir = dir.clone();
                self.state.status_message = Some((
                    format!("Download folder: {}", dir.display()),
                    Instant::now(),
                ));
                self.request_redraw();
            }

            AppEvent::TrackersSynced { count: _, success, message, fetched } => {
                if *success && !fetched.is_empty() {
                    self.state.cached_remote_trackers = fetched.clone();
                }
                if let Some(ref mut modal) = self.state.trackers_modal {
                    modal.sync_status = Some(message.clone());
                }
                self.state.status_message = Some((message.clone(), Instant::now()));
                self.request_redraw();
            }

            AppEvent::SubtitleExtracted { track_name, path } => {
                self.state.status_message = Some((
                    format!("Extracted: {} to {:?}", track_name, path),
                    Instant::now(),
                ));
                self.request_redraw();
            }

            AppEvent::SubtitleExtractionError(err) => {
                self.state.status_message =
                    Some((format!("Extraction failed: {}", err), Instant::now()));
                self.request_redraw();
            }

            AppEvent::PlaybackRestart => {
                if self.state.stream_buffering.is_some() {
                    self.set_stream_buffering(None);
                }
                if self.state.is_rebuffering {
                    self.set_rebuffering(false, 0);
                }
            }

            AppEvent::CacheBuffering {
                is_buffering,
                percent,
            } => {
                self.set_rebuffering(*is_buffering, *percent);
            }

            AppEvent::StatusMessage(msg) => {
                if msg.starts_with("Playback error") {
                    self.set_stream_buffering(None);
                    self.set_video_visible(false);
                }
                self.state.status_message = Some((msg.clone(), Instant::now()));
                self.request_redraw();
            }

            AppEvent::SubtitleFetched { path, provider } => {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("subtitle");
                self.state.status_message = Some((
                    format!("Loaded subtitle ({provider}): {name}"),
                    Instant::now(),
                ));
                self.request_redraw();
            }

            AppEvent::ExternalSubtitlesDiscovered(results) => {
                self.state.is_searching_subs = false;
                self.state.fetched_subtitles = results.clone();
                if results.is_empty() {
                    self.state.status_message = Some((
                        "No external subtitles found".to_string(),
                        Instant::now(),
                    ));
                } else {
                    self.state.status_message = Some((
                        format!("Found {} external subtitle(s)", results.len()),
                        Instant::now(),
                    ));
                }
                self.request_redraw();
            }

            AppEvent::ExternalSubtitleSearching(searching) => {
                self.state.is_searching_subs = *searching;
                if *searching {
                    self.state.fetched_subtitles.clear();
                }
                self.request_redraw();
            }

            AppEvent::OpenFilePath(_) | AppEvent::OpenFolderPath(_) | AppEvent::FileEnded | AppEvent::MediaKey(_) => {}

            AppEvent::MpvWakeup => {
                self.request_redraw();
            }
        }
    }

    /// Retrieve and remove the next pending action emitted by the UI
    pub fn poll_action(&mut self) -> Option<UiAction> {
        self.action_queue.pop_front()
    }

    /// Drain all pending actions
    pub fn take_actions(&mut self) -> Vec<UiAction> {
        self.action_queue.drain(..).collect()
    }

    /// Convenience method to pop player command directly
    pub fn pop_player_command(&mut self) -> Option<PlayerCommand> {
        let mut idx = None;
        for (i, action) in self.action_queue.iter().enumerate() {
            if let UiAction::Player(_) = action {
                idx = Some(i);
                break;
            }
        }
        if let Some(i) = idx {
            if let Some(UiAction::Player(cmd)) = self.action_queue.remove(i) {
                return Some(cmd);
            }
        }
        None
    }

    /// Render double-buffered GDI surface
    pub fn render(&mut self) {
        unsafe {
            let hdc = GetDC(Some(self.parent_hwnd));
            self.double_buffer
                .ensure_size(hdc, self.width as i32, self.height as i32);

            if let Some(mem_dc) = self.double_buffer.mem_dc {
                render_ui(mem_dc, &self.geom, &self.state, &self.fonts);
                self.double_buffer.present(
                    hdc,
                    0,
                    0,
                    self.width as i32,
                    self.height as i32,
                );
            }

            let _ = ReleaseDC(Some(self.parent_hwnd), hdc);
        }
    }

    /// Invalidate window to schedule repaint
    pub fn request_redraw(&self) {
        unsafe {
            let rect = RECT {
                left: 0,
                top: 0,
                right: self.width as i32,
                bottom: self.height as i32,
            };
            let _ = InvalidateRect(Some(self.parent_hwnd), Some(&rect), false);
            if !self.bar_hwnd.is_invalid() && self.state.bar_visible {
                let _ = InvalidateRect(Some(self.bar_hwnd), None, false);
            }
        }
    }

    /// Update window title according to: "Vesper" or "{clean_title} — Vesper"
    pub fn set_title(&mut self, clean_title: &str) {
        let title = if clean_title.is_empty() || clean_title == "Vesper" {
            "Vesper".to_string()
        } else {
            format!("{} — Vesper", clean_title)
        };

        let mut title_utf16: Vec<u16> = title.encode_utf16().collect();
        title_utf16.push(0);

        unsafe {
            let _ = SetWindowTextW(self.parent_hwnd, PCWSTR::from_raw(title_utf16.as_ptr()));
        }
    }

    fn update_hover(&mut self, px: i32, py: i32) {
        let mut new_hover = None;
        let mut new_tooltip = None;

        // Sidebar subtitle dropdown hover
        if self.state.sidebar_sub_dropdown_open && self.state.active_tab == Tab::Subtitles {
            let dd_x = self.geom.sub_track_selector_rect.x;
            let dd_y = self.geom.sub_track_selector_rect.bottom() + 4;
            let dd_w = self.geom.sub_track_selector_rect.w;
            let item_h = 28;
            let dd_h = ((self.state.tracks.len() as i32) * item_h).min(180);
            let dd_r = Rect::new(dd_x, dd_y, dd_w, dd_h);
            if dd_r.contains(px, py) {
                let mut drop_hover = None;
                for idx in 0..self.state.tracks.len().min(6) {
                    let row_y = dd_y + (idx as i32) * item_h;
                    if py >= row_y && py < row_y + item_h {
                        drop_hover = Some(HoverControl::SubTrackSelectorItem(idx));
                        break;
                    }
                }
                self.state.hovered_control = drop_hover;
                self.state.tooltip = None;
                return;
            }
        }

        // Check modal dialog first if open
        if let Some(ref modal) = self.state.batch_modal {
            let modal_w = 560.min(self.geom.total_width - 40);
            let modal_h = 440.min(self.geom.total_height - 60);
            let modal_x = (self.geom.total_width - modal_w) / 2;
            let modal_y = (self.geom.total_height - modal_h) / 2;

            let close_btn_w = 26;
            let close_btn_h = 24;
            let close_btn_x = modal_x + modal_w - 16 - close_btn_w;
            let close_btn_y = modal_y + 12;

            let sub_y = modal_y + 44;
            let sub_h = 26;
            let list_x = modal_x + 16;
            let list_y = sub_y + sub_h + 6;
            let list_w = modal_w - 32;
            let list_h = modal_h - (list_y - modal_y) - 52;
            let item_h = 36;

            let footer_y = modal_y + modal_h - 44;
            let cancel_btn_w = 70;
            let cancel_btn_x = modal_x + modal_w - 16 - cancel_btn_w;

            if Rect::new(close_btn_x, close_btn_y, close_btn_w, close_btn_h).contains(px, py) {
                new_hover = Some(HoverControl::BatchCancel);
            } else if modal.mode == BatchMode::Download && Rect::new(modal_x + 16, sub_y, 70, 24).contains(px, py) {
                new_hover = Some(HoverControl::BatchSelectAll);
            } else if modal.mode == BatchMode::Download && Rect::new(modal_x + 16 + 76, sub_y, 52, 24).contains(px, py) {
                new_hover = Some(HoverControl::BatchClearAll);
            } else if modal.mode == BatchMode::Download && Rect::new(modal_x + modal_w - 16 - 150, footer_y, 150, 30).contains(px, py) {
                new_hover = Some(HoverControl::BatchConfirm);
            } else if Rect::new(if modal.mode == BatchMode::Download { modal_x + modal_w - 16 - 150 - 10 - cancel_btn_w } else { cancel_btn_x }, footer_y, cancel_btn_w, 30).contains(px, py) {
                new_hover = Some(HoverControl::BatchCancel);
            } else if Rect::new(list_x, list_y, list_w, list_h).contains(px, py) {
                let mut item_y = list_y - modal.scroll_y;
                for (idx, _) in modal.files.iter().enumerate() {
                    let row_r = Rect::new(list_x, item_y, list_w, item_h - 4);
                    if row_r.contains(px, py) {
                        if modal.mode == BatchMode::Stream {
                            let watch_w = 54;
                            let watch_x = row_r.right() - 8 - watch_w;
                            if Rect::new(watch_x, row_r.y, watch_w, row_r.h).contains(px, py) {
                                new_hover = Some(HoverControl::BatchItemWatch(idx));
                            } else {
                                new_hover = Some(HoverControl::BatchFile(idx));
                            }
                        } else {
                            new_hover = Some(HoverControl::BatchFile(idx));
                        }
                        break;
                    }
                    item_y += item_h;
                }
            }

            self.state.hovered_control = new_hover;
            self.state.tooltip = None;
            return;
        }

        // Check trackers configuration modal dialog if open
        if self.state.trackers_modal.is_some() {
            let modal_w = 560.min(self.geom.total_width - 40);
            let modal_h = 440.min(self.geom.total_height - 60);
            let modal_x = (self.geom.total_width - modal_w) / 2;
            let modal_y = (self.geom.total_height - modal_h) / 2;

            let close_btn_w = 26;
            let close_btn_h = 24;
            let close_btn_x = modal_x + modal_w - 16 - close_btn_w;
            let close_btn_y = modal_y + 12;

            let chk_x = modal_x + 18;
            let chk_y = modal_y + 46;
            let chk_w = 16;
            let chk_h = 16;

            let sync_btn_w = 82;
            let sync_btn_h = 26;
            let sync_btn_x = modal_x + modal_w - 18 - sync_btn_w;
            let sync_btn_y = modal_y + 92;

            let url_x = modal_x + 18;
            let url_y = modal_y + 92;
            let url_w = sync_btn_x - url_x - 8;
            let url_h = 26;

            let custom_box_x = modal_x + 18;
            let custom_box_y = modal_y + 164;
            let custom_box_w = modal_w - 36;
            let footer_y = modal_y + modal_h - 44;
            let custom_box_h = footer_y - custom_box_y - 10;

            let reset_w = 110;
            let reset_h = 30;
            let reset_x = modal_x + 18;

            let cancel_w = 70;
            let cancel_h = 30;
            let cancel_x = modal_x + modal_w - 18 - cancel_w;

            let save_w = 80;
            let save_h = 30;
            let save_x = cancel_x - 10 - save_w;

            if Rect::new(close_btn_x, close_btn_y, close_btn_w, close_btn_h).contains(px, py) {
                new_hover = Some(HoverControl::TrackersModalClose);
            } else if Rect::new(chk_x, chk_y, chk_w + 300, chk_h).contains(px, py) {
                new_hover = Some(HoverControl::TrackersModalAutoSyncToggle);
            } else if Rect::new(sync_btn_x, sync_btn_y, sync_btn_w, sync_btn_h).contains(px, py) {
                new_hover = Some(HoverControl::TrackersModalSyncNow);
            } else if Rect::new(url_x, url_y, url_w, url_h).contains(px, py) {
                new_hover = Some(HoverControl::TrackersModalUrlInput);
            } else if Rect::new(custom_box_x, custom_box_y, custom_box_w, custom_box_h).contains(px, py) {
                new_hover = Some(HoverControl::TrackersModalCustomInput);
            } else if Rect::new(reset_x, footer_y, reset_w, reset_h).contains(px, py) {
                new_hover = Some(HoverControl::TrackersModalReset);
            } else if Rect::new(cancel_x, footer_y, cancel_w, cancel_h).contains(px, py) {
                new_hover = Some(HoverControl::TrackersModalClose);
            } else if Rect::new(save_x, footer_y, save_w, save_h).contains(px, py) {
                new_hover = Some(HoverControl::TrackersModalSave);
            }

            self.state.hovered_control = new_hover;
            self.state.tooltip = None;
            return;
        }

        // Check subtitle & integration settings modal dialog if open
        if self.state.sub_settings_modal.is_some() {
            let modal_w = 520.min(self.geom.total_width - 40);
            let modal_h = 420.min(self.geom.total_height - 60);
            let modal_x = (self.geom.total_width - modal_w) / 2;
            let modal_y = (self.geom.total_height - modal_h) / 2;

            let close_btn_w = 26;
            let close_btn_h = 24;
            let close_btn_x = modal_x + modal_w - 16 - close_btn_w;
            let close_btn_y = modal_y + 12;

            let chk_x = modal_x + 18;
            let chk_y = modal_y + 52;
            let chk_w = 16;
            let chk_h = 16;

            let lang_lbl_y = modal_y + 82;
            let lang_box_y = lang_lbl_y + 20;
            let lang_box_w = 120;
            let lang_box_h = 26;

            let open_lbl_y = lang_box_y + 34;
            let open_box_y = open_lbl_y + 20;
            let full_box_w = modal_w - 36;
            let open_box_h = 26;

            let jimaku_lbl_y = open_box_y + 34;
            let jimaku_box_y = jimaku_lbl_y + 20;

            let assoc_btn_y = jimaku_box_y + 36;
            let assoc_btn_h = 28;

            let footer_y = modal_y + modal_h - 44;
            let save_w = 110;
            let save_h = 30;
            let save_x = modal_x + modal_w - 18 - save_w;

            if Rect::new(close_btn_x, close_btn_y, close_btn_w, close_btn_h).contains(px, py) {
                new_hover = Some(HoverControl::SubModalClose);
            } else if Rect::new(chk_x, chk_y, chk_w + 350, chk_h).contains(px, py) {
                new_hover = Some(HoverControl::SubModalAutoFetchToggle);
            } else if Rect::new(chk_x, lang_box_y, lang_box_w, lang_box_h).contains(px, py) {
                new_hover = Some(HoverControl::SubModalLangInput);
            } else if Rect::new(chk_x, open_box_y, full_box_w, open_box_h).contains(px, py) {
                new_hover = Some(HoverControl::SubModalOpenSubKeyInput);
            } else if Rect::new(chk_x, jimaku_box_y, full_box_w, open_box_h).contains(px, py) {
                new_hover = Some(HoverControl::SubModalJimakuKeyInput);
            } else if Rect::new(chk_x, assoc_btn_y, full_box_w, assoc_btn_h).contains(px, py) {
                new_hover = Some(HoverControl::SubModalRegisterAssoc);
            } else if Rect::new(save_x, footer_y, save_w, save_h).contains(px, py) {
                new_hover = Some(HoverControl::SubModalSave);
            }

            self.state.hovered_control = new_hover;
            self.state.tooltip = None;
            return;
        }

        // Check category dropdown items if open
        if self.state.category_dropdown_open {
            let dd = &self.geom.nyaa_category_dropdown;
            let menu_y = dd.bottom() + 4;
            for i in 0..3 {
                let item_r = Rect::new(dd.x, menu_y + (i as i32) * 24, dd.w, 24);
                if item_r.contains(px, py) {
                    new_hover = Some(HoverControl::NyaaCategoryItem(i));
                    break;
                }
            }
            if new_hover.is_some() {
                self.state.hovered_control = new_hover;
                return;
            }
        }

        // Empty state viewport buttons (when idle)
        if self.state.current_path.is_none() && self.state.duration <= 0.0 {
            if self.geom.empty_open_file.contains(px, py) {
                new_hover = Some(HoverControl::EmptyStateOpenFile);
            } else if self.geom.empty_open_folder.contains(px, py) {
                new_hover = Some(HoverControl::EmptyStateOpenFolder);
            }
        }

        // Bottom playback bar controls
        if self.geom.btn_open.contains(px, py) {
            new_hover = Some(HoverControl::BottomBarOpen);
            new_tooltip = Some((
                "Open File (Ctrl+O) / Folder (Ctrl+Shift+O)".to_string(),
                px,
                self.geom.btn_open.y,
            ));
        } else if self.geom.btn_play_pause.contains(px, py) {
            new_hover = Some(HoverControl::PlayPause);
        } else if self.geom.btn_next.contains(px, py) {
            new_hover = Some(HoverControl::Next);
        } else if self.geom.seekbar_rect.contains(px, py) {
            new_hover = Some(HoverControl::Seekbar);

            // Compute seekbar hover tooltip & chapter detection
            if self.geom.seekbar_rect.w > 0 && self.state.duration > 0.0 {
                let pct = ((px - self.geom.seekbar_rect.x) as f64
                    / self.geom.seekbar_rect.w as f64)
                    .clamp(0.0, 1.0);
                let hover_time = pct * self.state.duration;

                let mut chapter_match = None;
                for ch in &self.state.chapters {
                    let ch_x = self.geom.seekbar_rect.x
                        + ((ch.time / self.state.duration) * (self.geom.seekbar_rect.w as f64))
                            as i32;
                    if (px - ch_x).abs() <= 6 {
                        chapter_match = Some(format!("{}: {}", ch.title, format_time(ch.time)));
                        break;
                    }
                }

                let text = chapter_match.unwrap_or_else(|| format_time(hover_time));
                new_tooltip = Some((text, px, self.geom.seekbar_rect.y));
            }
        } else if self.geom.btn_vol_icon.contains(px, py) {
            new_hover = Some(HoverControl::VolumeIcon);
        } else if self.geom.vol_slider_rect.contains(px, py) {
            new_hover = Some(HoverControl::VolumeSlider);
        } else if self.geom.btn_audio_toggle.contains(px, py) {
            new_hover = Some(HoverControl::AudioToggle);
            new_tooltip = Some(("Audio Tracks (A)".to_string(), px, self.geom.btn_audio_toggle.y));
        } else if self.geom.btn_sub_toggle.contains(px, py) {
            new_hover = Some(HoverControl::SubToggle);
            new_tooltip = Some(("Subtitle Tracks (S)".to_string(), px, self.geom.btn_sub_toggle.y));
        } else if self.geom.btn_fullscreen.contains(px, py) {
            new_hover = Some(HoverControl::Fullscreen);
        } else if self.geom.btn_drawer_toggle.contains(px, py) {
            new_hover = Some(HoverControl::DrawerToggle);
        } else if self.geom.drawer_rect.contains(px, py) {
            // Check drawer widgets
            if self.geom.tab_subtitles.contains(px, py) {
                new_hover = Some(HoverControl::Tab(Tab::Subtitles));
            } else if self.geom.tab_episodes.contains(px, py) {
                new_hover = Some(HoverControl::Tab(Tab::Episodes));
            } else if self.geom.tab_nyaa.contains(px, py) {
                new_hover = Some(HoverControl::Tab(Tab::Nyaa));
            } else {
                match self.state.active_tab {
                    Tab::Subtitles => {
                        if self.geom.sub_track_selector_rect.contains(px, py) {
                            new_hover = Some(HoverControl::SubTrackSelector);
                        } else if self.geom.btn_fetch_subs.contains(px, py) {
                            new_hover = Some(HoverControl::FetchSubs);
                        } else if self.geom.btn_sub_settings.contains(px, py) {
                            new_hover = Some(HoverControl::SubSettings);
                        } else if self.geom.btn_extract_ass.contains(px, py) {
                            new_hover = Some(HoverControl::ExtractAss);
                            if self.state.is_streaming_torrent && !self.state.is_torrent_complete {
                                new_tooltip = Some((
                                    "Cannot extract while streaming unfinished torrent".to_string(),
                                    px,
                                    self.geom.btn_extract_ass.y,
                                ));
                            }
                        } else if self.geom.subtitle_list_rect.contains(px, py) {
                            let mut y =
                                self.geom.subtitle_list_rect.y + 8 - self.state.sub_scroll_y;
                            let list_r = &self.geom.subtitle_list_rect;

                            if self.state.is_searching_subs {
                                y += 42;
                            }

                            if !self.state.fetched_subtitles.is_empty() {
                                y += 24;
                                let card_h = 52;
                                let card_margin = 6;
                                for (idx, _) in self.state.fetched_subtitles.iter().enumerate() {
                                    let card_r = Rect::new(list_r.x + 12, y, list_r.w - 24, card_h);
                                    if card_r.contains(px, py) {
                                        new_hover = Some(HoverControl::AttachFetchedSub(idx));
                                        break;
                                    }
                                    y += card_h + card_margin;
                                }

                                if new_hover.is_none() && !self.state.cues.is_empty() {
                                    y += 32;
                                }
                            }

                            if new_hover.is_none() {
                                let card_h = 70;
                                let card_margin = 8;
                                for (idx, _) in self.state.cues.iter().enumerate() {
                                    let card_r = Rect::new(
                                        self.geom.subtitle_list_rect.x + 12,
                                        y,
                                        self.geom.subtitle_list_rect.w - 24,
                                        card_h,
                                    );
                                    if card_r.contains(px, py) {
                                        let copy_r =
                                            Rect::new(card_r.right() - 56, card_r.y + 8, 48, 20);
                                        if copy_r.contains(px, py) {
                                            new_hover = Some(HoverControl::CueCopy(idx));
                                        } else {
                                            new_hover = Some(HoverControl::CueCard(idx));
                                        }
                                        break;
                                    }
                                    y += card_h + card_margin;
                                }
                            }
                        }
                    }

                    Tab::Episodes => {
                        if self.geom.ep_header_open_folder.contains(px, py)
                            || (self.state.episodes.is_empty() && self.geom.ep_empty_open_folder.contains(px, py))
                        {
                            new_hover = Some(HoverControl::EpisodesOpenFolder);
                        } else if self.geom.episode_list_rect.contains(px, py) {
                            let item_h = 56;
                            let item_margin = 6;
                            let mut y = self.geom.episode_list_rect.y + 8 - self.state.ep_scroll_y;
                            for (idx, _) in self.state.episodes.iter().enumerate() {
                                let row_r = Rect::new(
                                    self.geom.episode_list_rect.x + 12,
                                    y,
                                    self.geom.episode_list_rect.w - 24,
                                    item_h,
                                );
                                if row_r.contains(px, py) {
                                    new_hover = Some(HoverControl::Episode(idx));
                                    break;
                                }
                                y += item_h + item_margin;
                            }
                        }
                    }

                    Tab::Nyaa => {
                        if self.geom.nyaa_search_input.contains(px, py) {
                            new_hover = Some(HoverControl::NyaaSearchInput);
                        } else if self.geom.nyaa_search_btn.contains(px, py) {
                            new_hover = Some(HoverControl::NyaaSearchBtn);
                        } else if self.geom.nyaa_trusted_checkbox.contains(px, py) {
                            new_hover = Some(HoverControl::NyaaTrustedCheckbox);
                        } else if self.geom.nyaa_category_dropdown.contains(px, py) {
                            new_hover = Some(HoverControl::NyaaCategoryDropdown);
                        } else if self.geom.stream_download_dir_btn.contains(px, py) {
                            new_hover = Some(HoverControl::StreamFolderPicker);
                            new_tooltip = Some((
                                format!("Download folder: {}", self.state.download_dir.display()),
                                px,
                                self.geom.stream_download_dir_btn.bottom() + 4,
                            ));
                        } else if self.geom.stream_trackers_btn.contains(px, py) {
                            new_hover = Some(HoverControl::StreamTrackersBtn);
                            new_tooltip = Some((
                                "Configure BitTorrent trackers".to_string(),
                                px,
                                self.geom.stream_trackers_btn.bottom() + 4,
                            ));
                        } else if self.geom.nyaa_results_list.contains(px, py) {
                            let mut y =
                                self.geom.nyaa_results_list.y + 4 - self.state.nyaa_scroll_y;

                            // Downloads section hit test (Active then Completed)
                            let active: Vec<(usize, &crate::types::TorrentDownloadItem)> = self
                                .state
                                .downloads
                                .iter()
                                .enumerate()
                                .filter(|(_, dl)| !matches!(dl.state, crate::types::TorrentTaskState::Completed))
                                .collect();
                            let completed: Vec<(usize, &crate::types::TorrentDownloadItem)> = self
                                .state
                                .downloads
                                .iter()
                                .enumerate()
                                .filter(|(_, dl)| matches!(dl.state, crate::types::TorrentTaskState::Completed))
                                .collect();

                            // Section 1: Active downloads hit test
                            if !active.is_empty() {
                                y += 22;
                                for &(idx, _dl) in &active {
                                    let card_h = 68;
                                    let card_r = Rect::new(
                                        self.geom.nyaa_results_list.x + 12,
                                        y,
                                        self.geom.nyaa_results_list.w - 24,
                                        card_h,
                                    );
                                    if new_hover.is_none() && card_r.contains(px, py) {
                                        let btn_y = card_r.y + 38;
                                        let btn_h = 22;
                                        let rem_x = card_r.x + card_r.w - 10 - 24;
                                        let fol_x = rem_x - 5 - 46;
                                        let pause_x = fol_x - 5 - 46;

                                        if Rect::new(rem_x, btn_y, 24, btn_h).contains(px, py) {
                                            new_hover = Some(HoverControl::DownloadRemove(idx));
                                        } else if Rect::new(fol_x, btn_y, 46, btn_h).contains(px, py) {
                                            new_hover = Some(HoverControl::DownloadFolder(idx));
                                        } else if Rect::new(pause_x, btn_y, 46, btn_h).contains(px, py) {
                                            new_hover = Some(HoverControl::DownloadPauseResume(idx));
                                        } else {
                                            new_hover = Some(HoverControl::DownloadCard(idx));
                                        }
                                    }
                                    y += card_h + 6;
                                }
                                y += 6;
                            }

                            // Section 2: Completed downloads hit test
                            if !completed.is_empty() {
                                y += 22;
                                for &(idx, dl) in &completed {
                                    let card_h = 68;
                                    let card_r = Rect::new(
                                        self.geom.nyaa_results_list.x + 12,
                                        y,
                                        self.geom.nyaa_results_list.w - 24,
                                        card_h,
                                    );
                                    if new_hover.is_none() && card_r.contains(px, py) {
                                        let btn_y = card_r.y + 38;
                                        let btn_h = 22;
                                        let is_batch = dl.batch_info.map(|(_, total)| total > 1).unwrap_or(false);
                                        let is_confirming = self
                                            .state
                                            .confirming_delete
                                            .map(|(id, t)| id == dl.id && t.elapsed() < Duration::from_secs(3))
                                            .unwrap_or(false);
                                        let del_w = if is_confirming { 62 } else { 46 };
                                        let del_x = card_r.x + card_r.w - 10 - del_w;
                                        let fol_x = del_x - 5 - 46;
                                        let files_w = 40;
                                        let files_x = fol_x - 5 - files_w;
                                        let play_w = 40;
                                        let play_x = (if is_batch { files_x } else { fol_x }) - 5 - play_w;

                                        if Rect::new(del_x, btn_y, del_w, btn_h).contains(px, py) {
                                            new_hover = Some(HoverControl::DownloadDelete(idx));
                                        } else if Rect::new(fol_x, btn_y, 46, btn_h).contains(px, py) {
                                            new_hover = Some(HoverControl::DownloadFolder(idx));
                                        } else if is_batch && Rect::new(files_x, btn_y, files_w, btn_h).contains(px, py) {
                                            new_hover = Some(HoverControl::DownloadEpisodes(idx));
                                        } else if Rect::new(play_x, btn_y, play_w, btn_h).contains(px, py) {
                                            new_hover = Some(HoverControl::DownloadPlay(idx));
                                        } else {
                                            new_hover = Some(HoverControl::DownloadCard(idx));
                                        }
                                    }
                                    y += card_h + 6;
                                }
                                y += 6;
                            }

                            // Search results section hit test
                            if new_hover.is_none() && !self.state.nyaa_results.is_empty() {
                                if !self.state.downloads.is_empty() {
                                    y += 22;
                                }
                                let item_h = 72;
                                let item_margin = 6;
                                for (idx, item) in self.state.nyaa_results.iter().enumerate() {
                                    let row_r = Rect::new(
                                        self.geom.nyaa_results_list.x + 12,
                                        y,
                                        self.geom.nyaa_results_list.w - 24,
                                        item_h,
                                    );
                                    if row_r.contains(px, py) {
                                        let btn_y = row_r.y + 38;
                                        let btn_h = 22;

                                        if cfg!(feature = "torrent") {
                                            let copy_x = row_r.x + row_r.w - 10 - 40;
                                            let dl_x = copy_x - 6 - 58;

                                            let is_dl = item.info_hash.as_ref().map(|h| self.state.completed_hashes.contains(&h.to_ascii_lowercase())).unwrap_or(false);
                                            let has_prog = is_dl && item.info_hash.as_ref().and_then(|h| self.state.watch_progress_keys.get(&h.to_ascii_lowercase())).map(|p| *p >= 10.0).unwrap_or(false);
                                            let watch_w = if is_dl { if has_prog { 56 } else { 46 } } else { 50 };
                                            let stream_x = dl_x - 6 - watch_w;

                                            if Rect::new(copy_x, btn_y, 40, btn_h).contains(px, py) {
                                                new_hover = Some(HoverControl::NyaaResultCopyMagnet(idx));
                                            } else if Rect::new(dl_x, btn_y, 58, btn_h).contains(px, py) {
                                                new_hover = Some(HoverControl::NyaaResultDownload(idx));
                                            } else if Rect::new(stream_x, btn_y, watch_w, btn_h).contains(px, py) {
                                                new_hover = Some(HoverControl::NyaaResultStream(idx));
                                            } else {
                                                new_hover = Some(HoverControl::NyaaResult(idx));
                                            }
                                        } else {
                                            let copy_x = row_r.x + row_r.w - 10 - 40;
                                            let open_x = copy_x - 6 - 76;

                                            if Rect::new(copy_x, btn_y, 40, btn_h).contains(px, py) {
                                                new_hover = Some(HoverControl::NyaaResultCopyMagnet(idx));
                                            } else if Rect::new(open_x, btn_y, 76, btn_h).contains(px, py) {
                                                new_hover = Some(HoverControl::NyaaResultOpenMagnet(idx));
                                            } else {
                                                new_hover = Some(HoverControl::NyaaResult(idx));
                                            }
                                        }
                                        break;
                                    }
                                    y += item_h + item_margin;
                                }
                            }
                        }
                    }
                }
            }
        }

        self.state.hovered_control = new_hover;
        self.state.tooltip = new_tooltip;
    }

    fn handle_mouse_down(&mut self, px: i32, py: i32) -> bool {
        // Modal dialog handling
        if let Some(ref modal) = self.state.batch_modal {
            let modal_w = 560.min(self.geom.total_width - 40);
            let modal_h = 440.min(self.geom.total_height - 60);
            let modal_x = (self.geom.total_width - modal_w) / 2;
            let modal_y = (self.geom.total_height - modal_h) / 2;

            let close_btn_w = 26;
            let close_btn_h = 24;
            let close_btn_x = modal_x + modal_w - 16 - close_btn_w;
            let close_btn_y = modal_y + 12;

            let sub_y = modal_y + 44;
            let sub_h = 26;
            let list_x = modal_x + 16;
            let list_y = sub_y + sub_h + 6;
            let list_w = modal_w - 32;
            let list_h = modal_h - (list_y - modal_y) - 52;
            let item_h = 36;

            let footer_y = modal_y + modal_h - 44;
            let cancel_btn_w = 70;
            let cancel_btn_x = modal_x + modal_w - 16 - cancel_btn_w;

            // Close button (×)
            if Rect::new(close_btn_x, close_btn_y, close_btn_w, close_btn_h).contains(px, py) {
                self.state.batch_modal = None;
                if self.state.duration > 0.0 {
                    self.set_video_visible(true);
                }
                self.request_redraw();
                return true;
            }

            // In Download Mode: Select All / Clear buttons
            if modal.mode == BatchMode::Download {
                if Rect::new(modal_x + 16, sub_y, 70, 24).contains(px, py) {
                    if let Some(ref mut m) = self.state.batch_modal {
                        m.selected_indices = m.files.iter().map(|f| f.index).collect();
                    }
                    self.request_redraw();
                    return true;
                } else if Rect::new(modal_x + 16 + 76, sub_y, 52, 24).contains(px, py) {
                    if let Some(ref mut m) = self.state.batch_modal {
                        m.selected_indices.clear();
                    }
                    self.request_redraw();
                    return true;
                }
            }

            // Footer Cancel button
            let cancel_rect = if modal.mode == BatchMode::Download {
                Rect::new(modal_x + modal_w - 16 - 150 - 10 - cancel_btn_w, footer_y, cancel_btn_w, 30)
            } else {
                Rect::new(cancel_btn_x, footer_y, cancel_btn_w, 30)
            };
            if cancel_rect.contains(px, py) {
                self.state.batch_modal = None;
                if self.state.duration > 0.0 {
                    self.set_video_visible(true);
                }
                self.request_redraw();
                return true;
            }

            // Footer Download Selected (N) button
            if modal.mode == BatchMode::Download {
                let dl_btn_rect = Rect::new(modal_x + modal_w - 16 - 150, footer_y, 150, 30);
                if dl_btn_rect.contains(px, py) {
                    if let Some(m) = self.state.batch_modal.take() {
                        let indices: Vec<usize> = m.selected_indices.into_iter().collect();
                        if !indices.is_empty() {
                            self.action_queue.push_back(UiAction::StartBatchDownload {
                                id: m.torrent_id,
                                file_indices: indices,
                            });
                        }
                    }
                    if self.state.duration > 0.0 {
                        self.set_video_visible(true);
                    }
                    self.request_redraw();
                    return true;
                }
            }

            // File items
            if Rect::new(list_x, list_y, list_w, list_h).contains(px, py) {
                let mut item_y = list_y - modal.scroll_y;
                for file in modal.files.iter() {
                    let row_r = Rect::new(list_x, item_y, list_w, item_h - 4);
                    if row_r.contains(px, py) {
                        if modal.mode == BatchMode::Stream {
                            let torrent_id = modal.torrent_id;
                            let file_idx = file.index;
                            let file_title = file.path.clone();
                            self.state.batch_modal = None;
                            self.set_stream_buffering(Some(BufferingStatus {
                                title: file_title,
                                stage: "Buffering stream...".to_string(),
                                buffered_bytes: 0,
                                total_bytes: 0,
                                speed_bytes: 0,
                                peers: 0,
                            }));
                            self.action_queue.push_back(UiAction::SelectBatchTorrentFile {
                                id: torrent_id,
                                file_idx,
                            });
                            self.request_redraw();
                            return true;
                        } else {
                            let file_idx = file.index;
                            if let Some(ref mut m) = self.state.batch_modal {
                                if m.selected_indices.contains(&file_idx) {
                                    m.selected_indices.remove(&file_idx);
                                } else {
                                    m.selected_indices.insert(file_idx);
                                }
                            }
                            self.request_redraw();
                            return true;
                        }
                    }
                    item_y += item_h;
                }
            }

            return true;
        }

        // Trackers configuration modal click handling
        if let Some(ref modal) = self.state.trackers_modal {
            let modal_w = 560.min(self.geom.total_width - 40);
            let modal_h = 440.min(self.geom.total_height - 60);
            let modal_x = (self.geom.total_width - modal_w) / 2;
            let modal_y = (self.geom.total_height - modal_h) / 2;

            let close_btn_w = 26;
            let close_btn_h = 24;
            let close_btn_x = modal_x + modal_w - 16 - close_btn_w;
            let close_btn_y = modal_y + 12;

            let chk_x = modal_x + 18;
            let chk_y = modal_y + 46;
            let chk_w = 16;
            let chk_h = 16;

            let sync_btn_w = 82;
            let sync_btn_h = 26;
            let sync_btn_x = modal_x + modal_w - 18 - sync_btn_w;
            let sync_btn_y = modal_y + 92;

            let url_x = modal_x + 18;
            let url_y = modal_y + 92;
            let url_w = sync_btn_x - url_x - 8;
            let url_h = 26;

            let custom_box_x = modal_x + 18;
            let custom_box_y = modal_y + 164;
            let custom_box_w = modal_w - 36;
            let footer_y = modal_y + modal_h - 44;
            let custom_box_h = footer_y - custom_box_y - 10;

            let reset_w = 110;
            let reset_h = 30;
            let reset_x = modal_x + 18;

            let cancel_w = 70;
            let cancel_h = 30;
            let cancel_x = modal_x + modal_w - 18 - cancel_w;

            let save_w = 80;
            let save_h = 30;
            let save_x = cancel_x - 10 - save_w;

            // Close (×) or Cancel
            if Rect::new(close_btn_x, close_btn_y, close_btn_w, close_btn_h).contains(px, py)
                || Rect::new(cancel_x, footer_y, cancel_w, cancel_h).contains(px, py)
            {
                self.state.trackers_modal = None;
                if self.state.duration > 0.0 {
                    self.set_video_visible(true);
                }
                self.request_redraw();
                return true;
            }

            // Auto-sync checkbox
            if Rect::new(chk_x, chk_y, chk_w + 300, chk_h).contains(px, py) {
                if let Some(ref mut m) = self.state.trackers_modal {
                    m.auto_sync = !m.auto_sync;
                }
                self.request_redraw();
                return true;
            }

            // Sync Now button
            if Rect::new(sync_btn_x, sync_btn_y, sync_btn_w, sync_btn_h).contains(px, py) {
                let url = modal.tracker_url.clone();
                if let Some(ref mut m) = self.state.trackers_modal {
                    m.sync_status = Some("Syncing trackers...".to_string());
                }
                self.action_queue.push_back(UiAction::SyncTrackersNow(url));
                self.request_redraw();
                return true;
            }

            // URL input box focus
            if Rect::new(url_x, url_y, url_w, url_h).contains(px, py) {
                if let Some(ref mut m) = self.state.trackers_modal {
                    m.focused_field = controls::TrackersFocusedField::Url;
                }
                self.request_redraw();
                return true;
            }

            // Custom trackers box focus
            if Rect::new(custom_box_x, custom_box_y, custom_box_w, custom_box_h).contains(px, py) {
                if let Some(ref mut m) = self.state.trackers_modal {
                    m.focused_field = controls::TrackersFocusedField::Custom;
                }
                self.request_redraw();
                return true;
            }

            // Reset Defaults button
            if Rect::new(reset_x, footer_y, reset_w, reset_h).contains(px, py) {
                if let Some(ref mut m) = self.state.trackers_modal {
                    m.auto_sync = true;
                    m.tracker_url = crate::config::default_tracker_list_url();
                    m.sync_status = Some("Defaults restored".to_string());
                }
                self.action_queue.push_back(UiAction::ResetTrackerDefaults);
                self.request_redraw();
                return true;
            }

            // Save button
            if Rect::new(save_x, footer_y, save_w, save_h).contains(px, py) {
                if let Some(m) = self.state.trackers_modal.take() {
                    let custom: Vec<String> = m
                        .custom_text
                        .lines()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty() && (s.starts_with("udp://") || s.starts_with("http://") || s.starts_with("https://") || s.starts_with("wss://")))
                        .collect();
                    self.action_queue.push_back(UiAction::SaveTrackers {
                        auto_sync: m.auto_sync,
                        url: m.tracker_url,
                        custom_trackers: custom,
                    });
                }
                if self.state.duration > 0.0 {
                    self.set_video_visible(true);
                }
                self.request_redraw();
                return true;
            }

            // Clicking elsewhere in modal clears focus
            if let Some(ref mut m) = self.state.trackers_modal {
                m.focused_field = controls::TrackersFocusedField::None;
            }
            self.request_redraw();
            return true;
        }

        // Subtitle & Integration settings modal click handling
        if self.state.sub_settings_modal.is_some() {
            let modal_w = 520.min(self.geom.total_width - 40);
            let modal_h = 420.min(self.geom.total_height - 60);
            let modal_x = (self.geom.total_width - modal_w) / 2;
            let modal_y = (self.geom.total_height - modal_h) / 2;

            let close_btn_w = 26;
            let close_btn_h = 24;
            let close_btn_x = modal_x + modal_w - 16 - close_btn_w;
            let close_btn_y = modal_y + 12;

            let chk_x = modal_x + 18;
            let chk_y = modal_y + 52;
            let chk_w = 16;
            let chk_h = 16;

            let lang_lbl_y = modal_y + 82;
            let lang_box_y = lang_lbl_y + 20;
            let lang_box_w = 120;
            let lang_box_h = 26;

            let open_lbl_y = lang_box_y + 34;
            let open_box_y = open_lbl_y + 20;
            let full_box_w = modal_w - 36;
            let open_box_h = 26;

            let jimaku_lbl_y = open_box_y + 34;
            let jimaku_box_y = jimaku_lbl_y + 20;

            let assoc_btn_y = jimaku_box_y + 36;
            let assoc_btn_h = 28;

            let footer_y = modal_y + modal_h - 44;
            let save_w = 110;
            let save_h = 30;
            let save_x = modal_x + modal_w - 18 - save_w;

            // Close
            if Rect::new(close_btn_x, close_btn_y, close_btn_w, close_btn_h).contains(px, py) {
                self.state.sub_settings_modal = None;
                if self.state.duration > 0.0 {
                    self.set_video_visible(true);
                }
                self.request_redraw();
                return true;
            }

            // Checkbox
            if Rect::new(chk_x, chk_y, chk_w + 350, chk_h).contains(px, py) {
                if let Some(ref mut m) = self.state.sub_settings_modal {
                    m.auto_fetch = !m.auto_fetch;
                }
                self.request_redraw();
                return true;
            }

            // Language focus
            if Rect::new(chk_x, lang_box_y, lang_box_w, lang_box_h).contains(px, py) {
                if let Some(ref mut m) = self.state.sub_settings_modal {
                    m.focused_field = controls::SubModalFocusedField::Lang;
                }
                self.request_redraw();
                return true;
            }

            // OpenSubtitles key focus
            if Rect::new(chk_x, open_box_y, full_box_w, open_box_h).contains(px, py) {
                if let Some(ref mut m) = self.state.sub_settings_modal {
                    m.focused_field = controls::SubModalFocusedField::OpenSubKey;
                }
                self.request_redraw();
                return true;
            }

            // Jimaku key focus
            if Rect::new(chk_x, jimaku_box_y, full_box_w, open_box_h).contains(px, py) {
                if let Some(ref mut m) = self.state.sub_settings_modal {
                    m.focused_field = controls::SubModalFocusedField::JimakuKey;
                }
                self.request_redraw();
                return true;
            }

            // Register File Associations
            if Rect::new(chk_x, assoc_btn_y, full_box_w, assoc_btn_h).contains(px, py) {
                self.action_queue.push_back(UiAction::RegisterFileAssociations);
                self.request_redraw();
                return true;
            }

            // Save button
            if Rect::new(save_x, footer_y, save_w, save_h).contains(px, py) {
                if let Some(m) = self.state.sub_settings_modal.take() {
                    let open_key = if m.opensubtitles_key.trim().is_empty() { None } else { Some(m.opensubtitles_key.trim().to_string()) };
                    let jim_key = if m.jimaku_key.trim().is_empty() { None } else { Some(m.jimaku_key.trim().to_string()) };
                    let lang = if m.preferred_lang.trim().is_empty() { "en".to_string() } else { m.preferred_lang.trim().to_string() };

                    self.state.auto_fetch_subs = m.auto_fetch;
                    self.state.preferred_sub_lang = lang.clone();
                    self.state.opensubtitles_api_key = open_key.clone();
                    self.state.jimaku_api_key = jim_key.clone();

                    self.action_queue.push_back(UiAction::SaveSubtitleConfig {
                        auto_fetch: m.auto_fetch,
                        preferred_lang: lang,
                        opensubtitles_key: open_key,
                        jimaku_key: jim_key,
                    });
                }
                if self.state.duration > 0.0 {
                    self.set_video_visible(true);
                }
                self.request_redraw();
                return true;
            }

            // Clicking elsewhere clears focus
            if let Some(ref mut m) = self.state.sub_settings_modal {
                m.focused_field = controls::SubModalFocusedField::None;
            }
            self.request_redraw();
            return true;
        }

        // Category dropdown menu selection
        if self.state.category_dropdown_open {
            let dd = &self.geom.nyaa_category_dropdown;
            let menu_y = dd.bottom() + 4;
            for i in 0..3 {
                let item_r = Rect::new(dd.x, menu_y + (i as i32) * 24, dd.w, 24);
                if item_r.contains(px, py) {
                    self.state.category = match i {
                        0 => NyaaCategory::AnimeEng,
                        1 => NyaaCategory::AllAnime,
                        _ => NyaaCategory::AnimeRaw,
                    };
                    self.state.category_dropdown_open = false;
                    self.request_redraw();
                    return true;
                }
            }
            self.state.category_dropdown_open = false;
            self.request_redraw();
            return true;
        }

        // Sidebar subtitle track dropdown selection
        if self.state.sidebar_sub_dropdown_open {
            let dd_x = self.geom.sub_track_selector_rect.x;
            let dd_y = self.geom.sub_track_selector_rect.bottom() + 4;
            let dd_w = self.geom.sub_track_selector_rect.w;
            let item_h = 28;
            let dd_h = ((self.state.tracks.len() as i32) * item_h).min(180);
            let dd_r = Rect::new(dd_x, dd_y, dd_w, dd_h);
            if dd_r.contains(px, py) {
                let clicked_idx = ((py - dd_y) / item_h) as usize;
                if let Some(target_tr) = self.state.tracks.get(clicked_idx) {
                    self.state.selected_extract_sub_id = Some(target_tr.id);
                    self.state.sidebar_sub_dropdown_open = false;
                    self.request_redraw();
                    return true;
                }
                return true;
            } else if !self.geom.sub_track_selector_rect.contains(px, py) {
                self.state.sidebar_sub_dropdown_open = false;
                self.request_redraw();
            }
        }

        // Viewport empty state buttons (when idle)
        if self.state.current_path.is_none() && self.state.duration <= 0.0 {
            if self.geom.empty_open_file.contains(px, py) {
                self.action_queue.push_back(UiAction::OpenFilePicker);
                return true;
            } else if self.geom.empty_open_folder.contains(px, py) {
                self.action_queue.push_back(UiAction::OpenFolderPicker);
                return true;
            }
        }

        // Open File button on bottom bar
        if self.geom.btn_open.contains(px, py) {
            self.action_queue.push_back(UiAction::OpenFilePicker);
            return true;
        }

        // Play / Pause button
        if self.geom.btn_play_pause.contains(px, py) {
            self.action_queue
                .push_back(UiAction::Player(PlayerCommand::TogglePause));
            return true;
        }

        // Next Episode button
        if self.geom.btn_next.contains(px, py) {
            self.play_next_episode();
            return true;
        }

        // Seekbar click & drag
        if self.geom.seekbar_rect.contains(px, py) {
            self.state.is_dragging_seekbar = true;
            if self.geom.seekbar_rect.w > 0 && self.state.duration > 0.0 {
                let pct = ((px - self.geom.seekbar_rect.x) as f64
                    / self.geom.seekbar_rect.w as f64)
                    .clamp(0.0, 1.0);
                let target_time = pct * self.state.duration;
                self.state.time_pos = target_time;
                self.action_queue
                    .push_back(UiAction::Player(PlayerCommand::SeekAbsolute(target_time)));
            }
            self.request_redraw();
            return true;
        }

        // Volume icon mute toggle
        if self.geom.btn_vol_icon.contains(px, py) {
            if self.state.muted || self.state.volume <= 0.0 {
                self.state.volume = if self.state.unmuted_volume > 0.0 {
                    self.state.unmuted_volume
                } else {
                    100.0
                };
                self.state.muted = false;
            } else {
                self.state.unmuted_volume = self.state.volume;
                self.state.volume = 0.0;
                self.state.muted = true;
            }
            self.action_queue
                .push_back(UiAction::Player(PlayerCommand::SetVolume(self.state.volume)));
            self.request_redraw();
            return true;
        }

        // Volume slider click & drag
        if self.geom.vol_slider_rect.contains(px, py) {
            self.state.is_dragging_volume = true;
            let pct = ((px - self.geom.vol_slider_rect.x) as f64
                / self.geom.vol_slider_rect.w as f64)
                .clamp(0.0, 1.0);
            self.state.volume = pct * 100.0;
            self.state.muted = false;
            self.action_queue
                .push_back(UiAction::Player(PlayerCommand::SetVolume(self.state.volume)));
            self.request_redraw();
            return true;
        }

        // Audio track toggle button (native Win32 popup menu immune to video child HWND clipping)
        if self.geom.btn_audio_toggle.contains(px, py) {
            self.show_audio_track_menu();
            return true;
        }

        // Subtitle track toggle button (native Win32 popup menu immune to video child HWND clipping)
        if self.geom.btn_sub_toggle.contains(px, py) {
            self.show_sub_track_menu();
            return true;
        }

        // Fullscreen toggle button
        if self.geom.btn_fullscreen.contains(px, py) {
            self.state.is_fullscreen = !self.state.is_fullscreen;
            self.action_queue.push_back(UiAction::ToggleFullscreen);
            return true;
        }

        // Drawer collapse toggle button (< / >)
        if self.geom.btn_drawer_toggle.contains(px, py) {
            self.state.drawer_open = !self.state.drawer_open;
            self.update_layout();
            self.request_redraw();
            return true;
        }

        // Drawer Tabs
        if self.geom.tab_subtitles.contains(px, py) {
            self.state.active_tab = Tab::Subtitles;
            self.request_redraw();
            return true;
        }
        if self.geom.tab_episodes.contains(px, py) {
            self.state.active_tab = Tab::Episodes;
            self.request_redraw();
            return true;
        }
        if self.geom.tab_nyaa.contains(px, py) {
            self.state.active_tab = Tab::Nyaa;
            self.request_redraw();
            return true;
        }

        // Drawer Content clicks
        if self.geom.drawer_rect.contains(px, py) {
            match self.state.active_tab {
                Tab::Subtitles => {
                    // Subtitle track selector dropdown
                    if self.geom.sub_track_selector_rect.contains(px, py) {
                        self.state.sidebar_sub_dropdown_open =
                            !self.state.sidebar_sub_dropdown_open;
                        self.request_redraw();
                        return true;
                    }

                    if self.geom.btn_fetch_subs.contains(px, py) {
                        self.action_queue.push_back(UiAction::FetchSubtitles);
                        self.request_redraw();
                        return true;
                    }

                    if self.geom.btn_sub_settings.contains(px, py) {
                        self.state.sub_settings_modal = Some(controls::SubtitleSettingsModalState {
                            auto_fetch: self.state.auto_fetch_subs,
                            preferred_lang: self.state.preferred_sub_lang.clone(),
                            opensubtitles_key: self.state.opensubtitles_api_key.clone().unwrap_or_default(),
                            jimaku_key: self.state.jimaku_api_key.clone().unwrap_or_default(),
                            focused_field: controls::SubModalFocusedField::None,
                            status_feedback: None,
                        });
                        self.set_video_visible(false);
                        self.request_redraw();
                        return true;
                    }

                    // Extract button (adaptive format)
                    if self.geom.btn_extract_ass.contains(px, py) {
                        if self.state.is_streaming_torrent && !self.state.is_torrent_complete {
                            // Disabled when streaming unfinished torrent
                            return true;
                        }

                        let sel_track = self
                            .state
                            .tracks
                            .iter()
                            .find(|t| Some(t.id) == self.state.selected_extract_sub_id)
                            .or_else(|| self.state.tracks.iter().find(|t| t.selected))
                            .or_else(|| self.state.tracks.first());

                        let (filter_name, ext, default_filename) = if let Some(tr) = sel_track {
                            let c = tr.codec.to_lowercase();
                            if c.contains("pgs") || c.contains("vobsub") || c.contains("hdmv") {
                                ("SUP Subtitle (*.sup)", "sup", "subtitles.sup")
                            } else if c.contains("srt") || c.contains("utf8") {
                                ("SRT Subtitle (*.srt)", "srt", "subtitles.srt")
                            } else {
                                ("ASS Subtitle (*.ass)", "ass", "subtitles.ass")
                            }
                        } else {
                            ("ASS Subtitle (*.ass)", "ass", "subtitles.ass")
                        };

                        // Open native save dialog
                        let dest = rfd::FileDialog::new()
                            .add_filter(filter_name, &[ext])
                            .set_file_name(default_filename)
                            .save_file();

                        self.action_queue.push_back(UiAction::ExtractSubtitle {
                            track_id: self.state.selected_extract_sub_id,
                            destination: dest,
                        });
                        return true;
                    }

                    // Subtitle cue card click & copy button & online subtitle attach
                    if self.geom.subtitle_list_rect.contains(px, py) {
                        let mut y =
                            self.geom.subtitle_list_rect.y + 8 - self.state.sub_scroll_y;
                        let list_r = &self.geom.subtitle_list_rect;

                        if self.state.is_searching_subs {
                            y += 42;
                        }

                        if !self.state.fetched_subtitles.is_empty() {
                            y += 24;
                            let card_h = 52;
                            let card_margin = 6;
                            for sub in &self.state.fetched_subtitles {
                                let card_r = Rect::new(list_r.x + 12, y, list_r.w - 24, card_h);
                                if card_r.contains(px, py) {
                                    self.action_queue.push_back(UiAction::AttachExternalSubtitle(sub.clone()));
                                    self.request_redraw();
                                    return true;
                                }
                                y += card_h + card_margin;
                            }

                            if !self.state.cues.is_empty() {
                                y += 32;
                            }
                        }

                        let card_h = 70;
                        let card_margin = 8;
                        let mut copy_target = None;
                        let mut seek_target = None;

                        for (idx, cue) in self.state.cues.iter().enumerate() {
                            let card_r = Rect::new(
                                self.geom.subtitle_list_rect.x + 12,
                                y,
                                self.geom.subtitle_list_rect.w - 24,
                                card_h,
                            );
                            if card_r.contains(px, py) {
                                let copy_r = Rect::new(card_r.right() - 56, card_r.y + 8, 48, 20);
                                if copy_r.contains(px, py) {
                                    copy_target = Some((idx, cue.text.clone()));
                                } else {
                                    seek_target = Some((idx, cue.timestamp));
                                }
                                break;
                            }
                            y += card_h + card_margin;
                        }

                        if let Some((idx, text)) = copy_target {
                            self.copy_cue_to_clipboard(idx, &text);
                            self.request_redraw();
                            return true;
                        }

                        if let Some((idx, ts)) = seek_target {
                            self.action_queue.push_back(UiAction::Player(
                                PlayerCommand::SeekAbsolute(ts),
                            ));
                            self.state.active_cue_idx = Some(idx);
                            self.request_redraw();
                            return true;
                        }
                    }
                }

                Tab::Episodes => {
                    if self.geom.ep_header_open_folder.contains(px, py)
                        || (self.state.episodes.is_empty() && self.geom.ep_empty_open_folder.contains(px, py))
                    {
                        self.action_queue.push_back(UiAction::OpenFolderPicker);
                        return true;
                    }

                    if self.geom.episode_list_rect.contains(px, py) {
                        let item_h = 56;
                        let item_margin = 6;
                        let mut y = self.geom.episode_list_rect.y + 8 - self.state.ep_scroll_y;
                        for (idx, ep) in self.state.episodes.iter().enumerate() {
                            let row_r = Rect::new(
                                self.geom.episode_list_rect.x + 12,
                                y,
                                self.geom.episode_list_rect.w - 24,
                                item_h,
                            );
                            if row_r.contains(px, py) {
                                let now = Instant::now();
                                let is_double_click = match self.state.last_ep_click {
                                    Some((prev_time, prev_idx)) => {
                                        prev_idx == idx
                                            && now.duration_since(prev_time).as_millis() < 400
                                    }
                                    None => false,
                                };

                                self.state.last_ep_click = Some((now, idx));
                                self.state.active_ep_idx = Some(idx);

                                if let Some(ref torrent_info) = ep.torrent_info {
                                    // Batch torrent stream episode
                                    let (t_id, f_idx) = *torrent_info;
                                    self.set_stream_buffering(Some(BufferingStatus {
                                        title: ep.clean_title.clone(),
                                        stage: "Buffering stream...".to_string(),
                                        buffered_bytes: 0,
                                        total_bytes: 0,
                                        speed_bytes: 0,
                                        peers: 0,
                                    }));
                                    self.action_queue.push_back(UiAction::SelectBatchTorrentFile {
                                        id: t_id,
                                        file_idx: f_idx,
                                    });
                                } else if is_double_click {
                                    // Local media file
                                    let ep_path = ep.path.clone();
                                    self.action_queue
                                        .push_back(UiAction::LoadFile(ep_path.clone()));
                                    self.action_queue.push_back(UiAction::Player(
                                        PlayerCommand::Load(ep_path.display().to_string()),
                                    ));
                                }

                                self.request_redraw();
                                return true;
                            }
                            y += item_h + item_margin;
                        }
                    }
                }

                Tab::Nyaa => {
                    if self.geom.nyaa_search_input.contains(px, py) {
                        self.state.search_focused = true;
                        self.request_redraw();
                        return true;
                    } else {
                        self.state.search_focused = false;
                    }

                    if self.geom.nyaa_search_btn.contains(px, py) {
                        self.trigger_nyaa_search();
                        return true;
                    }

                    if self.geom.nyaa_trusted_checkbox.contains(px, py) {
                        self.state.trusted_only = !self.state.trusted_only;
                        self.request_redraw();
                        return true;
                    }

                    if self.geom.nyaa_category_dropdown.contains(px, py) {
                        self.state.category_dropdown_open =
                            !self.state.category_dropdown_open;
                        self.request_redraw();
                        return true;
                    }

                    if self.geom.stream_download_dir_btn.contains(px, py) {
                        self.action_queue.push_back(UiAction::ChangeDownloadDir);
                        return true;
                    }

                    if self.geom.stream_trackers_btn.contains(px, py) {
                        let custom_text = self.state.custom_trackers.join("\r\n");
                        self.state.trackers_modal = Some(controls::TrackersModalState {
                            auto_sync: self.state.auto_sync_trackers,
                            tracker_url: self.state.tracker_list_url.clone(),
                            custom_text,
                            sync_status: None,
                            focused_field: controls::TrackersFocusedField::None,
                            scroll_y: 0,
                        });
                        self.set_video_visible(false);
                        self.request_redraw();
                        return true;
                    }

                    if self.geom.nyaa_results_list.contains(px, py) {
                        let mut y =
                            self.geom.nyaa_results_list.y + 4 - self.state.nyaa_scroll_y;

                        // Downloads section (Active then Completed)
                        let active: Vec<(usize, &crate::types::TorrentDownloadItem)> = self
                            .state
                            .downloads
                            .iter()
                            .enumerate()
                            .filter(|(_, dl)| !matches!(dl.state, crate::types::TorrentTaskState::Completed))
                            .collect();
                        let completed: Vec<(usize, &crate::types::TorrentDownloadItem)> = self
                            .state
                            .downloads
                            .iter()
                            .enumerate()
                            .filter(|(_, dl)| matches!(dl.state, crate::types::TorrentTaskState::Completed))
                            .collect();

                        // Section 1: Active downloads click handling
                        if !active.is_empty() {
                            y += 22;
                            for &(_idx, dl) in &active {
                                let card_h = 68;
                                let card_r = Rect::new(
                                    self.geom.nyaa_results_list.x + 12,
                                    y,
                                    self.geom.nyaa_results_list.w - 24,
                                    card_h,
                                );
                                if card_r.contains(px, py) {
                                    let btn_y = card_r.y + 38;
                                    let btn_h = 22;
                                    let rem_x = card_r.x + card_r.w - 10 - 24;
                                    let fol_x = rem_x - 5 - 46;
                                    let pause_x = fol_x - 5 - 46;

                                    if Rect::new(rem_x, btn_y, 24, btn_h).contains(px, py) {
                                        self.action_queue.push_back(UiAction::CancelTorrent {
                                            id: dl.id,
                                            delete_files: false,
                                        });
                                        return true;
                                    } else if Rect::new(fol_x, btn_y, 46, btn_h).contains(px, py) {
                                        self.action_queue
                                            .push_back(UiAction::OpenTorrentFolder(dl.output_path.clone()));
                                        return true;
                                    } else if Rect::new(pause_x, btn_y, 46, btn_h).contains(px, py) {
                                        if matches!(dl.state, crate::types::TorrentTaskState::Paused) {
                                            self.action_queue.push_back(UiAction::ResumeTorrent(dl.id));
                                        } else {
                                            self.action_queue.push_back(UiAction::PauseTorrent(dl.id));
                                        }
                                        return true;
                                    }
                                    return true;
                                }
                                y += card_h + 6;
                            }
                            y += 6;
                        }

                        // Section 2: Completed downloads click handling
                        if !completed.is_empty() {
                            y += 22;
                            for &(_idx, dl) in &completed {
                                let card_h = 68;
                                let card_r = Rect::new(
                                    self.geom.nyaa_results_list.x + 12,
                                    y,
                                    self.geom.nyaa_results_list.w - 24,
                                    card_h,
                                );
                                if card_r.contains(px, py) {
                                    let btn_y = card_r.y + 38;
                                    let btn_h = 22;
                                    let is_batch = dl.batch_info.map(|(_, total)| total > 1).unwrap_or(false);
                                    let is_confirming = self
                                        .state
                                        .confirming_delete
                                        .map(|(id, t)| id == dl.id && t.elapsed() < Duration::from_secs(3))
                                        .unwrap_or(false);
                                    let del_w = if is_confirming { 62 } else { 46 };
                                    let del_x = card_r.x + card_r.w - 10 - del_w;
                                    let fol_x = del_x - 5 - 46;
                                    let files_w = 40;
                                    let files_x = fol_x - 5 - files_w;
                                    let play_w = 40;
                                    let play_x = (if is_batch { files_x } else { fol_x }) - 5 - play_w;

                                    if Rect::new(del_x, btn_y, del_w, btn_h).contains(px, py) {
                                        if let Some((conf_id, t)) = self.state.confirming_delete {
                                            if conf_id == dl.id && t.elapsed() < Duration::from_secs(3) {
                                                self.state.confirming_delete = None;
                                                self.action_queue.push_back(UiAction::DeleteTorrent(dl.id));
                                                self.request_redraw();
                                                return true;
                                            }
                                        }
                                        self.state.confirming_delete = Some((dl.id, Instant::now()));
                                        self.request_redraw();
                                        return true;
                                    } else if Rect::new(fol_x, btn_y, 46, btn_h).contains(px, py) {
                                        self.action_queue
                                            .push_back(UiAction::OpenTorrentFolder(dl.output_path.clone()));
                                        return true;
                                    } else if is_batch && Rect::new(files_x, btn_y, files_w, btn_h).contains(px, py) {
                                        self.action_queue.push_back(UiAction::OpenBatchPicker(dl.id));
                                        return true;
                                    } else if Rect::new(play_x, btn_y, play_w, btn_h).contains(px, py) {
                                        if dl.output_path.exists() {
                                            let target = if dl.output_path.is_dir() {
                                                crate::episodes::scan_folder_episodes(&dl.output_path)
                                                    .first()
                                                    .map(|e| e.path.clone())
                                            } else {
                                                Some(dl.output_path.clone())
                                            };
                                            if let Some(path) = target {
                                                self.action_queue
                                                    .push_back(UiAction::LoadFile(path.clone()));
                                                self.action_queue.push_back(UiAction::Player(
                                                    PlayerCommand::Load(path.display().to_string()),
                                                ));
                                            }
                                        }
                                        return true;
                                    }
                                    return true;
                                }
                                y += card_h + 6;
                            }
                            y += 6;
                        }

                        // Search results section
                        if !self.state.nyaa_results.is_empty() {
                            if !self.state.downloads.is_empty() {
                                y += 22;
                            }
                            let item_h = 72;
                            let item_margin = 6;
                            for (_idx, item) in self.state.nyaa_results.iter().enumerate() {
                                let row_r = Rect::new(
                                    self.geom.nyaa_results_list.x + 12,
                                    y,
                                    self.geom.nyaa_results_list.w - 24,
                                    item_h,
                                );
                                if row_r.contains(px, py) {
                                    let btn_y = row_r.y + 38;
                                    let btn_h = 22;

                                    if cfg!(feature = "torrent") {
                                        let copy_x = row_r.x + row_r.w - 10 - 40;
                                        let dl_x = copy_x - 6 - 58;

                                        let is_dl = item.info_hash.as_ref().map(|h| self.state.completed_hashes.contains(&h.to_ascii_lowercase())).unwrap_or(false);
                                        let has_prog = is_dl && item.info_hash.as_ref().and_then(|h| self.state.watch_progress_keys.get(&h.to_ascii_lowercase())).map(|p| *p >= 10.0).unwrap_or(false);
                                        let watch_w = if is_dl { if has_prog { 56 } else { 46 } } else { 50 };
                                        let stream_x = dl_x - 6 - watch_w;

                                        if Rect::new(copy_x, btn_y, 40, btn_h).contains(px, py) {
                                            let magnet =
                                                item.magnet_uri().unwrap_or_else(|| item.link.clone());
                                            self.action_queue.push_back(UiAction::CopyText(magnet));
                                            return true;
                                        } else if Rect::new(dl_x, btn_y, 58, btn_h).contains(px, py) {
                                            let torrent_url = if !item.link.is_empty()
                                                && (item.link.ends_with(".torrent")
                                                    || item.link.contains("/download/"))
                                            {
                                                Some(item.link.clone())
                                            } else {
                                                None
                                            };
                                            let magnet = item.magnet_uri();

                                            let (source, fallback_source) = match (torrent_url, magnet) {
                                                (Some(t_url), Some(mag)) => (t_url, Some(mag)),
                                                (Some(t_url), None) => (t_url, None),
                                                (None, Some(mag)) => (mag, None),
                                                (None, None) => (item.link.clone(), None),
                                            };

                                            self.state.torrent_status_text =
                                                "Fetching torrent file list...".to_string();
                                            self.state.status_message = Some((
                                                "Fetching torrent file list...".to_string(),
                                                Instant::now(),
                                            ));
                                            self.action_queue.push_back(UiAction::StartTorrentDownload {
                                                source,
                                                fallback_source,
                                                title: item.clean_title.clone(),
                                            });
                                            return true;
                                        } else if Rect::new(stream_x, btn_y, watch_w, btn_h).contains(px, py) {
                                            self.action_queue
                                                .push_back(UiAction::StreamNyaa(item.clone()));
                                            return true;
                                        } else {
                                            self.action_queue
                                                .push_back(UiAction::StreamNyaa(item.clone()));
                                            return true;
                                        }
                                    } else {
                                        let copy_x = row_r.x + row_r.w - 10 - 40;
                                        let open_x = copy_x - 6 - 76;

                                        if Rect::new(copy_x, btn_y, 40, btn_h).contains(px, py) {
                                            let magnet =
                                                item.magnet_uri().unwrap_or_else(|| item.link.clone());
                                            self.action_queue.push_back(UiAction::CopyText(magnet));
                                            return true;
                                        } else if Rect::new(open_x, btn_y, 76, btn_h).contains(px, py) {
                                            let magnet =
                                                item.magnet_uri().unwrap_or_else(|| item.link.clone());
                                            self.action_queue.push_back(UiAction::OpenExternal(magnet));
                                            return true;
                                        } else {
                                            let magnet =
                                                item.magnet_uri().unwrap_or_else(|| item.link.clone());
                                            self.action_queue.push_back(UiAction::OpenExternal(magnet));
                                            return true;
                                        }
                                    }
                                }
                                y += item_h + item_margin;
                            }
                        }
                    }
                }
            }
        }

        false
    }

    fn handle_key_down(&mut self, logical_key: &Key) -> bool {
        // Trackers configuration modal keyboard input
        if let Some(ref mut modal) = self.state.trackers_modal {
            if logical_key == &Key::Named(NamedKey::Escape) {
                self.state.trackers_modal = None;
                if self.state.duration > 0.0 {
                    self.set_video_visible(true);
                }
                self.request_redraw();
                return true;
            }

            if self.modifiers.control_key() {
                if let Key::Character(s) = logical_key {
                    if s.eq_ignore_ascii_case("v") {
                        if let Ok(mut clip) = arboard::Clipboard::new() {
                            if let Ok(text) = clip.get_text() {
                                match modal.focused_field {
                                    controls::TrackersFocusedField::Url => {
                                        modal.tracker_url.push_str(text.trim());
                                    }
                                    controls::TrackersFocusedField::Custom => {
                                        modal.custom_text.push_str(&text);
                                    }
                                    controls::TrackersFocusedField::None => {}
                                }
                                self.request_redraw();
                                return true;
                            }
                        }
                    }
                }
            }

            match modal.focused_field {
                controls::TrackersFocusedField::Url => {
                    match logical_key {
                        Key::Named(NamedKey::Backspace) => {
                            modal.tracker_url.pop();
                            self.request_redraw();
                            return true;
                        }
                        Key::Named(NamedKey::Space) => {
                            modal.tracker_url.push(' ');
                            self.request_redraw();
                            return true;
                        }
                        Key::Character(s) => {
                            modal.tracker_url.push_str(s);
                            self.request_redraw();
                            return true;
                        }
                        Key::Named(NamedKey::Enter) => {
                            modal.focused_field = controls::TrackersFocusedField::None;
                            self.request_redraw();
                            return true;
                        }
                        _ => return true,
                    }
                }
                controls::TrackersFocusedField::Custom => {
                    match logical_key {
                        Key::Named(NamedKey::Backspace) => {
                            modal.custom_text.pop();
                            self.request_redraw();
                            return true;
                        }
                        Key::Named(NamedKey::Enter) => {
                            modal.custom_text.push('\n');
                            self.request_redraw();
                            return true;
                        }
                        Key::Named(NamedKey::Space) => {
                            modal.custom_text.push(' ');
                            self.request_redraw();
                            return true;
                        }
                        Key::Character(s) => {
                            modal.custom_text.push_str(s);
                            self.request_redraw();
                            return true;
                        }
                        _ => return true,
                    }
                }
                controls::TrackersFocusedField::None => return true,
            }
        }

        // Subtitle settings modal keyboard input
        if let Some(ref mut modal) = self.state.sub_settings_modal {
            if logical_key == &Key::Named(NamedKey::Escape) {
                self.state.sub_settings_modal = None;
                if self.state.duration > 0.0 {
                    self.set_video_visible(true);
                }
                self.request_redraw();
                return true;
            }

            if self.modifiers.control_key() {
                if let Key::Character(s) = logical_key {
                    if s.eq_ignore_ascii_case("v") {
                        if let Ok(mut clip) = arboard::Clipboard::new() {
                            if let Ok(text) = clip.get_text() {
                                match modal.focused_field {
                                    controls::SubModalFocusedField::Lang => {
                                        modal.preferred_lang.push_str(text.trim());
                                    }
                                    controls::SubModalFocusedField::OpenSubKey => {
                                        modal.opensubtitles_key.push_str(text.trim());
                                    }
                                    controls::SubModalFocusedField::JimakuKey => {
                                        modal.jimaku_key.push_str(text.trim());
                                    }
                                    controls::SubModalFocusedField::None => {}
                                }
                                self.request_redraw();
                                return true;
                            }
                        }
                    }
                }
            }

            let field_target = match modal.focused_field {
                controls::SubModalFocusedField::Lang => Some(&mut modal.preferred_lang),
                controls::SubModalFocusedField::OpenSubKey => Some(&mut modal.opensubtitles_key),
                controls::SubModalFocusedField::JimakuKey => Some(&mut modal.jimaku_key),
                controls::SubModalFocusedField::None => None,
            };

            if let Some(buf) = field_target {
                match logical_key {
                    Key::Named(NamedKey::Backspace) => {
                        buf.pop();
                        self.request_redraw();
                        return true;
                    }
                    Key::Named(NamedKey::Space) => {
                        buf.push(' ');
                        self.request_redraw();
                        return true;
                    }
                    Key::Character(s) => {
                        buf.push_str(s);
                        self.request_redraw();
                        return true;
                    }
                    Key::Named(NamedKey::Enter) => {
                        modal.focused_field = controls::SubModalFocusedField::None;
                        self.request_redraw();
                        return true;
                    }
                    _ => return true,
                }
            }
            return true;
        }

        // Keyboard shortcuts for native file/folder dialogs: Ctrl+O and Ctrl+Shift+O
        if self.modifiers.control_key() {
            if let Key::Character(s) = logical_key {
                if s.eq_ignore_ascii_case("o") {
                    if self.modifiers.shift_key() {
                        self.action_queue.push_back(UiAction::OpenFolderPicker);
                    } else {
                        self.action_queue.push_back(UiAction::OpenFilePicker);
                    }
                    return true;
                }
            }
        }

        // If Nyaa search input is focused, direct typing into search query
        if self.state.search_focused {
            match logical_key {
                Key::Named(NamedKey::Enter) => {
                    self.trigger_nyaa_search();
                    return true;
                }
                Key::Named(NamedKey::Escape) => {
                    self.state.search_focused = false;
                    self.request_redraw();
                    return true;
                }
                Key::Named(NamedKey::Backspace) => {
                    self.state.search_query.pop();
                    self.request_redraw();
                    return true;
                }
                Key::Named(NamedKey::Space) => {
                    self.state.search_query.push(' ');
                    self.request_redraw();
                    return true;
                }
                Key::Character(s) => {
                    self.state.search_query.push_str(s);
                    self.request_redraw();
                    return true;
                }
                _ => return false,
            }
        }

        // Global shortcuts matching Requirement 3:
        // Space: Pause/Play
        // S: Cycle subtitles
        // D: Copy active subtitle cue
        // Tab: Toggle right sidebar
        // F: Toggle fullscreen
        // Left / Right: Seek backward/forward 5 seconds
        match logical_key {
            Key::Named(NamedKey::Space) => {
                self.action_queue
                    .push_back(UiAction::Player(PlayerCommand::TogglePause));
                true
            }

            Key::Named(NamedKey::Tab) => {
                self.state.drawer_open = !self.state.drawer_open;
                self.update_layout();
                self.request_redraw();
                true
            }

            Key::Named(NamedKey::ArrowLeft) => {
                self.action_queue
                    .push_back(UiAction::Player(PlayerCommand::SeekRelative(-5.0)));
                true
            }

            Key::Named(NamedKey::ArrowRight) => {
                self.action_queue
                    .push_back(UiAction::Player(PlayerCommand::SeekRelative(5.0)));
                true
            }

            Key::Character(s) if s.eq_ignore_ascii_case("s") => {
                self.action_queue
                    .push_back(UiAction::Player(PlayerCommand::CycleSubtitle));
                true
            }

            Key::Character(s) if s.eq_ignore_ascii_case("a") => {
                self.action_queue
                    .push_back(UiAction::Player(PlayerCommand::CycleAudio));
                true
            }

            Key::Character(s) if s.eq_ignore_ascii_case("d") => {
                // Copy active subtitle cue to clipboard
                if let Some(idx) = self.state.active_cue_idx {
                    if let Some(cue) = self.state.cues.get(idx) {
                        self.copy_cue_to_clipboard(idx, &cue.text.clone());
                    }
                }
                true
            }

            Key::Character(s) if s.eq_ignore_ascii_case("f") => {
                self.state.is_fullscreen = !self.state.is_fullscreen;
                self.action_queue.push_back(UiAction::ToggleFullscreen);
                true
            }

            _ => false,
        }
    }

    fn copy_cue_to_clipboard(&mut self, idx: usize, text: &str) {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(text);
            self.state.copied_feedback = Some((idx, Instant::now()));
            self.request_redraw();
        }
    }

    fn trigger_nyaa_search(&mut self) {
        let q = self.state.search_query.trim().to_string();
        if !q.is_empty() {
            // 1. Direct HTTP/HTTPS/HLS stream URL pasted into search/stream box
            if crate::stream::is_direct_stream_url(&q) {
                self.set_stream_buffering(Some(BufferingStatus {
                    title: q.clone(),
                    stage: "Buffering stream...".to_string(),
                    buffered_bytes: 0,
                    total_bytes: 0,
                    speed_bytes: 0,
                    peers: 0,
                }));
                self.action_queue
                    .push_back(UiAction::Player(PlayerCommand::Load(q.clone())));
                self.state.search_focused = false;
                self.state.current_path = None;
                self.state.media_title = q.clone();
                self.state.cues.clear();
                self.set_title(&q);
                self.request_redraw();
                return;
            }

            // 2. Magnet URI or .torrent URL
            if crate::stream::is_magnet(&q) || crate::stream::is_torrent_url(&q) {
                self.state.search_focused = false;
                let title = if let Some(dn) = q.split("&dn=").nth(1) {
                    let end = dn.find('&').unwrap_or(dn.len());
                    urlencoding::decode(&dn[..end])
                        .unwrap_or_default()
                        .to_string()
                } else {
                    "Pasted Stream".to_string()
                };

                if cfg!(feature = "torrent") {
                    self.set_stream_buffering(Some(BufferingStatus {
                        title: title.clone(),
                        stage: "Fetching torrent metadata...".to_string(),
                        buffered_bytes: 0,
                        total_bytes: 0,
                        speed_bytes: 0,
                        peers: 0,
                    }));
                    self.state.torrent_status_text =
                        "Fetching torrent file list...".to_string();
                    self.state.status_message = Some((
                        "Fetching torrent file list...".to_string(),
                        Instant::now(),
                    ));
                    self.action_queue.push_back(UiAction::StartTorrentStream {
                        source: q.clone(),
                        title,
                    });
                } else {
                    self.action_queue.push_back(UiAction::OpenExternal(q.clone()));
                }
                self.request_redraw();
                return;
            }

            self.state.searching = true;
            self.state.nyaa_error = None;
            let filter = if self.state.trusted_only {
                NyaaFilter::TrustedOnly
            } else {
                NyaaFilter::NoFilter
            };
            self.action_queue.push_back(UiAction::SearchNyaa {
                query: q,
                category: self.state.category,
                filter,
            });
            self.request_redraw();
        }
    }

    pub fn play_next_episode(&mut self) {
        if self.state.episodes.is_empty() {
            return;
        }

        let next_idx = match self.state.active_ep_idx {
            Some(i) => (i + 1) % self.state.episodes.len(),
            None => 0,
        };

        if let Some(ep) = self.state.episodes.get(next_idx) {
            let ep_clone = ep.clone();
            self.state.active_ep_idx = Some(next_idx);
            if let Some((t_id, f_idx)) = ep_clone.torrent_info {
                self.set_stream_buffering(Some(crate::types::BufferingStatus {
                    title: ep_clone.clean_title.clone(),
                    stage: "Buffering stream...".to_string(),
                    buffered_bytes: 0,
                    total_bytes: 0,
                    speed_bytes: 0,
                    peers: 0,
                }));
                self.action_queue.push_back(UiAction::SelectBatchTorrentFile {
                    id: t_id,
                    file_idx: f_idx,
                });
            } else {
                let path = ep_clone.path;
                self.set_video_visible(true);
                self.action_queue
                    .push_back(UiAction::LoadFile(path.clone()));
                self.action_queue.push_back(UiAction::Player(
                    PlayerCommand::Load(path.display().to_string()),
                ));
            }
            self.request_redraw();
        }
    }

    /// Native Win32 popup menu for subtitle tracks, anchored above btn_sub_toggle
    fn show_sub_track_menu(&mut self) {
        self.state.last_mouse_activity = Instant::now();
        unsafe {
            let hmenu = match CreatePopupMenu() {
                Ok(m) => m,
                Err(_) => return,
            };

            let mut pt = POINT {
                x: self.geom.btn_sub_toggle.x,
                y: self.geom.btn_sub_toggle.y,
            };
            let _ = ClientToScreen(self.bar_hwnd, &mut pt);

            // Item 1: Off
            let off_text: Vec<u16> = "Off\0".encode_utf16().collect();
            let is_off_selected = !self.state.tracks.iter().any(|t| t.selected);
            let off_flags = MF_STRING | if is_off_selected { MF_CHECKED } else { MF_UNCHECKED };
            let _ = AppendMenuW(hmenu, off_flags, 1, PCWSTR::from_raw(off_text.as_ptr()));

            if !self.state.tracks.is_empty() {
                let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
            }

            // Keep wide strings in scope for the duration of menu calls
            let mut text_buffers = Vec::new();
            for tr in &self.state.tracks {
                let lang = if !tr.lang.is_empty() {
                    tr.lang.to_uppercase()
                } else {
                    "UND".to_string()
                };
                let title = if !tr.title.is_empty() {
                    tr.title.clone()
                } else {
                    format!("Track {}", tr.id)
                };
                let codec = if !tr.codec.is_empty() {
                    tr.codec.to_uppercase()
                } else {
                    "SUB".to_string()
                };
                let label = format!("[{lang}] {title} ({codec})\0");
                let utf16: Vec<u16> = label.encode_utf16().collect();
                text_buffers.push(utf16);
            }

            for (idx, tr) in self.state.tracks.iter().enumerate() {
                let utf16 = &text_buffers[idx];
                let flags = MF_STRING | if tr.selected { MF_CHECKED } else { MF_UNCHECKED };
                let item_id = 100 + idx;
                let _ = AppendMenuW(hmenu, flags, item_id, PCWSTR::from_raw(utf16.as_ptr()));
            }

            let cmd = TrackPopupMenuEx(
                hmenu,
                (TPM_RETURNCMD | TPM_BOTTOMALIGN).0,
                pt.x,
                pt.y,
                self.parent_hwnd,
                None,
            );
            let _ = DestroyMenu(hmenu);
            self.state.last_mouse_activity = Instant::now();

            if cmd.0 == 1 {
                self.action_queue
                    .push_back(UiAction::Player(PlayerCommand::SetSubtitleTrack(0)));
                self.request_redraw();
            } else if cmd.0 >= 100 {
                let idx = (cmd.0 - 100) as usize;
                if let Some(tr) = self.state.tracks.get(idx) {
                    self.action_queue
                        .push_back(UiAction::Player(PlayerCommand::SetSubtitleTrack(tr.id)));
                    self.request_redraw();
                }
            }
        }
    }

    /// Native Win32 popup menu for audio tracks, anchored above btn_audio_toggle
    fn show_audio_track_menu(&mut self) {
        if self.state.audio_tracks.is_empty() {
            return;
        }
        self.state.last_mouse_activity = Instant::now();

        unsafe {
            let hmenu = match CreatePopupMenu() {
                Ok(m) => m,
                Err(_) => return,
            };

            let mut pt = POINT {
                x: self.geom.btn_audio_toggle.x,
                y: self.geom.btn_audio_toggle.y,
            };
            let _ = ClientToScreen(self.bar_hwnd, &mut pt);

            let mut text_buffers = Vec::new();
            for tr in &self.state.audio_tracks {
                let lang = if !tr.lang.is_empty() {
                    tr.lang.to_uppercase()
                } else {
                    "UND".to_string()
                };
                let title = if !tr.title.is_empty() {
                    tr.title.clone()
                } else {
                    format!("Track {}", tr.id)
                };
                let codec = if !tr.codec.is_empty() {
                    tr.codec.to_uppercase()
                } else {
                    "AUDIO".to_string()
                };
                let channels = if let Some(ch) = tr.channels {
                    format!(" {ch}ch")
                } else {
                    String::new()
                };
                let label = format!("[{lang}] {title} ({codec}{channels})\0");
                let utf16: Vec<u16> = label.encode_utf16().collect();
                text_buffers.push(utf16);
            }

            for (idx, tr) in self.state.audio_tracks.iter().enumerate() {
                let utf16 = &text_buffers[idx];
                let flags = MF_STRING | if tr.selected { MF_CHECKED } else { MF_UNCHECKED };
                let item_id = 200 + idx;
                let _ = AppendMenuW(hmenu, flags, item_id, PCWSTR::from_raw(utf16.as_ptr()));
            }

            let cmd = TrackPopupMenuEx(
                hmenu,
                (TPM_RETURNCMD | TPM_BOTTOMALIGN).0,
                pt.x,
                pt.y,
                self.parent_hwnd,
                None,
            );
            let _ = DestroyMenu(hmenu);
            self.state.last_mouse_activity = Instant::now();

            if cmd.0 >= 200 {
                let idx = (cmd.0 - 200) as usize;
                if let Some(tr) = self.state.audio_tracks.get(idx) {
                    self.action_queue
                        .push_back(UiAction::Player(PlayerCommand::SetAudioTrack(tr.id)));
                    self.request_redraw();
                }
            }
        }
    }
}
