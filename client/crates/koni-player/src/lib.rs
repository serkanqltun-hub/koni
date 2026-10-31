//! `koni-player`: a thin libmpv wrapper. libmpv is loaded at runtime with
//! `libloading`; nothing is linked at build time.

mod ffi;
mod render;

pub use render::{GetProcAddress, RenderContext};

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;

use ffi::{Api, MpvEventProperty, MpvHandle};

/// Options for [`Player::new`].
#[derive(Clone, Debug, Default)]
pub struct PlayerOptions {
    /// Native window id to embed into (`wid`), e.g. a Win32 HWND.
    pub wid: Option<i64>,
    /// Explicit libmpv path. When set, only this path is tried.
    pub lib_path: Option<PathBuf>,
    /// Extra mpv options applied after the defaults (so they can override them).
    pub extra_options: Vec<(String, String)>,
}

/// Errors returned by the player.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerError {
    /// libmpv could not be loaded or a symbol is missing.
    LibraryNotFound(String),
    /// libmpv returned an error code.
    Mpv { code: i32, message: String },
    /// A string contained an interior NUL byte.
    InvalidString,
}

impl std::fmt::Display for PlayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlayerError::LibraryNotFound(s) => write!(f, "libmpv not found: {s}"),
            PlayerError::Mpv { code, message } => write!(f, "mpv error {code}: {message}"),
            PlayerError::InvalidString => write!(f, "string contains a NUL byte"),
        }
    }
}

impl std::error::Error for PlayerError {}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, PlayerError>;

/// Value of an observed property.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PropValue {
    None,
    Flag(bool),
    Double(f64),
    String(String),
}

/// Events returned by [`Player::wait_event`].
#[derive(Clone, Debug, PartialEq)]
pub enum PlayerEvent {
    PropertyChanged {
        name: String,
        value: PropValue,
    },
    FileLoaded,
    EndFile,
    Seek,
    PlaybackRestart,
    Shutdown,
    /// Any other `mpv_event_id`.
    Other(i32),
}

/// Point-in-time playback state.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    pub time_pos: Option<f64>,
    pub duration: Option<f64>,
    pub paused: bool,
    pub speed: f64,
    pub seeking: bool,
    pub paused_for_cache: bool,
    pub eof_reached: bool,
}

/// Defaults from docs/design/client.md §3.5 plus embedding-friendly input settings.
const DEFAULT_OPTIONS: &[(&str, &str)] = &[
    ("hwdec", "auto-safe"),
    ("vo", "gpu-next"),
    ("hr-seek", "yes"),
    ("audio-pitch-correction", "yes"),
    ("keep-open", "yes"),
    ("cache", "yes"),
    ("demuxer-max-bytes", "200MiB"),
    ("demuxer-readahead-secs", "30"),
    ("input-default-bindings", "no"),
    ("input-vo-keyboard", "no"),
    // Kaynakta Türkçe altyazı varsa kendiliğinden açılsın (ses de Türkçeyse açılmaz).
    ("slang", "tr,tur"),
];

/// Options that only exist when mpv is built with Lua (the OSC is a Lua script). The LGPL
/// build has no scripting, so "option not found" is fine there.
const SCRIPT_OPTIONS: &[(&str, &str)] = &[("osc", "no"), ("cache-secs", "30")];
const MPV_ERROR_OPTION_NOT_FOUND: c_int = -5;

const OBSERVED: &[(&str, c_int)] = &[
    ("time-pos", ffi::MPV_FORMAT_DOUBLE),
    ("pause", ffi::MPV_FORMAT_FLAG),
    ("seeking", ffi::MPV_FORMAT_FLAG),
    ("paused-for-cache", ffi::MPV_FORMAT_FLAG),
    ("eof-reached", ffi::MPV_FORMAT_FLAG),
    ("speed", ffi::MPV_FORMAT_DOUBLE),
    ("duration", ffi::MPV_FORMAT_DOUBLE),
    ("demuxer-cache-duration", ffi::MPV_FORMAT_DOUBLE),
];

