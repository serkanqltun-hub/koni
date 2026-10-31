//! Stream resolution for KONİ.
//!
//! Pasted URL -> [`classify`] -> [`resolve`]. Direct media links are passed
//! through; every other http(s) page goes through a `yt-dlp -J` subprocess.
//! Sites yt-dlp cannot extract are reported as [`ResolveError::Unsupported`].

mod classify;
mod error;
mod runner;
mod search;
mod ytdlp;

pub use classify::{classify, UrlKind};
pub use error::ResolveError;
pub use runner::{CommandRunner, ProcessRunner};
pub use search::{playlist, search, Hit};
pub use ytdlp::{resolve, resolve_with, Alt, ResolvedStream, Sub, YtDlpConfig, DEFAULT_FORMAT};
