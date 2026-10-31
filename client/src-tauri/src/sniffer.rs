//! Keşfet tarayıcısı: ana pencereye bağlı çerçevesiz pencere, Keşfet alanının üstüne
//! oturur (`set_embed`). Sayfaya `pagesync.js` enjekte edilir: sitenin videosu odada
//! tarayıcıda senkronlanır (`page.rs`).
//!
//! Güvenlik: pencerede rastgele site çalışır. IPC yalnızca bu pencereye ve yalnızca
//! `page_video` komutuna açıktır (`capabilities/sniffer.json`); yeni pencere
//! (pop-up, `target="_blank"`) istekleri reddedilir. Reklam sunucuları `adblock.rs`
//! ile engellenir; kurulumda exe'nin yanında `extensions/` varsa (uBlock Origin Lite)
//! tarayıcıya eklenti olarak yüklenir (yalnızca Windows/WebView2).

use crate::i18n::trs;
use serde::Serialize;
use tauri::webview::NewWindowResponse;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};

use crate::{adblock, AppState};

pub const LABEL: &str = "sniffer";
/// Tarayıcıda senkron betiği (`page.rs`). Eskiden burada akışı mpv'ye alan
/// `sniffer.js` vardı; Keşfet'te video artık sitenin oynatıcısında kalır.
const SCRIPT: &str = include_str!("pagesync.js");

