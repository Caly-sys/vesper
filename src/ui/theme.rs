#![allow(dead_code)]

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::*;

// Cohesive, understated dark palette with soft muted purple accent
pub const COLOR_VIEWPORT_BG: COLORREF = COLORREF(0x00110E0E); // #0E0E11 deep viewport black
pub const COLOR_BG: COLORREF = COLORREF(0x00181414);          // #141418 sidebar & bar surfaces
pub const COLOR_SURFACE: COLORREF = COLORREF(0x00261E1E);     // #1E1E26 cards & inputs
pub const COLOR_SURFACE_HOVER: COLORREF = COLORREF(0x00342828); // #282834 card hover
pub const COLOR_SURFACE_ACTIVE: COLORREF = COLORREF(0x003A2026); // #26203A card active
pub const COLOR_BORDER: COLORREF = COLORREF(0x00342A2A);      // #2A2A34 subtle borders
pub const COLOR_BORDER_HOVER: COLORREF = COLORREF(0x004C3C3C); // #3C3C4C border hover
pub const COLOR_TEXT_PRIMARY: COLORREF = COLORREF(0x00F6F4F4); // #F4F4F6 high contrast text
pub const COLOR_TEXT_SECONDARY: COLORREF = COLORREF(0x00B4A2A2); // #A2A2B4 secondary labels
pub const COLOR_TEXT_MUTED: COLORREF = COLORREF(0x00786565);     // #656578 hints & shortcuts
pub const COLOR_ACCENT: COLORREF = COLORREF(0x00E66E7C);       // #7C6EE6 soft muted accent
pub const COLOR_ACCENT_HOVER: COLORREF = COLORREF(0x00EE8390); // #9083EE accent hover
pub const COLOR_ACCENT_MUTED: COLORREF = COLORREF(0x003A2026); // #26203A dark accent container
pub const COLOR_SUCCESS: COLORREF = COLORREF(0x0081B910);      // #10B981
pub const COLOR_WARNING: COLORREF = COLORREF(0x000B9EF5);      // #F59E0B
pub const COLOR_ERROR: COLORREF = COLORREF(0x004444EF);        // #EF4444
pub const COLOR_SEEK_TRACK: COLORREF = COLORREF(0x002B2020);   // #20202B seek track background
pub const COLOR_CARD_ACTIVE: COLORREF = COLORREF(0x003A2026);  // #26203A active item background

/// Pre-created fonts for UI rendering
pub struct ThemeFonts {
    pub regular: HFONT,
    pub bold: HFONT,
    pub small: HFONT,
    pub small_bold: HFONT,
    pub heading: HFONT,
    pub mono: HFONT,
}

impl ThemeFonts {
    pub fn new() -> Self {
        unsafe {
            let segoe: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
            let consolas: Vec<u16> = "Consolas\0".encode_utf16().collect();

            let create = |height: i32, weight: i32, face: &[u16]| -> HFONT {
                CreateFontW(
                    height,
                    0,
                    0,
                    0,
                    weight,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET,
                    OUT_DEFAULT_PRECIS,
                    CLIP_DEFAULT_PRECIS,
                    CLEARTYPE_QUALITY,
                    (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                    PCWSTR::from_raw(face.as_ptr()),
                )
            };

            Self {
                regular: create(-13, FW_NORMAL.0 as i32, &segoe),
                bold: create(-13, FW_BOLD.0 as i32, &segoe),
                small: create(-11, FW_NORMAL.0 as i32, &segoe),
                small_bold: create(-11, FW_BOLD.0 as i32, &segoe),
                heading: create(-16, FW_BOLD.0 as i32, &segoe),
                mono: create(-12, FW_NORMAL.0 as i32, &consolas),
            }
        }
    }
}

impl Default for ThemeFonts {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ThemeFonts {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.regular.into());
            let _ = DeleteObject(self.bold.into());
            let _ = DeleteObject(self.small.into());
            let _ = DeleteObject(self.small_bold.into());
            let _ = DeleteObject(self.heading.into());
            let _ = DeleteObject(self.mono.into());
        }
    }
}

/// Double buffering surface for flicker-free GDI rendering
pub struct DoubleBuffer {
    pub mem_dc: Option<HDC>,
    pub mem_bm: Option<HBITMAP>,
    pub old_bm: Option<HGDIOBJ>,
    pub width: i32,
    pub height: i32,
}

impl DoubleBuffer {
    pub fn new() -> Self {
        Self {
            mem_dc: None,
            mem_bm: None,
            old_bm: None,
            width: 0,
            height: 0,
        }
    }

    /// Resize or re-create buffer if dimensions change
    pub fn ensure_size(&mut self, hdc: HDC, width: i32, height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }

