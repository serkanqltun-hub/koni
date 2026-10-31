//! Müzik (docs/design/media.md §6.3): görüntüsüz ses video alanında boş kalmasın.
//! Akışın yt-dlp kapağı varsa albüm kapağı olarak açılır; yoksa mpv `lavfi-complex`
//! ile sesten üretilen görselleştirici çalışır (ses mpv'de çaldığı için webview'da
//! Web Audio yok; görüntü sesle birebir aynı zamanda). Grafik yalnızca o dosya için
//! geçerlidir: `Player::load*` bir sonrakinde temizler.
//!
//! Müzik modunda (Lounge) mpv penceresi gizlidir; arayüz kendi "Şimdi çalıyor"
//! ekranını çizer. Görüntüsü ayrı akış olan parçada (YouTube) yalnızca ses açılır
//! (`media::load`); tek dosyalı görüntülü parçada video izi boşa çizilmesin diye grafiğe
//! bağlanıp atılır. Sinema'ya dönünce video geri gelir.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use koni_player::Player;

/// Sabit Q dönüşümü çubukları + akan spektrogram; eksen yazısı yok (yazı tipi istemez).
fn visualizer(aid: &str) -> String {
    format!(
        "[aid{aid}]asplit[ao][a];[a]showcqt=s=1280x720:r=30:axis=0:\
         bar_g=2:sono_g=3:bar_v=9:sono_v=12:count=2[vo]"
    )
}

/// Müzik modu açık mı (`set_music_view`).
static MUSIC_VIEW: AtomicBool = AtomicBool::new(false);
/// Müzik modunda yalnızca sesi açılan akış: (açılan ses adresi, indirilmeyen görüntü).
/// Sinema'ya geçilince görüntü eklenir.
static VIDEO_LATER: Mutex<Option<(String, String)>> = Mutex::new(None);
/// Müzik modunda (mpv gizli) kapak/görselleştirici açılmadı; Sinema'da açılır.
static UNDECORATED: AtomicBool = AtomicBool::new(false);

pub fn music_view() -> bool {
    MUSIC_VIEW.load(Ordering::Relaxed)
}

/// `media::load`: müzik modunda `main` (ses) açıldı, `video` sonraya kaldı; `None`: normal.
pub fn set_video_later(v: Option<(String, String)>) {
    *VIDEO_LATER.lock().unwrap() = v;
}
/// Görselleştirici videonun yerine açıldı: grafik kalkınca mpv izleri seçili bırakmaz,
/// eski `vid` ve `aid` geri seçilir.
static HIDDEN: Mutex<Option<(String, String)>> = Mutex::new(None);

/// (görüntü izi var, albüm kapağı var, ses izi var)
fn tracks(mpv: &Player) -> (bool, bool, bool) {
    let n: usize = mpv
        .get_property_string("track-list/count")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let (mut video, mut art, mut audio) = (false, false, false);
    for i in 0..n {
        let get = |k: &str| mpv.get_property_string(&format!("track-list/{i}/{k}"));
        match get("type").as_deref() {
            Some("video") if get("albumart").as_deref() == Some("yes") => art = true,
            Some("video") => video = true,
            Some("audio") => audio = true,
            _ => {}
        }
    }
    (video, art, audio)
}

/// Çalan dosya müzik mi: ses var, (kapak dışında) görüntü yok.
pub fn is_music(mpv: &Player) -> bool {
    let (video, _, audio) = tracks(mpv);
    audio && !video
}

/// mpv `file-loaded`: dosya müzik mi? Müzikse kapak ya da görselleştirici arka planda
/// açılır. Müzik modunda görüntülü dosyanın videosu gizlenir.
pub fn on_file_loaded(mpv: &Arc<Player>) -> bool {
    *HIDDEN.lock().unwrap() = None; // yükleme önceki grafiği temizledi
    UNDECORATED.store(false, Ordering::Relaxed);
    let (video, art, audio) = tracks(mpv);
    if video && audio && music_view() {
        hide_video(mpv);
        return false;
    }
    if video || !audio {
        return false;
    }
    if art {
        return true; // dosyanın kendi kapağı
    }
    if music_view() {
        // mpv penceresi gizli: kapak indirmek, hele görselleştirici (showcqt, 30 fps)
        // çizmek boşa; zayıf makinede sesi de takıltır.
        UNDECORATED.store(true, Ordering::Relaxed);
        return true;
    }
    decorate(mpv);
    true
}