#[cfg(windows)]
const LIB_CANDIDATES: &[&str] = &["libmpv-2.dll", "mpv-2.dll"];
#[cfg(target_os = "macos")]
const LIB_CANDIDATES: &[&str] = &["libmpv.2.dylib", "libmpv.dylib"];
#[cfg(all(unix, not(target_os = "macos")))]
const LIB_CANDIDATES: &[&str] = &["libmpv.so.2", "libmpv.so"];

fn load_library(lib_path: Option<&PathBuf>) -> Result<Arc<libloading::Library>> {
    let candidates: Vec<PathBuf> = match lib_path {
        Some(p) => vec![p.clone()],
        None => LIB_CANDIDATES.iter().map(PathBuf::from).collect(),
    };
    let mut errors = Vec::new();
    for c in &candidates {
        // SAFETY: loading libmpv runs its initializers, which is expected.
        match unsafe { libloading::Library::new(c) } {
            Ok(lib) => return Ok(Arc::new(lib)),
            Err(e) => errors.push(format!("{}: {e}", c.display())),
        }
    }
    Err(PlayerError::LibraryNotFound(errors.join("; ")))
}

fn cstr(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| PlayerError::InvalidString)
}

/// A libmpv instance.
pub struct Player {
    api: Api,
    handle: *mut MpvHandle,
    /// A `lavfi-complex` graph is set for the current file; the next load clears it.
    lavfi: AtomicBool,
}

// SAFETY: the libmpv client API is thread-safe for a single mpv_handle (all
// functions may be called concurrently), except `mpv_wait_event`, which must
// only be called from one thread at a time; that is documented on `wait_event`.
unsafe impl Send for Player {}
// SAFETY: see above.
unsafe impl Sync for Player {}

impl Player {
    /// Loads libmpv, creates a handle, applies options and initializes it.
    pub fn new(opts: PlayerOptions) -> Result<Player> {
        let lib = load_library(opts.lib_path.as_ref())?;
        // SAFETY: the library was found under a libmpv name/path.
        let api = unsafe { Api::load(lib) }
            .map_err(|e| PlayerError::LibraryNotFound(format!("missing symbol: {e}")))?;
        // SAFETY: plain constructor call.
        let handle = unsafe { (api.create)() };
        if handle.is_null() {
            return Err(PlayerError::Mpv {
                code: -1,
                message: "mpv_create failed".into(),
            });
        }
        // From here on, Drop takes care of mpv_terminate_destroy.
        let player = Player {
            api,
            handle,
            lavfi: AtomicBool::new(false),
        };

        if let Some(wid) = opts.wid {
            player.set_option("wid", &wid.to_string())?;
        }
        for (k, v) in DEFAULT_OPTIONS {
            player.set_option(k, v)?;
        }
        for (k, v) in SCRIPT_OPTIONS {
            match player.set_option(k, v) {
                Err(PlayerError::Mpv {
                    code: MPV_ERROR_OPTION_NOT_FOUND,
                    ..
                }) => {}
                r => r?,
            }
        }
        for (k, v) in &opts.extra_options {
            player.set_option(k, v)?;
        }
        // SAFETY: valid handle.
        player.check(unsafe { (player.api.initialize)(player.handle) })?;
        for (name, format) in OBSERVED {
            let n = cstr(name)?;
            // SAFETY: valid handle and NUL-terminated name.
            player.check(unsafe {
                (player.api.observe_property)(player.handle, 0, n.as_ptr(), *format)
            })?;
        }
        Ok(player)
    }

    fn check(&self, code: c_int) -> Result<()> {
        if code >= 0 {
            return Ok(());
        }
        // SAFETY: mpv_error_string returns a static string for any code.
        let p = unsafe { (self.api.error_string)(code) };
        let message = if p.is_null() {
            "unknown error".to_string()
        } else {
            // SAFETY: non-null static C string.
            unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
        };
        Err(PlayerError::Mpv { code, message })
    }

    fn set_option(&self, name: &str, value: &str) -> Result<()> {
        let (n, v) = (cstr(name)?, cstr(value)?);
        // SAFETY: valid handle and C strings.
        self.check(unsafe { (self.api.set_option_string)(self.handle, n.as_ptr(), v.as_ptr()) })
    }

