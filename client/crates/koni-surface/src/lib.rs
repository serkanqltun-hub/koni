//! KONİ video surface.
//!
//! Platform-neutral part: [`LayoutConfig`], the pure [`video_rect`] layout
//! function and the [`VideoSurface`] trait (docs/design/client.md §3.3).
//!
//! Windows part (`cfg(windows)`): [`Win32ChildSurface`], a native child HWND
//! inside the Tauri main window that libmpv renders into via `wid`
//! (docs/design/client.md §3.2). The parent window is subclassed so the video box follows
//! `WM_SIZE` / `WM_WINDOWPOSCHANGED` / `WM_DPICHANGED` directly in the window
//! procedure, without a JS roundtrip.

use std::fmt;

use serde::{Deserialize, Serialize};

#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub use win32::Win32ChildSurface;

/// Which layout the video box uses.
///
/// Serialized as `"cockpit"`, `"fullscreen"`, `"hidden"`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LayoutMode {
    /// Video in the top-left box; right column + bottom bar belong to the webview.
    Cockpit,
    /// Video fills the whole client area.
    Fullscreen,
    /// Video hidden (Hub/Lounge/prep screens, full-window modals).
    Hidden,
}

/// Sparse layout config sent from Svelte (values in DIP / logical pixels).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutConfig {
    pub mode: LayoutMode,
    pub right_col_dip: f64,
    pub bottom_bar_dip: f64,
    pub gap_dip: f64,
    /// Custom title bar height above the video (0 when the OS draws it).
    #[serde(default)]
    pub top_bar_dip: f64,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            mode: LayoutMode::Cockpit,
            right_col_dip: 340.0,
            bottom_bar_dip: 72.0,
            gap_dip: 0.0,
            top_bar_dip: 0.0,
        }
    }
}

/// Rectangle in physical pixels, relative to the parent's client area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const ZERO: Rect = Rect {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    };
}

/// DIP → physical pixels, rounded. Non-finite / negative inputs count as 0.
fn dip_to_px(dip: f64, scale: f64) -> i32 {
    let v = dip * scale;
    if !v.is_finite() || v <= 0.0 {
        0
    } else if v >= i32::MAX as f64 {
        i32::MAX
    } else {
        v.round() as i32
    }
}

/// Compute the video box in physical pixels for a client area of
/// `client_w × client_h` physical pixels at `scale` (DPI / 96).
///
/// - Cockpit: `x = 0, y = top_bar * scale, w = client_w - (right_col + gap) * scale,
///   h = client_h - (top_bar + bottom_bar + gap) * scale`, never below 0.
/// - Fullscreen: the whole client area.
/// - Hidden: all zeros (the surface is also hidden).
///
/// A non-finite or non-positive `scale` is treated as 1.0.
pub fn video_rect(client_w: i32, client_h: i32, scale: f64, layout: &LayoutConfig) -> Rect {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let cw = client_w.max(0);
    let ch = client_h.max(0);
    match layout.mode {
        LayoutMode::Hidden => Rect::ZERO,
        LayoutMode::Fullscreen => Rect {
            x: 0,
            y: 0,
            w: cw,
            h: ch,
        },
        LayoutMode::Cockpit => {
            let right = dip_to_px(
                layout.right_col_dip.max(0.0) + layout.gap_dip.max(0.0),
                scale,
            );
            let bottom = dip_to_px(
                layout.bottom_bar_dip.max(0.0) + layout.gap_dip.max(0.0),
                scale,
            );
            let top = dip_to_px(layout.top_bar_dip.max(0.0), scale).min(ch);
            Rect {
                x: 0,
                y: top,
                w: cw.saturating_sub(right).max(0),
                h: ch.saturating_sub(bottom).saturating_sub(top).max(0),
            }
        }
    }
}

/// What to hand to libmpv as its render target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MpvTarget {
    /// Native window id for mpv's `wid` option (HWND on Windows).
    Wid(i64),
}

/// Errors from creating / driving a video surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceError {
    /// The given parent handle is not supported on this platform.
    UnsupportedHandle(String),
    /// No implementation for this platform yet.
    UnsupportedPlatform,
    /// A native OS call failed.
    Os { call: &'static str, message: String },
    /// The surface was already destroyed.
    Destroyed,
}

impl fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SurfaceError::UnsupportedHandle(h) => {
                write!(f, "unsupported parent window handle: {h}")
            }
            SurfaceError::UnsupportedPlatform => {
                write!(f, "video surface not supported on this platform")
            }
            SurfaceError::Os { call, message } => write!(f, "{call} failed: {message}"),
            SurfaceError::Destroyed => write!(f, "video surface already destroyed"),
        }
    }
}

impl std::error::Error for SurfaceError {}

/// Lifecycle of the native video surface (docs/design/client.md §3.3).
pub trait VideoSurface {
    fn create(
        parent: raw_window_handle::RawWindowHandle,
        layout: &LayoutConfig,
    ) -> Result<Self, SurfaceError>
    where
        Self: Sized;
    fn mpv_target(&self) -> MpvTarget;
    fn apply_layout(&mut self, layout: &LayoutConfig);
    fn on_parent_resized(&mut self, client_w: i32, client_h: i32, scale: f64);
    fn set_visible(&mut self, visible: bool);
    fn enter_fullscreen(&mut self) -> Result<(), SurfaceError>;
    fn exit_fullscreen(&mut self) -> Result<(), SurfaceError>;
    fn destroy(self)
    where
        Self: Sized;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cockpit() -> LayoutConfig {
        LayoutConfig::default()
    }