/// UI'a giden yakalayıcı durumu (`sniffer://state`).
#[derive(Serialize, Clone)]
pub struct SnifferEvent {
    /// `open` | `captured` | `closed` | `error`
    pub status: &'static str,
    pub page: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn emit(app: &AppHandle, ev: SnifferEvent) {
    let _ = app.emit_to("main", "sniffer://state", ev);
}

/// Keşfet modunda tarayıcının gömüleceği alan: ana pencerenin istemci alanında
/// CSS pikseli (x, y, genişlik, yükseklik). `None`: ayrı pencere.
#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct EmbedRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Gömülü alan değişti (UI'da `ResizeObserver`); açık gömülü pencere yerine oturur.
pub fn set_embed(app: &AppHandle, rect: Option<EmbedRect>) {
    *app.state::<AppState>().sniff_rect.lock().unwrap() = rect;
    place(app);
}

/// Gömülü yakalayıcıyı alanın üstüne taşır (ana pencere taşınınca da çağrılır).
pub fn place(app: &AppHandle) {
    let Some(r) = *app.state::<AppState>().sniff_rect.lock().unwrap() else {
        return;
    };
    let (Some(main), Some(w)) = (
        app.get_webview_window("main"),
        app.get_webview_window(LABEL),
    ) else {
        return;
    };
    if !*app.state::<AppState>().sniff_embedded.lock().unwrap() {
        return;
    }
    let (Ok(origin), Ok(scale)) = (main.inner_position(), main.scale_factor()) else {
        return;
    };
    // Kenarlar ayrı yuvarlanır: genişlik sağ kenardan çıkarılır, alan dışına taşma olmaz.
    let px = |v: f64| (v * scale).round() as i32;
    let (left, top) = (px(r.x), px(r.y));
    let (width, height) = (px(r.x + r.w) - left, px(r.y + r.h) - top);
    let _ = w.set_size(PhysicalSize::new(width.max(1) as u32, height.max(1) as u32));
    // Windows çerçevesiz pencerede de görünmez kenarlık bırakır: konum dış çerçeveye
    // uygulanır, sayfa iç alandan başlar. Fark düşülür ki iç alan tam alana otursun.
    let (dx, dy) = match (w.outer_position(), w.inner_position()) {
        (Ok(o), Ok(i)) => (i.x - o.x, i.y - o.y),
        _ => (0, 0),
    };
    let _ = w.set_position(PhysicalPosition::new(
        origin.x + left - dx,
        origin.y + top - dy,
    ));
}

/// Gömülü tarayıcıda geri / ileri / yenile.
pub fn nav(app: &AppHandle, action: &str) -> Result<(), String> {
    let js = match action {
        "back" => "history.back()",
        "forward" => "history.forward()",
        "reload" => "location.reload()",
        _ => return Err(trs("geçersiz komut", "invalid command").into()),
    };
    let w = app
        .get_webview_window(LABEL)
        .ok_or(trs("tarayıcı kapalı", "browser is closed"))?;
    w.eval(js).map_err(|e| e.to_string())
}

/// Yakalayıcıyı `page` ile açar; açıksa o sayfaya gider. Keşfet alanı varsa
/// (`set_embed`) çerçevesiz pencere olarak video alanının üstüne oturur. Async bağlamdan çağrılmalı
/// (Windows'ta senkron komuttan pencere oluşturmak kilitlenir).
pub fn open(app: &AppHandle, page: &str) -> Result<(), String> {
    let url: tauri::Url = page
        .parse()
        .map_err(|e| tr!("geçersiz URL: {e}", "invalid URL: {e}"))?;
    crate::page::forget_video();
    *app.state::<AppState>().sniff_page.lock().unwrap() = Some(page.to_owned());
    if let Some(w) = app.get_webview_window(LABEL) {
        w.navigate(url).map_err(|e| e.to_string())?;
        let _ = w.set_focus();
    } else {
        let main = app
            .get_webview_window("main")
            .ok_or(trs("ana pencere bulunamadı", "main window not found"))?;
        let embed = app.state::<AppState>().sniff_rect.lock().unwrap().is_some();
        *app.state::<AppState>().sniff_embedded.lock().unwrap() = embed;
        let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(url));
        let win = with_extensions(app, builder)
            .title(trs(
                "KONİ Yakalayıcı: sayfada videoyu oynat",
                "KONİ Catcher: play the video on the page",
            ))
            .inner_size(1100.0, 720.0)
            .decorations(!embed)
            // Çerçevesiz pencerenin gölgesi ve 1 px kenarı video alanının dışına taşar.
            .shadow(!embed)
            .skip_taskbar(embed)
            .resizable(!embed)
            .visible(!embed)
            .parent(&main)
            .map_err(|e| e.to_string())?
            .initialization_script_for_all_frames(SCRIPT)
            .on_new_window(|_, _| NewWindowResponse::Deny)
            // Reklam sitesine yönlendirme (tıklama korsanlığı) sayfayı değiştirmesin.
            .on_navigation(|url| {
                if adblock::is_ad_url(url) {
                    return false;
                }
                adblock::reset_blocked();
                true
            })
            .build()
            .map_err(|e| {
                tr!(
                    "yakalayıcı açılamadı: {e}",
                    "couldn't open the catcher: {e}"
                )
            })?;
        adblock::install(&win);
        if embed {
            place(app);
            let _ = win.show();
        }
        let app = app.clone();
        win.on_window_event(move |e| {
            if matches!(e, WindowEvent::Destroyed) {
                // Yakalamadan kapatıldı (yakalanınca sayfa önceden alınır).
                if let Some(page) = app.state::<AppState>().sniff_page.lock().unwrap().take() {
                    emit(
                        &app,
                        SnifferEvent {
                            status: "closed",
                            page,
                            title: None,
                            error: None,
                        },
                    );
                }
            }
        });
    }
    emit(
        app,
        SnifferEvent {
            status: "open",
            page: page.to_owned(),
            title: None,
            error: None,
        },
    );
    Ok(())
}

/// Tarayıcının kendi profili: eklentiler WebView2 ortamının ayarı olduğu için ana
/// pencereyle aynı profili paylaşamaz (ayarlar farklıysa pencere açılmaz). Exe'nin
/// yanındaki `extensions/` altındaki her klasör açılmış bir Chrome eklentisidir.
fn with_extensions<'a, R: tauri::Runtime, M: Manager<R>>(
    app: &AppHandle,
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WebviewWindowBuilder<'a, R, M> {
    let Ok(dir) = app.path().app_local_data_dir() else {
        return builder;
    };
    let builder = builder.data_directory(dir.join("browser"));
    let ext = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("extensions")))
        .filter(|d| d.is_dir());
    match ext {
        Some(ext) => builder
            .browser_extensions_enabled(true)
            .extensions_path(ext),
        None => builder,
    }
}

/// Kullanıcı vazgeçti.
pub fn close(app: &AppHandle) {
    crate::page::forget_video();
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
    }
}