    /// Loads a file or URL, replacing the current one.
    pub fn load(&self, path_or_url: &str) -> Result<()> {
        self.clear_lavfi();
        self.command(&["loadfile", path_or_url, "replace"])
    }

    /// Sets a `lavfi-complex` graph (e.g. an audio visualizer) for the current file
    /// only: `load`/`load_stream` clear it, so the next file plays normally.
    pub fn set_lavfi_complex(&self, graph: &str) -> Result<()> {
        self.set_property_string("lavfi-complex", graph)?;
        self.lavfi.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn clear_lavfi(&self) {
        if self.lavfi.swap(false, Ordering::Relaxed) {
            let _ = self.set_property_string("lavfi-complex", "");
        }
    }

    /// Loads a resolved web stream: separate audio track, display title and any `extra`
    /// per-file options such as `referrer`/`user-agent` (mpv ≥ 0.38
    /// `loadfile <url> replace -1 <options>`).
    pub fn load_stream(
        &self,
        video_url: &str,
        audio_url: Option<&str>,
        title: &str,
        extra: &[(&str, &str)],
    ) -> Result<()> {
        // `%N%` byte-length prefix: values may contain commas or `=` (signed URLs).
        let quote = |v: &str| format!("%{}%{v}", v.len());
        let mut opts = format!("force-media-title={}", quote(title));
        if let Some(a) = audio_url {
            opts.push_str(",audio-files-append=");
            opts.push_str(&quote(a));
        }
        for (k, v) in extra {
            opts.push_str(&format!(",{k}={}", quote(v)));
        }
        self.clear_lavfi();
        self.command(&["loadfile", video_url, "replace", "-1", &opts])
    }

    /// Unpauses playback.
    pub fn play(&self) -> Result<()> {
        self.set_pause(false)
    }

    /// Pauses playback.
    pub fn pause(&self) -> Result<()> {
        self.set_pause(true)
    }

    /// Sets the `pause` property.
    pub fn set_pause(&self, p: bool) -> Result<()> {
        self.set_property_string("pause", if p { "yes" } else { "no" })
    }

    /// Toggles the `pause` property.
    pub fn toggle_pause(&self) -> Result<()> {
        self.command(&["cycle", "pause"])
    }

    /// Exact absolute seek (`seek <s> absolute+exact`).
    pub fn seek_absolute(&self, seconds: f64) -> Result<()> {
        self.command(&["seek", &seconds.to_string(), "absolute+exact"])
    }

    /// Sets playback speed.
    pub fn set_speed(&self, speed: f64) -> Result<()> {
        self.set_property_string("speed", &speed.to_string())
    }

    fn get_double(&self, name: &str) -> Option<f64> {
        let n = cstr(name).ok()?;
        let mut out: f64 = 0.0;
        // SAFETY: MPV_FORMAT_DOUBLE writes a double into `out`.
        let rc = unsafe {
            (self.api.get_property)(
                self.handle,
                n.as_ptr(),
                ffi::MPV_FORMAT_DOUBLE,
                &mut out as *mut f64 as *mut c_void,
            )
        };
        (rc >= 0).then_some(out)
    }

    fn get_flag(&self, name: &str) -> Option<bool> {
        let n = cstr(name).ok()?;
        let mut out: c_int = 0;
        // SAFETY: MPV_FORMAT_FLAG writes an int into `out`.
        let rc = unsafe {
            (self.api.get_property)(
                self.handle,
                n.as_ptr(),
                ffi::MPV_FORMAT_FLAG,
                &mut out as *mut c_int as *mut c_void,
            )
        };
        (rc >= 0).then_some(out != 0)
    }

    /// Current position in seconds.
    pub fn time_pos(&self) -> Option<f64> {
        self.get_double("time-pos")
    }

    /// Media duration in seconds, if known.
    pub fn duration(&self) -> Option<f64> {
        self.get_double("duration")
    }

    /// Whether `pause` is set.
    pub fn is_paused(&self) -> bool {
        self.get_flag("pause").unwrap_or(false)
    }

    /// Current playback speed (1.0 if unavailable).
    pub fn speed(&self) -> f64 {
        self.get_double("speed").unwrap_or(1.0)
    }

    /// Seconds buffered ahead by the demuxer (0.0 if unavailable).
    pub fn demuxer_cache_duration(&self) -> f64 {
        self.get_double("demuxer-cache-duration").unwrap_or(0.0)
    }

    /// Whether playback is stalled waiting for cache.
    pub fn paused_for_cache(&self) -> bool {
        self.get_flag("paused-for-cache").unwrap_or(false)
    }

    /// Whether a seek is currently in progress.
    pub fn is_seeking(&self) -> bool {
        self.get_flag("seeking").unwrap_or(false)
    }

    /// Whether the end of the file was reached (with keep-open).
    pub fn eof_reached(&self) -> bool {
        self.get_flag("eof-reached").unwrap_or(false)
    }

    /// Shows OSD text for `duration_ms`.
    pub fn show_text(&self, text: &str, duration_ms: u32) -> Result<()> {
        self.command(&["show-text", text, &duration_ms.to_string()])
    }

    /// Runs a raw mpv command.
    pub fn command(&self, args: &[&str]) -> Result<()> {
        let owned = args.iter().map(|a| cstr(a)).collect::<Result<Vec<_>>>()?;
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(ptr::null());
        // SAFETY: NULL-terminated array of valid C strings that outlive the call.
        self.check(unsafe { (self.api.command)(self.handle, ptrs.as_mut_ptr()) })
    }

    /// Sets a property from a string.
    pub fn set_property_string(&self, name: &str, value: &str) -> Result<()> {
        let (n, v) = (cstr(name)?, cstr(value)?);
        // SAFETY: valid handle and C strings.
        self.check(unsafe { (self.api.set_property_string)(self.handle, n.as_ptr(), v.as_ptr()) })
    }

    /// Reads a property as a string.
    pub fn get_property_string(&self, name: &str) -> Option<String> {
        let n = cstr(name).ok()?;
        let mut out: *mut c_char = ptr::null_mut();
        // SAFETY: MPV_FORMAT_STRING writes a `char*` that we must mpv_free.
        let rc = unsafe {
            (self.api.get_property)(
                self.handle,
                n.as_ptr(),
                ffi::MPV_FORMAT_STRING,
                &mut out as *mut *mut c_char as *mut c_void,
            )
        };
        if rc < 0 || out.is_null() {
            return None;
        }
        // SAFETY: mpv returned a valid C string; freed right after copying.
        let s = unsafe { CStr::from_ptr(out) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.api.free)(out as *mut c_void) };
        Some(s)
    }