    #[test]
    fn default_config() {
        let c = LayoutConfig::default();
        assert_eq!(c.mode, LayoutMode::Cockpit);
        assert_eq!(
            (c.right_col_dip, c.bottom_bar_dip, c.gap_dip),
            (340.0, 72.0, 0.0)
        );
    }

    #[test]
    fn cockpit_scale_1() {
        assert_eq!(
            video_rect(1280, 720, 1.0, &cockpit()),
            Rect {
                x: 0,
                y: 0,
                w: 940,
                h: 648
            }
        );
    }

    #[test]
    fn cockpit_scale_1_5() {
        // 340*1.5 = 510, 72*1.5 = 108
        assert_eq!(
            video_rect(1920, 1080, 1.5, &cockpit()),
            Rect {
                x: 0,
                y: 0,
                w: 1410,
                h: 972
            }
        );
    }

    #[test]
    fn cockpit_with_gap_and_rounding() {
        let l = LayoutConfig {
            gap_dip: 8.0,
            ..cockpit()
        };
        // (340+8)*1.25 = 435, (72+8)*1.25 = 100
        assert_eq!(
            video_rect(1600, 900, 1.25, &l),
            Rect {
                x: 0,
                y: 0,
                w: 1165,
                h: 800
            }
        );
        let l = LayoutConfig {
            right_col_dip: 1.0,
            bottom_bar_dip: 1.0,
            gap_dip: 0.0,
            top_bar_dip: 0.0,
            mode: LayoutMode::Cockpit,
        };
        // 1.5 -> rounds to 2
        assert_eq!(
            video_rect(100, 100, 1.5, &l),
            Rect {
                x: 0,
                y: 0,
                w: 98,
                h: 98
            }
        );
    }

    #[test]
    fn cockpit_with_top_bar() {
        let l = LayoutConfig {
            top_bar_dip: 40.0,
            right_col_dip: 64.0,
            bottom_bar_dip: 64.0,
            ..cockpit()
        };
        // 40*1.5 = 60, 64*1.5 = 96
        assert_eq!(
            video_rect(1920, 1200, 1.5, &l),
            Rect {
                x: 0,
                y: 60,
                w: 1824,
                h: 1044
            }
        );
        // top bar taller than the window: zero height, y stays inside
        assert_eq!(
            video_rect(500, 30, 1.0, &l),
            Rect {
                x: 0,
                y: 30,
                w: 436,
                h: 0
            }
        );
    }

    #[test]
    fn cockpit_clamps_to_zero() {
        assert_eq!(video_rect(300, 50, 1.0, &cockpit()), Rect::ZERO);
        assert_eq!(video_rect(-10, -10, 2.0, &cockpit()), Rect::ZERO);
        assert_eq!(
            video_rect(400, 100, 1.0, &cockpit()),
            Rect {
                x: 0,
                y: 0,
                w: 60,
                h: 28
            }
        );
    }

    #[test]
    fn fullscreen_is_whole_client() {
        let l = LayoutConfig {
            mode: LayoutMode::Fullscreen,
            ..cockpit()
        };
        assert_eq!(
            video_rect(2560, 1440, 2.0, &l),
            Rect {
                x: 0,
                y: 0,
                w: 2560,
                h: 1440
            }
        );
        assert_eq!(
            video_rect(-5, 10, 1.0, &l),
            Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 10
            }
        );
    }

    #[test]
    fn hidden_is_zero() {
        let l = LayoutConfig {
            mode: LayoutMode::Hidden,
            ..cockpit()
        };
        assert_eq!(video_rect(1920, 1080, 1.0, &l), Rect::ZERO);
    }

    #[test]
    fn bad_scale_falls_back_to_1() {
        for s in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                video_rect(1280, 720, s, &cockpit()),
                Rect {
                    x: 0,
                    y: 0,
                    w: 940,
                    h: 648
                }
            );
        }
    }

    #[test]
    fn serde_camel_case() {
        let json = serde_json::to_string(&cockpit()).unwrap();
        assert_eq!(
            json,
            r#"{"mode":"cockpit","rightColDip":340.0,"bottomBarDip":72.0,"gapDip":0.0,"topBarDip":0.0}"#
        );
        let l: LayoutConfig = serde_json::from_str(
            r#"{"mode":"fullscreen","rightColDip":300,"bottomBarDip":64,"gapDip":4}"#,
        )
        .unwrap();
        assert_eq!(l.mode, LayoutMode::Fullscreen);
        assert_eq!(l.right_col_dip, 300.0);
        assert_eq!(
            l.top_bar_dip, 0.0,
            "older configs without topBarDip still parse"
        );
        let h: LayoutMode = serde_json::from_str(r#""hidden""#).unwrap();
        assert_eq!(h, LayoutMode::Hidden);
    }

    #[test]
    fn error_display() {
        let e = SurfaceError::Os {
            call: "CreateWindowExW",
            message: "x".into(),
        };
        assert_eq!(e.to_string(), "CreateWindowExW failed: x");
        let _: &dyn std::error::Error = &e;
    }
}