        if self.width == width && self.height == height && self.mem_dc.is_some() {
            return;
        }

        self.cleanup();

        unsafe {
            let dc = CreateCompatibleDC(Some(hdc));
            let bm = CreateCompatibleBitmap(hdc, width, height);
            let old = SelectObject(dc, bm.into());

            self.mem_dc = Some(dc);
            self.mem_bm = Some(bm);
            self.old_bm = Some(old);
            self.width = width;
            self.height = height;
        }
    }

    /// Blit memory buffer to screen
    pub fn present(&self, dest_dc: HDC, x: i32, y: i32, width: i32, height: i32) {
        if let Some(dc) = self.mem_dc {
            unsafe {
                let _ = BitBlt(dest_dc, x, y, width, height, Some(dc), x, y, SRCCOPY);
            }
        }
    }

    fn cleanup(&mut self) {
        unsafe {
            if let Some(dc) = self.mem_dc {
                if let Some(old) = self.old_bm {
                    SelectObject(dc, old);
                }
                let _ = DeleteDC(dc);
            }
            if let Some(bm) = self.mem_bm {
                let _ = DeleteObject(bm.into());
            }
        }
        self.mem_dc = None;
        self.mem_bm = None;
        self.old_bm = None;
        self.width = 0;
        self.height = 0;
    }
}

impl Drop for DoubleBuffer {
    fn drop(&mut self) {
        self.cleanup();
    }
}

// Drawing helper functions

pub fn fill_rect(hdc: HDC, x: i32, y: i32, w: i32, h: i32, color: COLORREF) {
    if w <= 0 || h <= 0 {
        return;
    }
    unsafe {
        let brush = CreateSolidBrush(color);
        let rect = RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        FillRect(hdc, &rect, brush);
        let _ = DeleteObject(brush.into());
    }
}

pub fn draw_rounded_rect(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    bg_color: COLORREF,
    border_color: Option<COLORREF>,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    unsafe {
        let brush = CreateSolidBrush(bg_color);
        let old_brush = SelectObject(hdc, brush.into());

        let (pen, old_pen) = if let Some(border) = border_color {
            let p = CreatePen(PS_SOLID, 1, border);
            let op = SelectObject(hdc, p.into());
            (Some(p), Some(op))
        } else {
            let p = CreatePen(PS_NULL, 0, COLORREF(0));
            let op = SelectObject(hdc, p.into());
            (Some(p), Some(op))
        };

        let _ = RoundRect(hdc, x, y, x + w, y + h, radius, radius);

        if let Some(op) = old_pen {
            SelectObject(hdc, op);
        }
        if let Some(p) = pen {
            let _ = DeleteObject(p.into());
        }

        SelectObject(hdc, old_brush);
        let _ = DeleteObject(brush.into());
    }
}

pub fn draw_text(
    hdc: HDC,
    text: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    font: HFONT,
    color: COLORREF,
    format: DRAW_TEXT_FORMAT,
) {
    if text.is_empty() || w <= 0 || h <= 0 {
        return;
    }
    unsafe {
        let mut utf16: Vec<u16> = text.encode_utf16().collect();
        let old_font = SelectObject(hdc, font.into());
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, color);

        let mut rect = RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };

        DrawTextW(hdc, &mut utf16, &mut rect, format);

        SelectObject(hdc, old_font);
    }
}

pub fn measure_text(hdc: HDC, text: &str, font: HFONT) -> (i32, i32) {
    if text.is_empty() {
        return (0, 0);
    }
    unsafe {
        let mut utf16: Vec<u16> = text.encode_utf16().collect();
        let old_font = SelectObject(hdc, font.into());
        let mut rect = RECT::default();

        DrawTextW(
            hdc,
            &mut utf16,
            &mut rect,
            DT_CALCRECT | DT_SINGLELINE,
        );

        SelectObject(hdc, old_font);
        (rect.right - rect.left, rect.bottom - rect.top)
    }
}

pub fn draw_badge(
    hdc: HDC,
    text: &str,
    x: i32,
    y: i32,
    font: HFONT,
    bg_color: COLORREF,
    border_color: Option<COLORREF>,
    text_color: COLORREF,
) -> i32 {
    let (tw, th) = measure_text(hdc, text, font);
    let pad_x = 6;
    let pad_y = 2;
    let w = tw + pad_x * 2;
    let h = th + pad_y * 2;

    draw_rounded_rect(hdc, x, y, w, h, 6, bg_color, border_color);
    draw_text(
        hdc,
        text,
        x + pad_x,
        y + pad_y,
        tw,
        th,
        font,
        text_color,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    w
}