/// Görüntüsüz seste kapak, yoksa görselleştirici (arka planda).
fn decorate(mpv: &Arc<Player>) {
    let cover = crate::tracks::cover(mpv);
    let mpv = mpv.clone();
    // `video-add` kapağı indirir: olay döngüsü beklemesin.
    std::thread::spawn(move || {
        let added = cover.is_some_and(|c| {
            mpv.command(&["video-add", &c, "select", "Kapak", "", "yes"])
                .is_ok()
        });
        if !added {
            if let Err(e) = mpv.set_lavfi_complex(&visualizer("1")) {
                eprintln!("[koni] görselleştirici açılamadı: {e}");
            }
        }
    });
}

/// Görüntülü dosyada video çizilmez, yalnızca ses çalar. Video izi grafikte
/// kalır (`nullsink`): seçili akışı olmayan ana demuxer okumayı bırakır, ses ayrı
/// dosyadaysa (YouTube) önbellek süresi 0 görünür ve oda "takılıyor"da bekler.
fn hide_video(mpv: &Player) {
    let get = |k: &str| mpv.get_property_string(k).unwrap_or_else(|| "auto".into());
    let (vid, aid) = (get("vid"), get("aid"));
    let id = |s: &str| {
        if s.parse::<u32>().is_ok() {
            s.to_string()
        } else {
            "1".into()
        }
    };
    let graph = format!("[vid{}]nullsink;[aid{}]anull[ao]", id(&vid), id(&aid));
    match mpv.set_lavfi_complex(&graph) {
        Ok(()) => *HIDDEN.lock().unwrap() = Some((vid, aid)),
        Err(e) => eprintln!("[koni] görselleştirici açılamadı: {e}"),
    }
}

/// Oda modu değişti (Müzik ↔ Sinema/Keşfet): çalan görüntülü dosyada video gizlenir ya
/// da geri getirilir.
pub fn set_music_view(mpv: &Arc<Player>, on: bool) {
    MUSIC_VIEW.store(on, Ordering::Relaxed);
    if !on {
        // Müzik modunda indirilmeyen görüntü (ya da çizilmeyen kapak) şimdi gelir.
        let later = VIDEO_LATER.lock().unwrap().take();
        let path = mpv.get_property_string("path");
        match later {
            Some((main, video)) if path.as_deref() == Some(main.as_str()) => {
                UNDECORATED.store(false, Ordering::Relaxed); // görüntü geliyor, kapak gerekmez
                let mpv = mpv.clone();
                // `video-add` akışı açar: arayüz beklemesin.
                std::thread::spawn(move || {
                    if let Err(e) = mpv.command(&["video-add", &video, "select"]) {
                        eprintln!("[koni] görüntü eklenemedi: {e}");
                    }
                });
            }
            _ if UNDECORATED.swap(false, Ordering::Relaxed) => decorate(mpv),
            _ => {}
        }
    }
    let hidden = HIDDEN.lock().unwrap().take();
    match (on, hidden) {
        (true, Some(h)) => *HIDDEN.lock().unwrap() = Some(h),
        (true, None) => {
            let (video, _, audio) = tracks(mpv);
            if video && audio {
                hide_video(mpv);
            }
        }
        (false, Some((vid, aid))) => {
            let _ = mpv.set_property_string("lavfi-complex", "");
            let _ = mpv.set_property_string("vid", &vid);
            let _ = mpv.set_property_string("aid", &aid);
        }
        (false, None) => {}
    }
}
