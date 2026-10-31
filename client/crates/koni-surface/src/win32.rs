//! `Win32ChildSurface`: a native child HWND inside the Tauri main window that
//! libmpv draws into (`wid`), kept in place by subclassing the parent window.
//!
//! # Threading
//! Every method (including `create`, `destroy` and `Drop`) must be called on
//! the thread that owns the parent window, i.e. the Tauri main/UI thread
//! (`AppHandle::run_on_main_thread`). Win32 window APIs such as
//! `SetWindowSubclass`, `RemoveWindowSubclass` and `DestroyWindow` only work
//! on the owning thread.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread::ThreadId;

use raw_window_handle::RawWindowHandle;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, GetStockObject, MonitorFromWindow, BLACK_BRUSH, HBRUSH, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowLongPtrW,
    GetWindowPlacement, IsIconic, IsWindow, LoadCursorW, RegisterClassExW, SetWindowLongPtrW,
    SetWindowPlacement, SetWindowPos, ShowWindow, CS_DBLCLKS, GWL_STYLE, HWND_TOP, IDC_ARROW,
    MA_NOACTIVATE, SET_WINDOW_POS_FLAGS, SIZE_MINIMIZED, SWP_FRAMECHANGED, SWP_HIDEWINDOW,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
    SW_HIDE, SW_SHOWNA, WINDOWPLACEMENT, WINDOW_EX_STYLE, WM_DPICHANGED, WM_MOUSEACTIVATE,
    WM_NCDESTROY, WM_SIZE, WM_WINDOWPOSCHANGED, WNDCLASSEXW, WS_CHILD, WS_CLIPCHILDREN,
    WS_CLIPSIBLINGS, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE,
};

use crate::{video_rect, LayoutConfig, LayoutMode, MpvTarget, SurfaceError, VideoSurface};

const CLASS_NAME: PCWSTR = w!("KoniVideoSurface");

fn hwnd(v: isize) -> HWND {
    HWND(v as *mut c_void)
}

fn os_err(call: &'static str, e: windows::core::Error) -> SurfaceError {
    SurfaceError::Os {
        call,
        message: e.to_string(),
    }
}

/// Saved parent state while in fullscreen.
struct Saved {
    style: isize,
    placement: WINDOWPLACEMENT,
    /// Layout to restore on exit (updated by `apply_layout` while fullscreen).
    layout: LayoutConfig,
}

struct State {
    layout: LayoutConfig,
    visible: bool,
    fullscreen: Option<Saved>,
}

/// State shared between the surface handle and the parent subclass proc.
/// Lives in a `Box` whose raw pointer is the subclass ref data.
struct Shared {
    parent: isize,
    child: isize,
    /// False once the parent got `WM_NCDESTROY` (subclass already removed).
    parent_alive: AtomicBool,
    /// Never held across a Win32 call that can send messages back to us
    /// (std `Mutex` is not reentrant).
    state: Mutex<State>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Position the child for a client area of `w × h` px at `scale`.
    fn relayout_with(&self, w: i32, h: i32, scale: f64) {
        let (r, show) = {
            let st = self.lock();
            (
                video_rect(w, h, scale, &st.layout),
                st.visible && st.layout.mode != LayoutMode::Hidden,
            )
        };
        let flags =
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | if show { SWP_SHOWWINDOW } else { SWP_HIDEWINDOW };
        // SAFETY: plain Win32 call on a window handle we created; a stale
        // handle just makes the call fail.
        unsafe {
            let _ = SetWindowPos(hwnd(self.child), Some(HWND_TOP), r.x, r.y, r.w, r.h, flags);
        }
    }

    /// Recompute from the parent's current client rect and DPI.
    fn relayout(&self) {
        let parent = hwnd(self.parent);
        let mut rc = RECT::default();
        // SAFETY: read-only queries on the parent window.
        let dpi = unsafe {
            if IsIconic(parent).as_bool() || GetClientRect(parent, &mut rc).is_err() {
                return;
            }
            GetDpiForWindow(parent)
        };
        let scale = if dpi == 0 { 1.0 } else { dpi as f64 / 96.0 };
        self.relayout_with(rc.right - rc.left, rc.bottom - rc.top, scale);
    }
}

