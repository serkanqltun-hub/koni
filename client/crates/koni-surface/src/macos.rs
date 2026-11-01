//! macOS video surface: an `NSOpenGLView` above the webview, inside the Tauri main
//! window's content view. mpv can't embed into a foreign view on macOS (`wid` is
//! unsupported there), so libmpv runs with `vo=libmpv` and KONİ draws each frame
//! through the render API (docs/design/client.md §3.3).
//!
//! Everything, including drawing, happens on the main thread: mpv's update callback
//! only queues one redraw on the main dispatch queue. Swap interval is 0, so a draw
//! never blocks the UI on vsync (the window server composites the view anyway).
//!
//! The view keeps fixed margins through `autoresizingMask`, so it follows a live
//! window resize without a roundtrip; [`VideoSurface::on_parent_resized`] then fixes
//! the drawable size.

use std::ffi::{c_char, c_int, c_void, CStr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use koni_player::{Player, RenderContext};
use objc2::encode::{Encode, Encoding};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use raw_window_handle::RawWindowHandle;

use crate::{video_rect, LayoutConfig, LayoutMode, MpvTarget, SurfaceError, VideoSurface};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct CGSize {
    w: f64,
    h: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}
// SAFETY: layouts match CoreGraphics' CGPoint/CGSize/CGRect (f64 on 64-bit macOS).
unsafe impl Encode for CGPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}
// SAFETY: see above.
unsafe impl Encode for CGSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}
// SAFETY: see above.
unsafe impl Encode for CGRect {
    const ENCODING: Encoding = Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
}

type CGLContextObj = *mut c_void;

#[link(name = "OpenGL", kind = "framework")]
extern "C" {
    fn CGLGetCurrentContext() -> CGLContextObj;
    fn CGLSetCurrentContext(ctx: CGLContextObj) -> c_int;
    fn CGLFlushDrawable(ctx: CGLContextObj) -> c_int;
    fn CGLSetParameter(ctx: CGLContextObj, pname: c_int, params: *const c_int) -> c_int;
}
const KCGLCP_SWAP_INTERVAL: c_int = 222;

extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    static _dispatch_main_q: c_void;
    fn dispatch_async_f(queue: *const c_void, ctx: *mut c_void, work: extern "C" fn(*mut c_void));
}
/// `RTLD_DEFAULT` on macOS.
const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

// NSOpenGLPixelFormatAttribute values.
const PFA_DOUBLE_BUFFER: u32 = 5;
const PFA_ACCELERATED: u32 = 73;
const PFA_ALLOW_OFFLINE_RENDERERS: u32 = 96;
const PFA_OPENGL_PROFILE: u32 = 99;
const PROFILE_3_2_CORE: u32 = 0x3200;
// NSAutoresizingMaskOptions: width and height follow the parent, margins stay fixed.
const WIDTH_SIZABLE: usize = 2;
const HEIGHT_SIZABLE: usize = 16;
/// NSWindowAbove.
const ORDER_ABOVE: isize = 1;

unsafe extern "C" fn get_proc_address(_ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    dlsym(RTLD_DEFAULT, name)
}

/// State shared with mpv's update callback. Only touched on the main thread, except
/// `scheduled`.
struct Shared {
    scheduled: AtomicBool,
    inner: Mutex<Inner>,
}

struct Inner {
    /// Dropped before `player` (field order): the render context must go first.
    render: Option<RenderContext>,
    player: Option<Arc<Player>>,
    cgl: CGLContextObj,
    /// Drawable size in pixels.
    px: (i32, i32),
    shown: bool,
}

// SAFETY: `cgl` and the render context are only used on the main thread (see module
// docs); the mpv callback only touches the atomic and the dispatch queue.
unsafe impl Send for Shared {}
// SAFETY: see above.
unsafe impl Sync for Shared {}

impl Shared {
    fn draw(&self, force: bool) {
        let inner = self.inner.lock().unwrap();
        let Some(rc) = inner.render.as_ref() else {
            return;
        };
        // Always ask, even when not drawing: mpv expects update() after each callback.
        let frame = rc.needs_frame();
        if !(frame || force) {
            return;
        }
        let (w, h) = inner.px;
        let visible = inner.shown && w > 0 && h > 0;
        // Hidden (Keşfet, lobby, music scene): still consume the frame. With the render
        // API mpv waits up to 200 ms per frame for a render + swap; skipping them while
        // hidden made playback crawl and the room saw this member as buffering.
        if !visible && !frame {
            return;
        }
        // SAFETY: our context, on the main thread.
        unsafe { CGLSetCurrentContext(inner.cgl) };
        if let Err(e) = rc.render_gl(0, w.max(1), h.max(1)) {
            eprintln!("[koni] video çizilemedi: {e}");
            return;
        }
        if visible {
            // SAFETY: as above.
            unsafe { CGLFlushDrawable(inner.cgl) };
        }
        rc.report_swap();
    }
}

extern "C" fn draw_on_main(ctx: *mut c_void) {
    // SAFETY: `ctx` is the strong count added in `on_mpv_update`.
    let shared = unsafe { Arc::from_raw(ctx as *const Shared) };
    shared.scheduled.store(false, Ordering::Release);
    shared.draw(false);
}

