use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("geçersiz URL: {0}")]
    InvalidUrl(String),
    #[error("bu site henüz desteklenmiyor: {0}")]
    Unsupported(String),
    #[error("yt-dlp bulunamadı ({0})")]
    YtDlpNotFound(String),
    #[error("yt-dlp {0} sn içinde yanıt vermedi")]
    Timeout(u64),
    #[error("video kullanılamıyor (silinmiş, gizli veya mevcut değil)")]
    Unavailable,
    #[error("video özel veya giriş gerektiriyor")]
    Private,
    #[error("video bu bölgede engelli")]
    GeoBlocked,
    #[error("yaş sınırlı video, giriş gerektiriyor")]
    AgeRestricted,
    #[error("canlı yayınlar senkron izlemede desteklenmiyor")]
    LiveUnsupported,
    #[error("yt-dlp başarısız oldu: {0}")]
    Failed(String),
    #[error("yt-dlp çıktısı anlaşılamadı: {0}")]
    BadOutput(String),
    #[error("G/Ç hatası: {0}")]
    Io(#[from] std::io::Error),
}

impl ResolveError {
    /// Map yt-dlp stderr to a specific error where we recognise the message.
    pub(crate) fn from_stderr(stderr: &str) -> Self {
        let s = stderr.to_ascii_lowercase();
        if s.contains("private video") || s.contains("members-only") {
            Self::Private
        } else if s.contains("confirm your age")
            || s.contains("age-restricted")
            || s.contains("age restricted")
        {
            Self::AgeRestricted
        } else if s.contains("in your country")
            || s.contains("geo restrict")
            || s.contains("blocked it in your country")
        {
            Self::GeoBlocked
        } else if s.contains("video unavailable")
            || s.contains("has been removed")
            || s.contains("does not exist")
            || s.contains("this video is not available")
        {
            Self::Unavailable
        } else {
            let last = stderr
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("bilinmeyen hata");
            Self::Failed(last.trim().to_owned())
        }
    }
}