    /// Waits up to `timeout_secs` for the next event; `None` on timeout.
    /// Must only be called from a single thread.
    pub fn wait_event(&self, timeout_secs: f64) -> Option<PlayerEvent> {
        // SAFETY: valid handle; the returned event is valid until the next call.
        let ev = unsafe { (self.api.wait_event)(self.handle, timeout_secs) };
        if ev.is_null() {
            return None;
        }
        // SAFETY: non-null pointer to an mpv_event owned by mpv.
        let ev = unsafe { &*ev };
        Some(match ev.event_id {
            ffi::MPV_EVENT_NONE => return None,
            ffi::MPV_EVENT_SHUTDOWN => PlayerEvent::Shutdown,
            ffi::MPV_EVENT_END_FILE => PlayerEvent::EndFile,
            ffi::MPV_EVENT_FILE_LOADED => PlayerEvent::FileLoaded,
            ffi::MPV_EVENT_SEEK => PlayerEvent::Seek,
            ffi::MPV_EVENT_PLAYBACK_RESTART => PlayerEvent::PlaybackRestart,
            ffi::MPV_EVENT_PROPERTY_CHANGE if !ev.data.is_null() => {
                // SAFETY: PROPERTY_CHANGE data is an mpv_event_property.
                let prop = unsafe { &*(ev.data as *const MpvEventProperty) };
                let name = if prop.name.is_null() {
                    String::new()
                } else {
                    // SAFETY: non-null C string owned by mpv.
                    unsafe { CStr::from_ptr(prop.name) }
                        .to_string_lossy()
                        .into_owned()
                };
                // SAFETY: data layout is determined by `format`.
                let value = unsafe { read_prop(prop.format, prop.data) };
                PlayerEvent::PropertyChanged { name, value }
            }
            other => PlayerEvent::Other(other),
        })
    }