unsafe extern "system" fn parent_subclass_proc(
    hwnd_: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    refdata: usize,
) -> LRESULT {
    // SAFETY: refdata is the `Box<Shared>` pointer installed in `create`; it
    // is freed only after `RemoveWindowSubclass` on this same thread.
    let shared = &*(refdata as *const Shared);
    match msg {
        WM_SIZE => {
            if wparam.0 as u32 != SIZE_MINIMIZED {
                shared.relayout();
            }
        }
        WM_WINDOWPOSCHANGED => shared.relayout(),
        WM_DPICHANGED => {
            // Apply the rect Windows suggests for the new DPI, then relayout.
            let suggested = lparam.0 as *const RECT;
            if let Some(r) = suggested.as_ref() {
                let _ = SetWindowPos(
                    hwnd_,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            let res = DefSubclassProc(hwnd_, msg, wparam, lparam);
            shared.relayout();
            return res;
        }
        WM_NCDESTROY => {
            // Parent dies first: detach; the child is destroyed with it.
            let _ = RemoveWindowSubclass(hwnd_, Some(parent_subclass_proc), id);
            shared.parent_alive.store(false, Ordering::SeqCst);
        }
        _ => {}
    }
    DefSubclassProc(hwnd_, msg, wparam, lparam)
}

unsafe extern "system" fn child_wnd_proc(
    hwnd_: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_MOUSEACTIVATE {
        // Clicking the video must not steal activation/focus from the webview.
        return LRESULT(MA_NOACTIVATE as isize);
    }
    DefWindowProcW(hwnd_, msg, wparam, lparam)
}

fn register_class() -> Result<HINSTANCE, SurfaceError> {
    static CLASS: OnceLock<Result<isize, SurfaceError>> = OnceLock::new();
    CLASS
        .get_or_init(|| {
            // SAFETY: standard class registration with a static wndproc and name.
            unsafe {
                let hinst: HINSTANCE = GetModuleHandleW(None)
                    .map_err(|e| os_err("GetModuleHandleW", e))?
                    .into();
                let wc = WNDCLASSEXW {
                    cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                    style: CS_DBLCLKS,
                    lpfnWndProc: Some(child_wnd_proc),
                    hInstance: hinst,
                    hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                    hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                    lpszClassName: CLASS_NAME,
                    ..Default::default()
                };
                if RegisterClassExW(&wc) == 0 {
                    return Err(os_err(
                        "RegisterClassExW",
                        windows::core::Error::from_thread(),
                    ));
                }
                Ok(hinst.0 as isize)
            }
        })
        .clone()
        .map(|h| HINSTANCE(h as *mut c_void))
}

/// Native child HWND video surface for Windows (docs/design/client.md §3.2).
///
/// See the module docs: all methods must run on the parent window's UI thread.
pub struct Win32ChildSurface {
    /// `Box<Shared>` raw pointer; `None` once destroyed.
    shared: Option<*mut Shared>,
    child: isize,
    owner_thread: ThreadId,
}

// SAFETY: HWNDs are stored as plain integers and `Shared` only holds
// integers, atomics and a `Mutex`. The Win32 calls themselves are thread-bound
// (documented requirement: call from the parent's UI thread); calling from
// another thread makes them fail or marshal, not corrupt memory. `Drop` on a
// foreign thread leaks instead of freeing (see `teardown`).
unsafe impl Send for Win32ChildSurface {}
unsafe impl Sync for Win32ChildSurface {}

impl Win32ChildSurface {
    fn shared(&self) -> Option<&Shared> {
        // SAFETY: pointer is valid until `teardown` sets `shared` to None.
        self.shared.map(|p| unsafe { &*p })
    }

    /// Child HWND as an integer (also what mpv gets as `wid`).
    pub fn child_hwnd(&self) -> isize {
        self.child
    }

    /// True while `enter_fullscreen` is active.
    pub fn is_fullscreen(&self) -> bool {
        self.shared().is_some_and(|s| s.lock().fullscreen.is_some())
    }

    fn teardown(&mut self) {
        let Some(ptr) = self.shared.take() else {
            return;
        };
        if std::thread::current().id() != self.owner_thread {
            // Cannot unsubclass/destroy from a foreign thread; leaking keeps
            // the subclass proc's pointer valid.
            return;
        }
        // SAFETY: owning thread; after RemoveWindowSubclass the proc can no
        // longer observe `ptr`, so freeing it is sound.
        unsafe {
            let shared = &*ptr;
            if shared.parent_alive.load(Ordering::SeqCst) {
                let _ = RemoveWindowSubclass(
                    hwnd(shared.parent),
                    Some(parent_subclass_proc),
                    ptr as usize,
                );
            }
            let child = hwnd(shared.child);
            if IsWindow(Some(child)).as_bool() {
                let _ = DestroyWindow(child);
            }
            drop(Box::from_raw(ptr));
        }
    }
}

impl VideoSurface for Win32ChildSurface {
    fn create(parent: RawWindowHandle, layout: &LayoutConfig) -> Result<Self, SurfaceError> {
        let RawWindowHandle::Win32(h) = parent else {
            return Err(SurfaceError::UnsupportedHandle(format!("{parent:?}")));
        };
        let parent_hwnd = hwnd(h.hwnd.get());
        let hinst = register_class()?;

        // SAFETY: creating a child of a valid top-level window on its thread.
        let child = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                CLASS_NAME,
                w!("KONİ video"),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                0,
                0,
                0,
                0,
                Some(parent_hwnd),
                None,
                Some(hinst),
                None,
            )
        }
        .map_err(|e| os_err("CreateWindowExW", e))?;

        let ptr = Box::into_raw(Box::new(Shared {
            parent: parent_hwnd.0 as isize,
            child: child.0 as isize,
            parent_alive: AtomicBool::new(true),
            state: Mutex::new(State {
                layout: layout.clone(),
                visible: true,
                fullscreen: None,
            }),
        }));

        // SAFETY: `ptr` stays valid until teardown removes the subclass.
        let ok = unsafe {
            SetWindowSubclass(
                parent_hwnd,
                Some(parent_subclass_proc),
                ptr as usize,
                ptr as usize,
            )
        };
        if !ok.as_bool() {
            let e = windows::core::Error::from_thread();
            // SAFETY: undo creation; subclass was never installed.
            unsafe {
                let _ = DestroyWindow(child);
                drop(Box::from_raw(ptr));
            }
            return Err(os_err("SetWindowSubclass", e));
        }

        let surface = Win32ChildSurface {
            shared: Some(ptr),
            child: child.0 as isize,
            owner_thread: std::thread::current().id(),
        };
        // Places it HWND_TOP (above WebView2's children) with the right rect.
        if let Some(s) = surface.shared() {
            s.relayout();
        }
        Ok(surface)
    }

    fn mpv_target(&self) -> MpvTarget {
        MpvTarget::Wid(self.child as i64)
    }

    /// While fullscreen, the config is stored as the layout to restore on exit.
    fn apply_layout(&mut self, layout: &LayoutConfig) {
        let Some(s) = self.shared() else { return };
        {
            let mut st = s.lock();
            match st.fullscreen.as_mut() {
                Some(saved) => saved.layout = layout.clone(),
                None => st.layout = layout.clone(),
            }
        }
        s.relayout();
    }

    fn on_parent_resized(&mut self, client_w: i32, client_h: i32, scale: f64) {
        if let Some(s) = self.shared() {
            s.relayout_with(client_w, client_h, scale);
        }
    }

    fn set_visible(&mut self, visible: bool) {
        let Some(s) = self.shared() else { return };
        let show = {
            let mut st = s.lock();
            st.visible = visible;
            visible && st.layout.mode != LayoutMode::Hidden
        };
        // SAFETY: show/hide our own child window.
        unsafe {
            let _ = ShowWindow(hwnd(s.child), if show { SW_SHOWNA } else { SW_HIDE });
        }
        if show {
            s.relayout();
        }
    }

    fn enter_fullscreen(&mut self) -> Result<(), SurfaceError> {
        let s = self.shared().ok_or(SurfaceError::Destroyed)?;
        if s.lock().fullscreen.is_some() {
            return Ok(());
        }
        let parent = hwnd(s.parent);
        // SAFETY: Win32 queries/updates on the parent, on its UI thread; the
        // state lock is not held across calls that dispatch messages.
        unsafe {
            let style = GetWindowLongPtrW(parent, GWL_STYLE);
            let mut placement = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            GetWindowPlacement(parent, &mut placement)
                .map_err(|e| os_err("GetWindowPlacement", e))?;
            let monitor = MonitorFromWindow(parent, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
                return Err(os_err(
                    "GetMonitorInfoW",
                    windows::core::Error::from_thread(),
                ));
            }
            {
                let mut st = s.lock();
                let layout = st.layout.clone();
                st.layout.mode = LayoutMode::Fullscreen;
                st.fullscreen = Some(Saved {
                    style,
                    placement,
                    layout,
                });
            }
            let new_style = (style as u32 & !WS_OVERLAPPEDWINDOW.0) | WS_POPUP.0 | WS_VISIBLE.0;
            SetWindowLongPtrW(parent, GWL_STYLE, new_style as isize);
            let r = mi.rcMonitor;
            let res = SetWindowPos(
                parent,
                Some(HWND_TOP),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_FRAMECHANGED | SWP_NOOWNERZORDER,
            );
            s.relayout();
            res.map_err(|e| os_err("SetWindowPos", e))
        }
    }

    fn exit_fullscreen(&mut self) -> Result<(), SurfaceError> {
        let s = self.shared().ok_or(SurfaceError::Destroyed)?;
        let saved = {
            let mut st = s.lock();
            let Some(saved) = st.fullscreen.take() else {
                return Ok(());
            };
            // Restore the layout first so WM_SIZE during restore uses it.
            st.layout = saved.layout.clone();
            saved
        };
        let parent = hwnd(s.parent);
        // SAFETY: restore the parent's saved style/placement on its UI thread.
        unsafe {
            SetWindowLongPtrW(parent, GWL_STYLE, saved.style);
            let res = SetWindowPlacement(parent, &saved.placement);
            let flags: SET_WINDOW_POS_FLAGS =
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_FRAMECHANGED;
            let _ = SetWindowPos(parent, None, 0, 0, 0, 0, flags);
            s.relayout();
            res.map_err(|e| os_err("SetWindowPlacement", e))
        }
    }

    fn destroy(mut self) {
        self.teardown();
    }
}

impl Drop for Win32ChildSurface {
    fn drop(&mut self) {
        self.teardown();
    }
}
