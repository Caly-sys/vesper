#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    pub fn inset(&self, dx: i32, dy: i32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            w: (self.w - dx * 2).max(0),
            h: (self.h - dy * 2).max(0),
        }
    }
}

/// Calculated UI bounding rectangles for Theater Mode Layout A
pub struct LayoutGeometry {
    pub total_width: i32,
    pub total_height: i32,

    pub video_rect: Rect,
    pub bottom_bar_rect: Rect,

    // Viewport empty state dropzone & pickers
    pub empty_dropzone_rect: Rect,
    pub empty_open_file: Rect,
    pub empty_open_folder: Rect,

    // Bottom playback bar controls
    pub btn_open: Rect,
    pub btn_play_pause: Rect,
    pub btn_next: Rect,
    pub time_text_rect: Rect,
    pub seekbar_rect: Rect,
    pub btn_vol_icon: Rect,
    pub vol_slider_rect: Rect,
    pub btn_audio_toggle: Rect,
    pub btn_sub_toggle: Rect,
    pub btn_fullscreen: Rect,
    pub btn_drawer_toggle: Rect,

    // Track selector popovers
    pub sub_popover_rect: Rect,
    pub audio_popover_rect: Rect,

    // Drawer
    pub drawer_rect: Rect,
    pub drawer_tab_bar: Rect,
    pub tab_subtitles: Rect,
    pub tab_episodes: Rect,
    pub tab_nyaa: Rect,

    // Drawer Subtitles tab
    pub sub_track_selector_rect: Rect,
    pub subtitle_list_rect: Rect,
    pub btn_extract_ass: Rect,

    // Drawer Episodes tab
    pub ep_header_open_folder: Rect,
    pub ep_empty_open_folder: Rect,
    pub episode_list_rect: Rect,

    // Drawer Nyaa tab
    pub nyaa_search_input: Rect,
    pub nyaa_search_btn: Rect,
    pub nyaa_trusted_checkbox: Rect,
    pub nyaa_category_dropdown: Rect,
    pub stream_download_dir_btn: Rect,
    pub nyaa_results_list: Rect,
}