    /// Reads the current state directly from mpv.
    pub fn snapshot(&self) -> PlayerSnapshot {
        PlayerSnapshot {
            time_pos: self.time_pos(),
            duration: self.duration(),
            paused: self.is_paused(),
            speed: self.speed(),
            seeking: self.get_flag("seeking").unwrap_or(false),
            paused_for_cache: self.paused_for_cache(),
            eof_reached: self.eof_reached(),
        }
    }
}

/// # Safety
/// `data` must match `format` as described in mpv/client.h.
unsafe fn read_prop(format: c_int, data: *mut c_void) -> PropValue {
    if data.is_null() {
        return PropValue::None;
    }
    match format {
        ffi::MPV_FORMAT_FLAG => PropValue::Flag(*(data as *const c_int) != 0),
        ffi::MPV_FORMAT_DOUBLE => PropValue::Double(*(data as *const f64)),
        ffi::MPV_FORMAT_STRING => {
            let s = *(data as *const *const c_char);
            if s.is_null() {
                PropValue::None
            } else {
                PropValue::String(CStr::from_ptr(s).to_string_lossy().into_owned())
            }
        }
        ffi::MPV_FORMAT_NONE => PropValue::None,
        _ => PropValue::None,
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // SAFETY: handle is valid and not used after this.
        unsafe { (self.api.terminate_destroy)(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bogus_lib_path_is_library_not_found() {
        let opts = PlayerOptions {
            lib_path: Some(PathBuf::from("/definitely/not/here/libmpv-bogus.so")),
            ..Default::default()
        };
        match Player::new(opts) {
            Err(PlayerError::LibraryNotFound(msg)) => assert!(msg.contains("libmpv-bogus")),
            Err(e) => panic!("unexpected error: {e}"),
            Ok(_) => panic!("bogus path loaded"),
        }
    }

    #[test]
    fn snapshot_serializes_camel_case() {
        let s = PlayerSnapshot {
            time_pos: Some(1.5),
            duration: None,
            paused: true,
            speed: 1.04,
            seeking: false,
            paused_for_cache: false,
            eof_reached: true,
        };
        let v: serde_json::Value = serde_json::to_value(&s).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "timePos": 1.5, "duration": null, "paused": true, "speed": 1.04,
                "seeking": false, "pausedForCache": false, "eofReached": true
            })
        );
    }

    #[test]
    fn prop_value_serializes_untagged() {
        let j = |v: PropValue| serde_json::to_string(&v).unwrap();
        assert_eq!(j(PropValue::None), "null");
        assert_eq!(j(PropValue::Flag(true)), "true");
        assert_eq!(j(PropValue::Double(2.5)), "2.5");
        assert_eq!(j(PropValue::String("x".into())), "\"x\"");
    }

    #[test]
    fn nul_in_string_is_invalid() {
        assert_eq!(cstr("a\0b"), Err(PlayerError::InvalidString));
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn ffi_layout_matches_client_h() {
        use std::mem::{offset_of, size_of};
        // mpv_event: int event_id, int error, uint64_t reply_userdata, void *data
        assert_eq!(size_of::<ffi::MpvEvent>(), 24);
        assert_eq!(offset_of!(ffi::MpvEvent, reply_userdata), 8);
        assert_eq!(offset_of!(ffi::MpvEvent, data), 16);
        // mpv_event_property: const char *name, mpv_format format, void *data
        assert_eq!(size_of::<MpvEventProperty>(), 24);
        assert_eq!(offset_of!(MpvEventProperty, data), 16);
    }
}
