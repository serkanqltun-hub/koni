//! Minimal hand-written FFI for mpv/client.h (client API v2), loaded at runtime.

use std::ffi::{c_char, c_double, c_int, c_void};
use std::sync::Arc;

use libloading::Library;

/// Opaque `mpv_handle`.
#[repr(C)]
pub struct MpvHandle {
    _private: [u8; 0],
}

// enum mpv_format
pub const MPV_FORMAT_NONE: c_int = 0;
pub const MPV_FORMAT_STRING: c_int = 1;
pub const MPV_FORMAT_FLAG: c_int = 3;
pub const MPV_FORMAT_DOUBLE: c_int = 5;

// enum mpv_event_id
pub const MPV_EVENT_NONE: c_int = 0;
pub const MPV_EVENT_SHUTDOWN: c_int = 1;
pub const MPV_EVENT_END_FILE: c_int = 7;
pub const MPV_EVENT_FILE_LOADED: c_int = 8;
pub const MPV_EVENT_SEEK: c_int = 20;
pub const MPV_EVENT_PLAYBACK_RESTART: c_int = 21;
pub const MPV_EVENT_PROPERTY_CHANGE: c_int = 22;

/// `mpv_event`.
#[repr(C)]
pub struct MpvEvent {
    pub event_id: c_int,
    pub error: c_int,
    pub reply_userdata: u64,
    pub data: *mut c_void,
}

/// `mpv_event_property`.
#[repr(C)]
pub struct MpvEventProperty {
    pub name: *const c_char,
    pub format: c_int,
    pub data: *mut c_void,
}

type FnCreate = unsafe extern "C" fn() -> *mut MpvHandle;
type FnInitialize = unsafe extern "C" fn(*mut MpvHandle) -> c_int;
type FnTerminateDestroy = unsafe extern "C" fn(*mut MpvHandle);
type FnSetStr = unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int;
type FnGetProperty =
    unsafe extern "C" fn(*mut MpvHandle, *const c_char, c_int, *mut c_void) -> c_int;
type FnCommand = unsafe extern "C" fn(*mut MpvHandle, *mut *const c_char) -> c_int;
type FnObserve = unsafe extern "C" fn(*mut MpvHandle, u64, *const c_char, c_int) -> c_int;
type FnWaitEvent = unsafe extern "C" fn(*mut MpvHandle, c_double) -> *mut MpvEvent;
type FnErrorString = unsafe extern "C" fn(c_int) -> *const c_char;
type FnFree = unsafe extern "C" fn(*mut c_void);

/// Resolved libmpv entry points. `_lib` keeps the library mapped for the pointers' lifetime.
pub struct Api {
    _lib: Arc<Library>,
    pub create: FnCreate,
    pub initialize: FnInitialize,
    pub terminate_destroy: FnTerminateDestroy,
    pub set_option_string: FnSetStr,
    pub set_property_string: FnSetStr,
    pub get_property: FnGetProperty,
    pub command: FnCommand,
    pub observe_property: FnObserve,
    pub wait_event: FnWaitEvent,
    pub error_string: FnErrorString,
    pub free: FnFree,
}

impl Api {
    /// Resolves all symbols from an already loaded library.
    ///
    /// # Safety
    /// `lib` must be a genuine libmpv (client API v2).
    pub unsafe fn load(lib: Arc<Library>) -> Result<Api, libloading::Error> {
        macro_rules! sym {
            ($name:literal) => {
                *lib.get($name)?
            };
        }
        Ok(Api {
            create: sym!("mpv_create"),
            initialize: sym!("mpv_initialize"),
            terminate_destroy: sym!("mpv_terminate_destroy"),
            set_option_string: sym!("mpv_set_option_string"),
            set_property_string: sym!("mpv_set_property_string"),
            get_property: sym!("mpv_get_property"),
            command: sym!("mpv_command"),
            observe_property: sym!("mpv_observe_property"),
            wait_event: sym!("mpv_wait_event"),
            error_string: sym!("mpv_error_string"),
            free: sym!("mpv_free"),
            _lib: lib,
        })
    }
}