/// mpv's render update callback (any thread): queue one redraw on the main thread.
unsafe extern "C" fn on_mpv_update(data: *mut c_void) {
    let shared = &*(data as *const Shared);
    if !shared.scheduled.swap(true, Ordering::AcqRel) {
        Arc::increment_strong_count(data as *const Shared);
        dispatch_async_f(&_dispatch_main_q as *const c_void, data, draw_on_main);
    }
}

/// `NSOpenGLView` video surface for macOS. All methods must run on the main thread.
pub struct MacVideoSurface {
    view: Option<Retained<AnyObject>>,
    parent: Retained<AnyObject>,
    gl: Retained<AnyObject>,
    layout: LayoutConfig,
    /// Layout to restore when leaving fullscreen.
    saved: Option<LayoutConfig>,
    visible: bool,
    shared: Arc<Shared>,
}

// SAFETY: the AppKit objects are only used on the main thread (documented
// requirement, same as the Win32 surface); `AppState` just stores the value.
unsafe impl Send for MacVideoSurface {}
// SAFETY: see above.
unsafe impl Sync for MacVideoSurface {}

impl MacVideoSurface {
    /// Creates the libmpv render context (the player must use `vo=libmpv`) and starts
    /// drawing. Call once, after the player is initialized.
    pub fn attach_player(&mut self, player: Arc<Player>) -> Result<(), SurfaceError> {
        let mut inner = self.shared.inner.lock().unwrap();
        // SAFETY: our context, on the main thread.
        unsafe { CGLSetCurrentContext(inner.cgl) };
        let rc = player
            .create_render_gl(get_proc_address, std::ptr::null_mut())
            .map_err(|e| SurfaceError::Os {
                call: "mpv_render_context_create",
                message: e.to_string(),
            })?;
        // SAFETY: `Shared` lives in an Arc that outlives the render context (it owns it).
        unsafe { rc.set_update_callback(on_mpv_update, Arc::as_ptr(&self.shared) as *mut c_void) };
        inner.render = Some(rc);
        inner.player = Some(player);
        Ok(())
    }

    /// Creates the surface in `ns_window`'s content view, above the webview.
    ///
    /// # Safety
    /// `ns_window` must be a live `NSWindow` (or null); call on the main thread.
    pub unsafe fn create_in_window(
        ns_window: *mut c_void,
        layout: &LayoutConfig,
    ) -> Result<Self, SurfaceError> {
        let os = |call: &'static str| SurfaceError::Os {
            call,
            message: "nil".into(),
        };
        // SAFETY: caller guarantees a live NSWindow on the main thread.
        unsafe {
            let window = (ns_window as *mut AnyObject)
                .as_ref()
                .ok_or(os("NSWindow"))?;
            let parent: Option<Retained<AnyObject>> = msg_send![window, contentView];
            let parent = parent.ok_or(os("contentView"))?;
            // Hardware first; without a GPU (virtual machines) fall back to Apple's
            // software renderer instead of having no video at all.
            let pixel_format = |accelerated: bool| -> Option<Retained<AnyObject>> {
                let mut attrs = vec![PFA_OPENGL_PROFILE, PROFILE_3_2_CORE, PFA_DOUBLE_BUFFER];
                if accelerated {
                    attrs.extend([PFA_ACCELERATED, PFA_ALLOW_OFFLINE_RENDERERS]);
                }
                attrs.push(0);
                msg_send![
                    msg_send![class!(NSOpenGLPixelFormat), alloc],
                    initWithAttributes: attrs.as_ptr()
                ]
            };
            let pf = pixel_format(true)
                .or_else(|| {
                    eprintln!("[koni] donanım OpenGL yok, yazılım çiziciye geçiliyor");
                    pixel_format(false)
                })
                .ok_or(os("NSOpenGLPixelFormat"))?;
            let view: Option<Retained<AnyObject>> = msg_send![
                msg_send![class!(NSOpenGLView), alloc],
                initWithFrame: CGRect::default(),
                pixelFormat: &*pf
            ];
            let view = view.ok_or(os("NSOpenGLView"))?;
            let _: () = msg_send![&*view, setWantsBestResolutionOpenGLSurface: true];
            let _: () = msg_send![&*view, setAutoresizingMask: WIDTH_SIZABLE | HEIGHT_SIZABLE];
            let none: Option<&AnyObject> = None;
            let _: () =
                msg_send![&*parent, addSubview: &*view, positioned: ORDER_ABOVE, relativeTo: none];
            let gl: Option<Retained<AnyObject>> = msg_send![&*view, openGLContext];
            let gl = gl.ok_or(os("openGLContext"))?;
            let _: () = msg_send![&*gl, makeCurrentContext];
            let cgl = CGLGetCurrentContext();
            if cgl.is_null() {
                return Err(os("CGLGetCurrentContext"));
            }
            let zero: c_int = 0;
            CGLSetParameter(cgl, KCGLCP_SWAP_INTERVAL, &zero);
            let mut s = MacVideoSurface {
                view: Some(view),
                parent,
                gl,
                layout: layout.clone(),
                saved: None,
                visible: true,
                shared: Arc::new(Shared {
                    scheduled: AtomicBool::new(false),
                    inner: Mutex::new(Inner {
                        render: None,
                        player: None,
                        cgl,
                        px: (0, 0),
                        shown: false,
                    }),
                }),
            };
            s.relayout();
            Ok(s)
        }
    }

    fn relayout(&mut self) {
        let Some(view) = self.view.as_ref() else {
            return;
        };
        // SAFETY: plain AppKit calls on live objects, main thread.
        unsafe {
            let bounds: CGRect = msg_send![&*self.parent, bounds];
            let flipped: bool = msg_send![&*self.parent, isFlipped];
            let (pw, ph) = (bounds.size.w, bounds.size.h);
            let r = video_rect(pw.round() as i32, ph.round() as i32, 1.0, &self.layout);
            let y = if flipped {
                r.y as f64
            } else {
                ph - r.y as f64 - r.h as f64
            };
            let frame = CGRect {
                origin: CGPoint { x: r.x as f64, y },
                size: CGSize {
                    w: r.w as f64,
                    h: r.h as f64,
                },
            };
            let _: () = msg_send![&**view, setFrame: frame];
            let shown =
                self.visible && self.layout.mode != LayoutMode::Hidden && r.w > 0 && r.h > 0;
            let _: () = msg_send![&**view, setHidden: !shown];
            let vb: CGRect = msg_send![&**view, bounds];
            let px: CGRect = msg_send![&**view, convertRectToBacking: vb];
            {
                let mut inner = self.shared.inner.lock().unwrap();
                inner.px = (px.size.w.round() as i32, px.size.h.round() as i32);
                inner.shown = shown;
            }
            let _: () = msg_send![&*self.gl, update];
        }
        self.shared.draw(true);
    }

    fn teardown(&mut self) {
        {
            let mut inner = self.shared.inner.lock().unwrap();
            // SAFETY: free the render context with its GL context current.
            unsafe { CGLSetCurrentContext(inner.cgl) };
            inner.render = None;
            inner.player = None;
        }
        if let Some(view) = self.view.take() {
            // SAFETY: detach our own view, main thread.
            unsafe {
                let _: () = msg_send![&*self.gl, clearDrawable];
                let _: () = msg_send![&*view, removeFromSuperview];
            }
        }
    }
}

