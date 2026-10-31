//! libmpv render API (`mpv/render.h`, `render_gl.h`): the host draws the video itself.
//! Used on macOS, where mpv can't embed into a foreign view via `wid`. The player must
//! be created with `vo=libmpv`.

use std::ffi::{c_char, c_int, c_void, CString};
use std::sync::Arc;

use libloading::Library;

use crate::{Player, PlayerError, Result};

/// `void *(*get_proc_address)(void *ctx, const char *name)`.
pub type GetProcAddress = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;
type UpdateCb = unsafe extern "C" fn(*mut c_void);

#[repr(C)]
struct Param {
    kind: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct GlInit {
    get_proc_address: GetProcAddress,
    ctx: *mut c_void,
}

#[repr(C)]
struct GlFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

// enum mpv_render_param_type
const P_INVALID: c_int = 0;
const P_API_TYPE: c_int = 1;
const P_OPENGL_INIT_PARAMS: c_int = 2;
const P_OPENGL_FBO: c_int = 3;
const P_FLIP_Y: c_int = 4;
const P_SW_SIZE: c_int = 17;
const P_SW_FORMAT: c_int = 18;
const P_SW_STRIDE: c_int = 19;
const P_SW_POINTER: c_int = 20;
/// `MPV_RENDER_UPDATE_FRAME`.
const UPDATE_FRAME: u64 = 1;

type FnCreate = unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *mut Param) -> c_int;
type FnSetCb = unsafe extern "C" fn(*mut c_void, Option<UpdateCb>, *mut c_void);
type FnUpdate = unsafe extern "C" fn(*mut c_void) -> u64;
type FnRender = unsafe extern "C" fn(*mut c_void, *mut Param) -> c_int;
type FnCtx = unsafe extern "C" fn(*mut c_void);

struct Fns {
    _lib: Arc<Library>,
    set_cb: FnSetCb,
    update: FnUpdate,
    render: FnRender,
    report_swap: FnCtx,
    free: FnCtx,
}

/// An `mpv_render_context`. Must be dropped before the [`Player`] it came from, on the
/// thread (and with the GL context current) it renders on.
pub struct RenderContext {
    fns: Fns,
    ctx: *mut c_void,
}

// SAFETY: the render API may be used from any single thread at a time; callers keep
// all calls on their render thread.
unsafe impl Send for RenderContext {}

fn sym<T: Copy>(lib: &Library, name: &[u8]) -> Result<T> {
    // SAFETY: the caller names a libmpv function with the matching signature.
    unsafe { lib.get::<T>(name).map(|s| *s) }
        .map_err(|e| PlayerError::LibraryNotFound(format!("missing symbol: {e}")))
}

impl Player {
    fn create_render(&self, params: &mut [Param]) -> Result<RenderContext> {
        let lib = &self.api.lib;
        let create: FnCreate = sym(lib, b"mpv_render_context_create")?;
        let fns = Fns {
            set_cb: sym(lib, b"mpv_render_context_set_update_callback")?,
            update: sym(lib, b"mpv_render_context_update")?,
            render: sym(lib, b"mpv_render_context_render")?,
            report_swap: sym(lib, b"mpv_render_context_report_swap")?,
            free: sym(lib, b"mpv_render_context_free")?,
            _lib: lib.clone(),
        };
        let mut ctx = std::ptr::null_mut();
        // SAFETY: params is terminated by P_INVALID and points at live data.
        self.check(unsafe { create(&mut ctx, self.handle as *mut c_void, params.as_mut_ptr()) })?;
        Ok(RenderContext { fns, ctx })
    }

