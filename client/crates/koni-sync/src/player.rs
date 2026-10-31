//! Oynatıcı soyutlaması. Gerçek implementasyon libmpv'dir (koni-player crate'i),
//! testlerde `sim::SimPlayer` kullanılır.

/// Motorun oynatıcıdan okuduğu ve oynatıcıya yaptırdığı her şey.
///
/// mpv karşılıkları: `time-pos`, `pause`, `demuxer-cache-duration`,
/// `paused-for-cache`, `duration`, `eof-reached`, `set speed`,
/// `seek <abs> absolute+exact`, `show-text`.
pub trait Player {
    /// O anki medya konumu (s).
    fn position(&self) -> f64;
    /// Oynatıcı duraklatılmış mı (kullanıcı/motor pause'u; cache beklemesi değil).
    fn is_paused(&self) -> bool;
    /// Konumun önünde tamponlanmış süre (s).
    fn cache_ahead(&self) -> f64;
    /// mpv cache boşaldığı için kendiliğinden bekliyor mu.
    fn paused_for_cache(&self) -> bool;
    /// Medya süresi (s). Canlı akışta veya bilinmiyorsa `None`.
    fn duration(&self) -> Option<f64>;
    /// Dosya sonuna gelindi mi.
    fn eof_reached(&self) -> bool;

    fn set_pause(&mut self, paused: bool);
    fn set_speed(&mut self, speed: f64);
    /// Mutlak ve kare hassasiyetinde seek (`absolute+exact`).
    fn seek_exact(&mut self, pos: f64);
    /// Tam ekranda OSD mesajı. Varsayılan: hiçbir şey yapma.
    fn show_text(&mut self, _text: &str) {}
}