impl VideoSurface for MacVideoSurface {
    /// The window's handle view is not necessarily its content view (wry replaces it),
    /// so the surface is placed through the `NSWindow`.
    fn create(parent: RawWindowHandle, layout: &LayoutConfig) -> Result<Self, SurfaceError> {
        let RawWindowHandle::AppKit(h) = parent else {
            return Err(SurfaceError::UnsupportedHandle(format!("{parent:?}")));
        };
        // SAFETY: `ns_view` is a live NSView; `window` may be nil.
        let window: *mut AnyObject =
            unsafe { msg_send![h.ns_view.as_ptr() as *mut AnyObject, window] };
        // SAFETY: a live NSWindow or null (checked inside).
        unsafe { Self::create_in_window(window as *mut c_void, layout) }
    }
    fn mpv_target(&self) -> MpvTarget {
        MpvTarget::RenderApi
    }

    fn apply_layout(&mut self, layout: &LayoutConfig) {
        match self.saved.as_mut() {
            Some(saved) => *saved = layout.clone(),
            None => self.layout = layout.clone(),
        }
        self.relayout();
    }

    /// Sizes come from the parent view itself (points), so the arguments are unused.
    fn on_parent_resized(&mut self, _client_w: i32, _client_h: i32, _scale: f64) {
        self.relayout();
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        self.relayout();
    }

    /// Layout only: the window itself goes fullscreen through Tauri (`set_fullscreen`).
    fn enter_fullscreen(&mut self) -> Result<(), SurfaceError> {
        if self.saved.is_none() {
            self.saved = Some(self.layout.clone());
            self.layout.mode = LayoutMode::Fullscreen;
            self.relayout();
        }
        Ok(())
    }

    fn exit_fullscreen(&mut self) -> Result<(), SurfaceError> {
        if let Some(saved) = self.saved.take() {
            self.layout = saved;
            self.relayout();
        }
        Ok(())
    }

    fn destroy(mut self) {
        self.teardown();
    }
}

impl Drop for MacVideoSurface {
    fn drop(&mut self) {
        self.teardown();
    }
}

/// Name of the GL function loader's library, for diagnostics.
pub fn gl_vendor() -> Option<String> {
    // SAFETY: glGetString is safe to call with a current context; null otherwise.
    unsafe {
        let f = dlsym(RTLD_DEFAULT, c"glGetString".as_ptr());
        if f.is_null() {
            return None;
        }
        let f: extern "C" fn(u32) -> *const c_char = std::mem::transmute(f);
        let p = f(0x1F01); // GL_RENDERER
        (!p.is_null()).then(|| CStr::from_ptr(p).to_string_lossy().into_owned())
    }
}