    /// OpenGL render context. The GL context must be current on the calling thread.
    pub fn create_render_gl(
        &self,
        get_proc_address: GetProcAddress,
        proc_ctx: *mut c_void,
    ) -> Result<RenderContext> {
        let api = c"opengl";
        let mut init = GlInit {
            get_proc_address,
            ctx: proc_ctx,
        };
        let mut params = [
            Param {
                kind: P_API_TYPE,
                data: api.as_ptr() as *mut c_void,
            },
            Param {
                kind: P_OPENGL_INIT_PARAMS,
                data: &mut init as *mut GlInit as *mut c_void,
            },
            Param {
                kind: P_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        self.create_render(&mut params)
    }

    /// Software render context (CPU, for tests without a GPU).
    pub fn create_render_sw(&self) -> Result<RenderContext> {
        let api = c"sw";
        let mut params = [
            Param {
                kind: P_API_TYPE,
                data: api.as_ptr() as *mut c_void,
            },
            Param {
                kind: P_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        self.create_render(&mut params)
    }
}

impl RenderContext {
    /// `cb(data)` is called from mpv's threads when a new frame may be ready. It must
    /// only wake the render thread, never call back into mpv.
    ///
    /// # Safety
    /// `data` must stay valid until this context is dropped.
    pub unsafe fn set_update_callback(&self, cb: UpdateCb, data: *mut c_void) {
        (self.fns.set_cb)(self.ctx, Some(cb), data);
    }

    /// Whether a new video frame should be drawn.
    pub fn needs_frame(&self) -> bool {
        // SAFETY: valid context.
        unsafe { (self.fns.update)(self.ctx) & UPDATE_FRAME != 0 }
    }

    /// Draws the current frame into OpenGL framebuffer `fbo` (0 = the window), `w × h`
    /// pixels, flipped for a bottom-left origin.
    pub fn render_gl(&self, fbo: i32, w: i32, h: i32) -> Result<()> {
        let mut target = GlFbo {
            fbo,
            w,
            h,
            internal_format: 0,
        };
        let mut flip: c_int = 1;
        let mut params = [
            Param {
                kind: P_OPENGL_FBO,
                data: &mut target as *mut GlFbo as *mut c_void,
            },
            Param {
                kind: P_FLIP_Y,
                data: &mut flip as *mut c_int as *mut c_void,
            },
            Param {
                kind: P_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        self.render(&mut params)
    }

    /// Draws the current frame as 32-bit `rgb0` pixels into `buf` (`stride` bytes per row).
    pub fn render_sw(&self, w: i32, h: i32, stride: usize, buf: &mut [u8]) -> Result<()> {
        if buf.len() < stride * h.max(0) as usize || stride < 4 * w.max(0) as usize {
            return Err(PlayerError::InvalidString);
        }
        let mut size: [c_int; 2] = [w, h];
        let fmt = CString::new("rgb0").map_err(|_| PlayerError::InvalidString)?;
        let mut stride = stride;
        let mut params = [
            Param {
                kind: P_SW_SIZE,
                data: size.as_mut_ptr() as *mut c_void,
            },
            Param {
                kind: P_SW_FORMAT,
                data: fmt.as_ptr() as *mut c_void,
            },
            Param {
                kind: P_SW_STRIDE,
                data: &mut stride as *mut usize as *mut c_void,
            },
            Param {
                kind: P_SW_POINTER,
                data: buf.as_mut_ptr() as *mut c_void,
            },
            Param {
                kind: P_INVALID,
                data: std::ptr::null_mut(),
            },
        ];
        self.render(&mut params)
    }

    fn render(&self, params: &mut [Param]) -> Result<()> {
        // SAFETY: valid context, params terminated and alive for the call.
        let rc = unsafe { (self.fns.render)(self.ctx, params.as_mut_ptr()) };
        if rc < 0 {
            return Err(PlayerError::Mpv {
                code: rc,
                message: "mpv_render_context_render failed".into(),
            });
        }
        Ok(())
    }

    /// Tells mpv a frame was presented (after the buffer swap), for frame timing.
    pub fn report_swap(&self) {
        // SAFETY: valid context.
        unsafe { (self.fns.report_swap)(self.ctx) }
    }
}

impl Drop for RenderContext {
    fn drop(&mut self) {
        // SAFETY: valid context, not used after this.
        unsafe { (self.fns.free)(self.ctx) }
    }
}