impl LayoutGeometry {
    pub fn calculate(
        width: i32,
        height: i32,
        drawer_open: bool,
        duration: f64,
        sub_label: &str,
        audio_label: &str,
    ) -> Self {
        let bottom_bar_h = 54;
        let drawer_w = if drawer_open {
            330.min(width * 2 / 5).max(280).min(width)
        } else {
            0
        };

        let left_w = (width - drawer_w).max(0);
        let video_rect = Rect::new(0, 0, left_w, height);
        let bottom_bar_rect = Rect::new(0, 0, left_w, bottom_bar_h);

        // Viewport empty state centered elements
        let dz_w = 420.min(left_w - 40).max(200);
        let dz_h = 240.min(height - 40).max(160);
        let dz_x = (left_w - dz_w) / 2;
        let dz_y = (height - dz_h) / 2;
        let empty_dropzone_rect = Rect::new(dz_x, dz_y, dz_w, dz_h);

        let btn_w = 126;
        let btn_h = 34;
        let btn_y = dz_y + dz_h - 60;
        let empty_open_file = Rect::new(dz_x + (dz_w / 2) - btn_w - 8, btn_y, btn_w, btn_h);
        let empty_open_folder = Rect::new(dz_x + (dz_w / 2) + 8, btn_y, btn_w, btn_h);

        // Bottom playback bar widgets (DPI & vertical-centering: 54px bar, 32px controls -> y = 11)
        let ctrl_y = 11;
        let btn_open = Rect::new(14, ctrl_y, 30, 32);
        let btn_play_pause = Rect::new(btn_open.right() + 8, ctrl_y, 32, 32);
        let btn_next = Rect::new(btn_play_pause.right() + 8, ctrl_y, 32, 32);

        // Dynamic timestamp width: "HH:MM:SS / HH:MM:SS" (19 chars -> ~142px) vs "MM:SS / MM:SS" (13 chars -> ~100px)
        let time_w = if duration >= 3600.0 { 142 } else { 102 };
        let time_text_rect = Rect::new(btn_next.right() + 10, 15, time_w, 24);
        let left_group_end = time_text_rect.right();

        // Dynamic button widths for SUB and AUDIO pills:
        // Character count * ~8px + 24px padding (12px each side), clamped to minimum 64px
        let sub_btn_w = ((sub_label.chars().count() as i32) * 8 + 24).max(64);
        let audio_btn_w = ((audio_label.chars().count() as i32) * 8 + 24).max(64);

        // Right side of bottom bar (14px outer right margin)
        let btn_drawer_toggle = Rect::new((left_w - 14 - 28).max(0), ctrl_y, 28, 32);
        let btn_fullscreen = Rect::new((btn_drawer_toggle.x - 8 - 32).max(0), ctrl_y, 32, 32);
        let btn_sub_toggle = Rect::new((btn_fullscreen.x - 8 - sub_btn_w).max(0), ctrl_y, sub_btn_w, 32);
        let btn_audio_toggle = Rect::new((btn_sub_toggle.x - 8 - audio_btn_w).max(0), ctrl_y, audio_btn_w, 32);
        let vol_slider_rect = Rect::new((btn_audio_toggle.x - 10 - 60).max(0), 19, 60, 16);
        let btn_vol_icon = Rect::new((vol_slider_rect.x - 6 - 26).max(0), ctrl_y, 26, 32);
        let right_group_start = btn_vol_icon.x;

        // Seekbar fills space between timestamp and volume icon with at least 16px gap on both sides
        let seek_x = left_group_end + 16;
        let seek_w = (right_group_start - 16 - seek_x).max(0);
        let seekbar_rect = Rect::new(seek_x, 18, seek_w, 18);

        let sub_popover_rect = Rect::default();
        let audio_popover_rect = Rect::default();

        // Right Drawer
        let drawer_x = left_w;
        let drawer_rect = Rect::new(drawer_x, 0, drawer_w, height);

        let drawer_tab_bar = Rect::new(drawer_x, 0, drawer_w, 42);
        let tab_inner_w = (drawer_w - 24).max(0);
        let single_tab_w = (tab_inner_w / 3).max(0);

        let tab_subtitles = Rect::new(drawer_x + 12, 6, single_tab_w, 30);
        let tab_episodes = Rect::new(tab_subtitles.right() + 2, 6, single_tab_w, 30);
        let tab_nyaa = Rect::new(tab_episodes.right() + 2, 6, single_tab_w, 30);

        // Tab 1 Subtitles
        let sub_track_selector_rect = Rect::new(drawer_x + 12, 48, (drawer_w - 24).max(0), 30);
        let btn_extract_ass = Rect::new(drawer_x + 12, (height - 48).max(0), (drawer_w - 24).max(0), 38);
        let subtitle_list_rect = Rect::new(
            drawer_x,
            84,
            drawer_w,
            (btn_extract_ass.y - 88).max(0),
        );

        // Tab 2 Episodes
        let ep_header_open_folder = Rect::new(drawer_x + 12, 48, (drawer_w - 24).max(0), 30);
        let episode_list_rect = Rect::new(drawer_x, 84, drawer_w, (height - 84).max(0));
        let ep_empty_open_folder = Rect::new(
            drawer_x + (drawer_w - 140) / 2,
            episode_list_rect.y + 90,
            140,
            34,
        );

        // Tab 3 Nyaa
        let nyaa_search_btn = Rect::new((drawer_x + drawer_w - 12 - 58).max(drawer_x), 48, 58, 28);
        let nyaa_search_w = (nyaa_search_btn.x - drawer_x - 18).max(0);
        let nyaa_search_input = Rect::new(drawer_x + 12, 48, nyaa_search_w, 28);

        let nyaa_trusted_checkbox = Rect::new(drawer_x + 12, 80, 95, 24);
        let nyaa_category_dropdown = Rect::new(nyaa_trusted_checkbox.right() + 6, 80, 90, 24);
        let dl_dir_x = nyaa_category_dropdown.right() + 6;
        let dl_dir_w = (drawer_x + drawer_w - 12 - dl_dir_x).max(40);
        let stream_download_dir_btn = Rect::new(dl_dir_x, 80, dl_dir_w, 24);

        let nyaa_results_list = Rect::new(
            drawer_x,
            108,
            drawer_w,
            (height - 108).max(0),
        );

        Self {
            total_width: width,
            total_height: height,
            video_rect,
            bottom_bar_rect,
            empty_dropzone_rect,
            empty_open_file,
            empty_open_folder,
            btn_open,
            btn_play_pause,
            btn_next,
            time_text_rect,
            seekbar_rect,
            btn_vol_icon,
            vol_slider_rect,
            btn_audio_toggle,
            btn_sub_toggle,
            btn_fullscreen,
            btn_drawer_toggle,
            sub_popover_rect,
            audio_popover_rect,
            drawer_rect,
            drawer_tab_bar,
            tab_subtitles,
            tab_episodes,
            tab_nyaa,
            sub_track_selector_rect,
            subtitle_list_rect,
            btn_extract_ass,
            ep_header_open_folder,
            ep_empty_open_folder,
            episode_list_rect,
            nyaa_search_input,
            nyaa_search_btn,
            nyaa_trusted_checkbox,
            nyaa_category_dropdown,
            stream_download_dir_btn,
            nyaa_results_list,
        }
    }
}
