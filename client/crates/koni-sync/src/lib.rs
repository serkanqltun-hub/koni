//! # koni-sync
//!
//! KONİ istemci senkron motoru. Platformdan bağımsız, saf Rust.
//!
//! - [`clock`]: ping/pong ile sunucu saat ofseti tahmini.
//! - [`controller`]: 50 ms – 3.0 s sapmayı 0.96–1.04 hızla kapatan kontrolcü.
//! - [`buffer`]: stall algılama, 12 s histerezis rezervi, "4 dk'da 3 stall" sayacı.
//! - [`engine`]: hepsini birleştiren istemci state makinesi (docs/design/sync.md §4.4–4.7).
//! - [`protocol`]: WebSocket mesajları (docs/PROTOCOL.md).
//! - [`player`]: oynatıcı soyutlaması (gerçeği libmpv).
//! - [`sim`]: testler için simüle oynatıcı ve ağ.
//!
//! Temel kural: motor sapmayı düzeltmek için **asla geri seek yapmaz**.

pub mod buffer;
pub mod clock;
pub mod config;
pub mod controller;
pub mod engine;
pub mod player;
pub mod protocol;
pub mod sim;

pub use config::SyncConfig;
pub use engine::{EngineEvent, LocalState, SyncEngine, Telemetry};
pub use player::Player;
