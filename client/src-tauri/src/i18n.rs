//! Arayüz dili (Türkçe / İngilizce). Arayüz açılışta ve dil değişince `set_lang`
//! çağırır; Rust'tan kullanıcıya giden metinler (hata, mpv ekran yazısı, etkinlik)
//! `tr!` / `trs` ile iki dilde yazılır.

use std::sync::atomic::{AtomicBool, Ordering};

static EN: AtomicBool = AtomicBool::new(false);

pub fn set_en(on: bool) {
    EN.store(on, Ordering::Relaxed);
}

pub fn en() -> bool {
    EN.load(Ordering::Relaxed)
}

/// Sabit metin: `trs("Bağlantı yok", "No connection")`.
pub fn trs(tr: &'static str, en: &'static str) -> &'static str {
    if self::en() {
        en
    } else {
        tr
    }
}

/// Biçimli metin, `format!` gibi: `tr!("{n} kişi", "{n} people")` → `String`.
macro_rules! tr {
    ($tr:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::en() { format!($en $(, $arg)*) } else { format!($tr $(, $arg)*) }
    };
}
