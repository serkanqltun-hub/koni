//! Frontend'in çağırdığı Tauri IPC komutları (sözleşme: kokpit UI ↔ Rust).

use crate::i18n::trs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use koni_player::{Player, PlayerSnapshot};
use koni_surface::{LayoutConfig, LayoutMode};
use koni_sync::protocol::{ClientMsg, Policy, SubFile};
use tauri::{AppHandle, Manager, State};

use crate::media::{self, Source};
use crate::sync::{self, Entry, UiCmd, Who};
use crate::{account, lobby, sniffer, AppState};

type CmdResult<T = ()> = Result<T, String>;

fn player(state: &State<'_, AppState>) -> CmdResult<Arc<Player>> {
    if let Some(p) = state.player.lock().unwrap().as_ref() {
        return Ok(p.clone());
    }
    Err(state
        .player_error
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| trs("oynatıcı hazır değil", "player isn't ready").into()))
}

/// "Aç" kutusu. Web adresi önce yerelde çözülür; yt-dlp siteyi tanımıyorsa Sanal
/// yt-dlp siteyi tanımıyorsa `None` döner: UI sayfayı Keşfet'te açıp tarayıcıda senkronlar.
/// Odadayken çözülen adres odaya gönderilir (herkes kendisi çözer ve yükler); yerel
/// dosya yüklenir ve adı odaya bildirilir. Dönen değer gösterilecek başlıktır.
#[tauri::command]
pub async fn player_load(app: AppHandle, src: String) -> CmdResult<Option<String>> {
    let state = app.state::<AppState>();
    let mpv = player(&state)?;
    let room = state.sync.lock().unwrap().as_ref().map(|h| h.tx.clone());
    let closed = |_| trs("oda bağlantısı kapandı", "room connection closed").to_string();
    let load = LOAD_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    match Source::parse(&src)? {
        Source::Web(url) => open_web(&app, url, None, String::new(), Vec::new(), load).await,
        Source::File(path) => {
            if !std::path::Path::new(&path).is_file() {
                return Err(tr!("dosya bulunamadı: {path}", "file not found: {path}"));
            }
            mpv.load(&path).map_err(|e| e.to_string())?;
            crate::tracks::clear();
            let name = media::file_name(&path);
            if let Some(tx) = room {
                tx.send(UiCmd::OpenedFile(name.clone())).map_err(closed)?;
            }
            Ok(Some(name))
        }
    }
}

/// Her `player_load` artırır. Çözüm (yt-dlp) birkaç sn sürer: bu arada başka şey açıldıysa
/// (müzikte art arda "sonraki") eski çözüm yüklenmez, yoksa sonradan biten eski şarkı
/// yenisinin yerine çalardı.
static LOAD_GEN: AtomicU64 = AtomicU64::new(0);
/// Yerine başka medya açıldığı için yüklenmeyen `player_load`'un hatası (UI yok sayar).
const SUPERSEDED: &str = "superseded";

/// Web adresini çözüp açar: odadaysa odaya gönderir, değilse yükler. `referer`:
/// yakalanan doğrudan akış (yt-dlp'ye gitmez). `load`: çağıran `player_load`'un sırası.
async fn open_web(
    app: &AppHandle,
    url: String,
    referer: Option<String>,
    title: String,
    subs: Vec<SubFile>,
    load: u64,
) -> CmdResult<Option<String>> {
    let state = app.state::<AppState>();
    let mpv = player(&state)?;
    let (u, r) = (url.clone(), referer.clone());
    let res = tauri::async_runtime::spawn_blocking(move || {
        media::resolve_web(&u, r.as_deref(), &title, &subs)
    })
    .await
    .map_err(|e| e.to_string())?;
    let stream = match res {
        Ok(s) => s,
        // yt-dlp tanımıyor: UI sayfayı Keşfet'te açar, video sitenin oynatıcısında
        // kalır ve tarayıcıda senkronlanır (`page.rs`).
        Err(media::WebError::Unsupported(_)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let room = state.sync.lock().unwrap().as_ref().map(|h| h.tx.clone());
    if let Some(tx) = room {
        // yt-dlp altyazılarını herkes kendisi çözer; yalnızca yakalananlar odaya gider.
        let subs = if referer.is_some() {
            stream.subs.clone()
        } else {
            Vec::new()
        };
        tx.send(UiCmd::OpenWeb {
            src: url,
            title: stream.title.clone(),
            referer: referer.unwrap_or_default(),
            subs,
        })
        .map_err(|_| trs("oda bağlantısı kapandı", "room connection closed").to_string())?;
    } else {
        if LOAD_GEN.load(Ordering::SeqCst) != load {
            return Err(SUPERSEDED.into());
        }
        media::load(&mpv, &stream)?;
    }
    crate::page::command(app, "pause", 0.0);
    Ok(Some(stream.title))
}

/// Kullanıcı yakalayıcıdan vazgeçti.
#[tauri::command]
pub fn sniffer_close(app: AppHandle) {
    sniffer::close(&app);
}

/// Keşfet "Siteye git": sayfa yt-dlp'ye sorulmadan tarayıcıda açılır.
#[tauri::command]
pub async fn browse_open(app: AppHandle, page: String) -> CmdResult {
    let page = page.trim();
    let page = if page.contains("://") {
        page.to_owned()
    } else {
        format!("https://{page}")
    };
    koni_resolver::classify(&page)
        .map_err(|_| trs("geçersiz adres", "invalid address").to_string())?;
    sniffer::open(&app, &page)
}

/// Keşfet alanı (CSS px); `None`: gömme yok, yakalayıcı ayrı pencerede açılır.
#[tauri::command]
pub fn browse_embed(app: AppHandle, rect: Option<sniffer::EmbedRect>) {
    sniffer::set_embed(&app, rect);
}

/// Keşfet adres çubuğu: açık sayfa, sayfadaki video ve engellenen reklam sayısı.
#[derive(serde::Serialize)]
pub struct BrowseState {
    url: Option<String>,
    title: String,
    video: Option<crate::page::VideoState>,
    blocked: u32,
}

#[tauri::command]
pub fn browse_state(app: AppHandle) -> BrowseState {
    let url = app
        .get_webview_window(sniffer::LABEL)
        .and_then(|w| w.url().ok())
        .map(|u| u.to_string());
    BrowseState {
        video: url.as_ref().and_then(|_| crate::page::state()),
        title: crate::page::title(),
        url,
        blocked: crate::adblock::blocked(),
    }
}

/// Sayfadaki betik (`pagesync.js`): videonun durumu; kullanıcı sitenin oynatıcısında
/// bastıysa odaya gider.
#[tauri::command]
pub fn page_video(
    app: AppHandle,
    webview: tauri::Webview,
    report: crate::page::Report,
) -> CmdResult {
    if webview.label() != sniffer::LABEL {
        return Err(trs("yalnızca tarayıcı penceresi", "browser window only").into());
    }
    let Some((op, pos)) = crate::page::report(report) else {
        return Ok(());
    };
    let tx = app
        .state::<AppState>()
        .sync
        .lock()
        .unwrap()
        .as_ref()
        .map(|h| h.tx.clone());
    if let Some(tx) = tx {
        let cmd = match op.as_str() {
            "play" => UiCmd::Play,
            "pause" => UiCmd::Pause,
            _ => UiCmd::Seek(pos),
        };
        let _ = tx.send(cmd);
    }
    Ok(())
}

/// Tarayıcıda senkron başlat: odaya `page` medyası (herkes sayfayı Keşfet'te açar).
#[tauri::command]
pub fn sync_page(app: AppHandle, src: String, title: String) -> CmdResult {
    koni_resolver::classify(&src)
        .map_err(|_| trs("geçersiz adres", "invalid address").to_string())?;
    let title: String = title.trim().chars().take(200).collect();
    let tx = app
        .state::<AppState>()
        .sync
        .lock()
        .unwrap()
        .as_ref()
        .map(|h| h.tx.clone())
        .ok_or(trs("odada değilsin", "you're not in a room"))?;
    tx.send(UiCmd::OpenPage { src, title })
        .map_err(|_| trs("oda bağlantısı kapandı", "room connection closed").to_string())
}

/// Gömülü tarayıcıda `back` | `forward` | `reload`.
#[tauri::command]
pub fn browse_nav(app: AppHandle, action: String) -> CmdResult {
    sniffer::nav(&app, &action)
}

#[tauri::command]
pub fn player_play(state: State<'_, AppState>) -> CmdResult {
    player(&state)?.play().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn player_pause(state: State<'_, AppState>) -> CmdResult {
    player(&state)?.pause().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn player_toggle_pause(state: State<'_, AppState>) -> CmdResult {
    player(&state)?.toggle_pause().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn player_seek(state: State<'_, AppState>, seconds: f64) -> CmdResult {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(tr!(
            "geçersiz konum: {seconds}",
            "invalid position: {seconds}"
        ));
    }
    player(&state)?
        .seek_absolute(seconds)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn player_set_speed(state: State<'_, AppState>, speed: f64) -> CmdResult {
    if !(0.25..=4.0).contains(&speed) {
        return Err(tr!("geçersiz hız: {speed}", "invalid speed: {speed}"));
    }
    player(&state)?.set_speed(speed).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn player_set_volume(state: State<'_, AppState>, volume: f64) -> CmdResult {
    if !(0.0..=130.0).contains(&volume) {
        return Err(tr!("geçersiz ses: {volume}", "invalid volume: {volume}"));
    }
    player(&state)?
        .set_property_string("volume", &volume.to_string())
        .map_err(|e| e.to_string())
}

/// Yerel altyazı dosyasını ekler ve seçer; dosya adını döndürür.
#[tauri::command]
pub fn player_add_subtitle(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    let path = path.trim().trim_matches('"').to_string();
    if !std::path::Path::new(&path).is_file() {
        return Err(tr!(
            "altyazı bulunamadı: {path}",
            "subtitles not found: {path}"
        ));
    }
    player(&state)?
        .command(&["sub-add", &path, "select"])
        .map_err(|e| e.to_string())?;
    Ok(media::file_name(&path))
}

/// Altyazı gecikmesini `delta` sn kaydırır (g/h: ∓0.05); yeni değeri döndürür.
#[tauri::command]
pub fn player_sub_delay(state: State<'_, AppState>, delta: f64) -> CmdResult<f64> {
    if !delta.is_finite() || delta.abs() > 10.0 {
        return Err(tr!("geçersiz gecikme: {delta}", "invalid delay: {delta}"));
    }
    let p = player(&state)?;
    p.command(&["add", "sub-delay", &delta.to_string()])
        .map_err(|e| e.to_string())?;
    let v: f64 = p
        .get_property_string("sub-delay")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let _ = p.show_text(
        &tr!(
            "Altyazı gecikmesi: {:+.0} ms",
            "Subtitle delay: {:+.0} ms",
            v * 1000.0
        ),
        1500,
    );
    Ok(v)
}

/// Kalite · ses · altyazı listesi (`tracks::Tracks`).
#[tauri::command]
pub fn player_tracks(state: State<'_, AppState>) -> CmdResult<crate::tracks::Tracks> {
    let p = player(&state)?;
    Ok(crate::tracks::list(&p))
}

/// Müzik modu: çalan görüntülü dosyada video yerine görselleştirici (`lounge`).
#[tauri::command]
pub fn player_music_view(state: State<'_, AppState>, on: bool) -> CmdResult {
    let p = player(&state)?;
    crate::lounge::set_music_view(&p, on);
    Ok(())
}

/// Müzik ekranı: çalan akışın yt-dlp kapağı (yoksa `None`).
#[tauri::command]
pub fn player_cover(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    let p = player(&state)?;
    Ok(crate::tracks::cover(&p))
}

/// `kind`: `video` | `audio` | `sub`; `key`: `player_tracks`'in verdiği anahtar.
#[tauri::command]
pub async fn player_set_track(app: AppHandle, kind: String, key: String) -> CmdResult {
    let p = player(&app.state::<AppState>())?;
    // Yeni kalite/dublaj adresini açmak bir-iki saniye sürebilir: arayüz donmasın.
    tauri::async_runtime::spawn_blocking(move || crate::tracks::select(&p, &kind, &key))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn player_state(state: State<'_, AppState>) -> CmdResult<PlayerSnapshot> {
    Ok(player(&state)?.snapshot())
}

/// Layout config'i saklar ve video kutusunu yeniden konumlar (seyrek çağrılır).
#[tauri::command]
pub fn set_layout(app: AppHandle, state: State<'_, AppState>, layout: LayoutConfig) -> CmdResult {
    *state.layout.lock().unwrap() = layout.clone();
    on_main(&app, move |_surface| {
        #[cfg(windows)]
        {
            use koni_surface::VideoSurface;
            _surface.apply_layout(&layout);
        }
    })
}

#[tauri::command]
pub fn set_fullscreen(app: AppHandle, on: bool) -> CmdResult {
    on_main(&app, move |_surface| {
        #[cfg(windows)]
        {
            use koni_surface::VideoSurface;
            let res = if on {
                _surface.enter_fullscreen()
            } else {
                _surface.exit_fullscreen()
            };
            if let Err(e) = res {
                eprintln!("[koni] tam ekran geçişi başarısız: {e}");
            }
        }
    })?;
    // Tam ekranda layout modu yüzey tarafında Fullscreen olur; kaydı da güncel tut.
    let state = app.state::<AppState>();
    let mut layout = state.layout.lock().unwrap();
    if on {
        layout.mode = LayoutMode::Fullscreen;
    } else if layout.mode == LayoutMode::Fullscreen {
        layout.mode = LayoutMode::Cockpit;
    }
    Ok(())
}

/// Win32 çağrıları pencerenin sahibi olan ana thread'de yapılmalı.
fn on_main<F>(app: &AppHandle, f: F) -> CmdResult
where
    F: FnOnce(&mut crate::Surface) + Send + 'static,
{
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let state = handle.state::<AppState>();
        let mut guard = state.surface.lock().unwrap();
        if let Some(surface) = guard.as_mut() {
            f(surface);
        }
    })
    .map_err(|e| e.to_string())
}

fn ws_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.starts_with("ws://") || url.starts_with("wss://") {
        Ok(url.to_string())
    } else {
        Err(tr!(
            "sunucu adresi ws:// veya wss:// ile başlamalı: {url}",
            "server address must start with ws:// or wss://: {url}"
        ))
    }
}

/// Santral'a bağlanır: `invite` varsa o odaya katılır, yoksa `room_name` ile oda kurar.
/// Bu sunucuya kayıtlı hesap oturumu varsa onunla girer. Açık bir oturum varsa önce kapatılır.
#[tauri::command]
pub fn sync_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    username: String,
    room_name: Option<String>,
    invite: Option<String>,
) -> CmdResult {
    let url = ws_url(&url)?;
    let entry = match invite
        .map(|i| i.trim().to_string())
        .filter(|i| !i.is_empty())
    {
        Some(invite) => Entry::Join { invite },
        None => Entry::Create {
            name: room_name
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "Oda".into()),
            policy: None,
        },
    };
    let who = Who {
        username: username.trim().to_string(),
        auth: account::get(&url).map(|a| a.token),
    };
    let mpv = player(&state)?;
    let mut slot = state.sync.lock().unwrap();
    if let Some(old) = slot.take() {
        let _ = old.tx.send(UiCmd::Leave);
    }
    *slot = Some(sync::spawn(app, mpv, url, who, entry));
    Ok(())
}

/// Bu sunucuda kayıtlı hesabın adı (yoksa `null`).
#[tauri::command]
pub fn account_get(url: String) -> Option<String> {
    account::get(url.trim()).map(|a| a.name)
}

/// Karşılıklı arkadaşların istatistikleri ve rozetler; hesap yoksa `null`.
#[tauri::command]
pub async fn stats_get(url: String, period: String) -> CmdResult<Option<serde_json::Value>> {
    let url = ws_url(&url)?;
    let Some(acc) = account::get(&url) else {
        return Ok(None);
    };
    Ok(Some(account::stats(&url, acc.token, period).await?))
}

/// Profil listeleri (p0.23). Hesap yoksa `null`.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri komutu: parametreler doğrudan JS'ten gelir
pub async fn lists_call(
    url: String,
    op: String,
    user: Option<String>,
    list: Option<String>,
    reference: Option<String>,
    rating: Option<i32>,
    entry: Option<serde_json::Value>,
) -> CmdResult<Option<serde_json::Value>> {
    let url = ws_url(&url)?;
    let Some(acc) = account::get(&url) else {
        return Ok(None);
    };
    let msg = koni_sync::protocol::ClientMsg::Lists {
        auth: acc.token,
        op,
        user: user.unwrap_or_default(),
        list: list.unwrap_or_default(),
        reference: reference.unwrap_or_default(),
        rating,
        entry,
    };
    Ok(Some(account::lists(&url, msg).await?))
}

/// Ana ekran "Etkinlik" sekmesi (p0.22). Hesap yoksa `null`.
#[tauri::command]
pub async fn feed_get(url: String) -> CmdResult<Option<serde_json::Value>> {
    let url = ws_url(&url)?;
    let Some(acc) = account::get(&url) else {
        return Ok(None);
    };
    Ok(Some(account::feed(&url, acc.token).await?))
}

/// Arkadaş listesi; `op` `list` | `add` | `remove`. Hesap yoksa `null`.
#[tauri::command]
pub async fn friends(
    url: String,
    op: String,
    user: String,
) -> CmdResult<Option<serde_json::Value>> {
    let url = ws_url(&url)?;
    let Some(acc) = account::get(&url) else {
        return Ok(None);
    };
    match account::friends(&url, acc.token, op, user.trim().to_string()).await {
        // p0.15 ve öncesi sunucu `friends` el sıkışmasını tanımaz.
        Err(e) if e.contains("create_room") => Err(trs(
            "Sunucu arkadaş eklemeyi henüz desteklemiyor",
            "This server doesn't support adding friends yet",
        )
        .into()),
        r => Ok(Some(r?)),
    }
}

/// Profil (p0.18): ad, etiket, fotoğraf, seçilebilir fotoğraflar, davet kodları, son 7 gün.
/// `op` `get` | `avatar` (+ `avatar`) | `invite` (yeni kod `code`). Hesap yoksa `null`.
#[tauri::command]
pub async fn profile(
    url: String,
    op: String,
    avatar: Option<String>,
    image: Option<String>,
) -> CmdResult<Option<serde_json::Value>> {
    let url = ws_url(&url)?;
    let Some(acc) = account::get(&url) else {
        return Ok(None);
    };
    let (avatar, image) = (avatar.unwrap_or_default(), image.unwrap_or_default());
    match account::profile(&url, acc.token, op, avatar, image).await {
        Err(e) if e.contains("create_room") => Err(trs(
            "Sunucu profilleri henüz desteklemiyor",
            "This server doesn't support profiles yet",
        )
        .into()),
        r => Ok(Some(r?)),
    }
}

/// Yönetim (p0.19, yalnızca admin): `op` `list` | `close` (+room) | `ban` (+user) |
/// `invite_right` (+user, on).
#[tauri::command]
pub async fn admin(
    url: String,
    op: String,
    room: Option<String>,
    user: Option<String>,
    on: Option<bool>,
) -> CmdResult<serde_json::Value> {
    let url = ws_url(&url)?;
    let Some(acc) = account::get(&url) else {
        return Err(trs("giriş yapılmamış", "not signed in").into());
    };
    let msg = ClientMsg::Admin {
        auth: acc.token,
        op,
        room: room.unwrap_or_default(),
        user: user.unwrap_or_default(),
        on: on.unwrap_or(false),
    };
    account::admin(&url, &msg).await
}

/// Profil fotoğrafı `data:` adresi olarak (oturum ister).
#[tauri::command]
pub async fn avatar_get(url: String, id: String) -> CmdResult<String> {
    let url = ws_url(&url)?;
    let Some(acc) = account::get(&url) else {
        return Err(trs("giriş yapılmamış", "not signed in").into());
    };
    account::avatar(&url, &acc.token, &id).await
}

/// Lobi bağlantısını aç (arkadaşların durumu, çağrılar: `lobby://msg`). Hesap yoksa kapatır.
#[tauri::command]
pub fn lobby_start(app: AppHandle, url: String) -> CmdResult {
    let url = ws_url(&url)?;
    crate::debrid::set_server(&url);
    match account::get(&url) {
        Some(acc) => lobby::start(app, url, acc.token),
        None => lobby::stop(),
    }
    Ok(())
}

/// Gelen çağrıyı geri çevir.
#[tauri::command]
pub fn lobby_decline(invite: String) {
    lobby::send(serde_json::json!({ "t": "decline", "invite": invite }).to_string());
}

/// Hata bildiriminin en büyük boyutu (sunucu sınırı, p0.14).
const BUG_REPORT_MAX: usize = 200 * 1024;

/// "Sorun bildir": sürüm, işletim sistemi, kullanıcının notu, arayüz etkinlik logu ve
/// son senkron CSV'sinin sonu sunucuya tek mesajla gider. Dönen değer rapor adı.
#[tauri::command]
pub async fn bug_report(
    app: AppHandle,
    url: String,
    username: String,
    note: String,
    log: Vec<String>,
) -> CmdResult<String> {
    let url = ws_url(&url)?;
    let version = app.package_info().version.to_string();
    let note: String = note.trim().chars().take(4000).collect();
    let mut head = format!(
        "KONİ {version} · {} {}\n\n## Açıklama\n{}\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        if note.is_empty() { "(yok)" } else { &note },
    );
    let mut body = format!("\n## Etkinlik ({} satır)\n", log.len());
    for l in &log {
        body.push_str(l);
        body.push('\n');
    }
    if let Some((name, tail)) = app
        .path()
        .app_log_dir()
        .ok()
        .and_then(|d| last_sync_csv(&d, 400))
    {
        body.push_str(&format!("\n## Senkron logu: {name} (son satırlar)\n{tail}"));
    }
    // Sınırı aşarsa en eski satırlar düşer; başlık ve not korunur.
    let budget = BUG_REPORT_MAX - head.len(); // not 4000 karakterle sınırlı, taşmaz
    if body.len() > budget {
        let mut cut = body.len() - budget;
        while !body.is_char_boundary(cut) {
            cut += 1;
        }
        body = body.split_off(cut);
    }
    head.push_str(&body);
    let acc = account::get(&url);
    let msg = ClientMsg::BugReport {
        username: acc.is_none().then(|| username.trim().to_string()),
        auth: acc.map(|a| a.token),
        version,
        text: head,
    };
    account::bug_report(&url, &msg).await
}

/// `app_log_dir`'deki en yeni `sync-*.csv`'nin adı ve son `lines` satırı.
fn last_sync_csv(dir: &std::path::Path, lines: usize) -> Option<(String, String)> {
    let path = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with("sync-") && n.ends_with(".csv")
        })
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())?
        .path();
    let text = std::fs::read_to_string(&path).ok()?;
    let all: Vec<&str> = text.lines().collect();
    let tail = all[all.len().saturating_sub(lines)..].join("\n");
    Some((
        path.file_name()?.to_string_lossy().into_owned(),
        tail + "\n",
    ))
}

#[tauri::command]
pub async fn account_login(url: String, username: String, password: String) -> CmdResult<String> {
    let url = ws_url(&url)?;
    let msg = ClientMsg::Login {
        username: username.trim().to_string(),
        password,
        device: account::device(),
    };
    let acc = account::handshake(&url, &msg).await?;
    account::save(&url, &acc)?;
    Ok(acc.name)
}

/// Tek kullanımlık davet koduyla hesap açar (ve giriş yapar).
#[tauri::command]
pub async fn account_register(
    url: String,
    code: String,
    username: String,
    password: String,
    avatar: Option<String>,
) -> CmdResult<String> {
    let url = ws_url(&url)?;
    let msg = ClientMsg::Register {
        code: code.trim().to_string(),
        username: username.trim().to_string(),
        password,
        device: account::device(),
        avatar: avatar.unwrap_or_default(),
    };
    let acc = account::handshake(&url, &msg).await?;
    account::save(&url, &acc)?;
    Ok(acc.name)
}

/// Çıkış: sunucudaki oturumu siler (ulaşılamazsa yalnızca yereli) ve belirteci unutur.
#[tauri::command]
pub async fn account_logout(url: String) -> CmdResult {
    let url = url.trim().to_string();
    lobby::stop();
    if let Some(acc) = account::get(&url) {
        let _ = account::handshake(&url, &ClientMsg::Logout { auth: acc.token }).await;
    }
    account::forget(&url)
}

fn sync_send(state: &State<'_, AppState>, cmd: UiCmd) -> CmdResult {
    match state.sync.lock().unwrap().as_ref() {
        Some(h) => {
            h.tx.send(cmd)
                .map_err(|_| trs("oda bağlantısı kapandı", "room connection closed").into())
        }
        None => Err(trs("odada değilsin", "you're not in a room").into()),
    }
}

#[tauri::command]
pub fn sync_play(state: State<'_, AppState>) -> CmdResult {
    sync_send(&state, UiCmd::Play)
}

/// Keşif/çark/altyazı mesajı (PROTOCOL §3.9–3.10). Yalnızca bu tipler odaya gider.
#[tauri::command]
pub fn sync_fun(state: State<'_, AppState>, msg: serde_json::Value) -> CmdResult {
    const ALLOWED: &[&str] = &[
        "discover",
        "swipe",
        "propose",
        "unpropose",
        "spin",
        "wheel_clear",
        "night_start",
        "night_close",
        "night_ready",
        "night_preview",
        "night_suggest",
        "night_search",
        "night_detail",
        "night_pick",
        "night_unpick",
        "night_done",
        "call",
        "subs_search",
        "subs_pick",
        "queue_vote",
        "queue_remove",
        "queue_next",
        "track_end",
        "react",
        "close_room",
    ];
    let t = msg["t"].as_str().unwrap_or_default();
    if !ALLOWED.contains(&t) {
        return Err(tr!(
            "izin verilmeyen mesaj: {t}",
            "message not allowed: {t}"
        ));
    }
    let text = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
    if text.len() > 4096 {
        return Err(trs("mesaj çok uzun", "message too long").into());
    }
    sync_send(&state, UiCmd::Fun(text))
}

/// Lounge kuyruğuna web adresi ekler (PROTOCOL §3.11, §5). Adres burada çözülür ki
/// başlık görünsün ve yt-dlp'nin tanımadığı site kuyruğa girmesin; çalma sırası gelince
/// herkes kendisi yeniden çözer. Yerel dosya kabul edilmez: Santral'a hiçbir dosya
/// yüklenmez. Dönen değer başlıktır. `title` verilirse
/// (çalma listesindeki şarkı: yt-dlp listesinden geldi) adres yeniden çözülmez.
#[tauri::command]
pub async fn queue_add(app: AppHandle, src: String, title: Option<String>) -> CmdResult<String> {
    let known = title
        .map(|t| t.trim().chars().take(200).collect::<String>())
        .filter(|t| !t.is_empty());
    let (url, title) = match (Source::parse(&src)?, known) {
        (Source::Web(url), Some(title)) => (url, title),
        (Source::Web(url), None) => {
            let u = url.clone();
            let res =
                tauri::async_runtime::spawn_blocking(move || media::resolve_web(&u, None, "", &[]))
                    .await
                    .map_err(|e| e.to_string())?;
            match res {
                Ok(s) => (url, s.title),
                Err(media::WebError::Unsupported(_)) => {
                    return Err(trs(
                        "bu site kuyrukta desteklenmiyor",
                        "this site isn't supported in the queue",
                    )
                    .into())
                }
                Err(e) => return Err(e.into()),
            }
        }
        (Source::File(_), _) => {
            return Err(trs(
                "sıraya yalnızca link eklenir; sunucuya dosya yüklenmez",
                "only links can be queued; files aren't uploaded to the server",
            )
            .into())
        }
    };
    let msg = serde_json::json!({
        "t": "queue_add",
        "media": { "kind": "url", "src": url, "title": title },
    });
    let state = app.state::<AppState>();
    sync_send(&state, UiCmd::Fun(msg.to_string()))?;
    Ok(title)
}

/// Oynatılan yerel dosyanın OpenSubtitles özeti (`moviehash`); akışta ya da çok
/// küçük dosyada `null`.
#[tauri::command]
pub fn subs_hash(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    let Some(path) = player(&state)?.get_property_string("path") else {
        return Ok(None);
    };
    if !std::path::Path::new(&path).is_file() {
        return Ok(None);
    }
    Ok(media::movie_hash(std::path::Path::new(&path)).ok())
}

/// Odaya gelen altyazı metnini önbelleğe yazar, ekler ve seçer.
#[tauri::command]
pub fn player_add_subtitle_text(app: AppHandle, name: String, text: String) -> CmdResult<String> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("subs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut file: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "._- ".contains(c) {
                c
            } else {
                '_'
            }
        })
        .take(120)
        .collect();
    let ext = std::path::Path::new(&file)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(ext.as_deref(), Some("srt" | "ass" | "ssa" | "vtt" | "sub")) {
        file.push_str(".srt");
    }
    let path = dir.join(file);
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    let p = path.to_string_lossy().to_string();
    player(&app.state::<AppState>())?
        .command(&["sub-add", &p, "select"])
        .map_err(|e| e.to_string())?;
    Ok(name)
}

#[tauri::command]
pub fn sync_pause(state: State<'_, AppState>) -> CmdResult {
    sync_send(&state, UiCmd::Pause)
}

#[tauri::command]
pub fn sync_seek(state: State<'_, AppState>, seconds: f64) -> CmdResult {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(tr!(
            "geçersiz konum: {seconds}",
            "invalid position: {seconds}"
        ));
    }
    sync_send(&state, UiCmd::Seek(seconds))
}

/// Odada çalanı herkes için kapat (oda açık kalır).
#[tauri::command]
pub fn sync_close(state: State<'_, AppState>) -> CmdResult {
    crate::tracks::clear();
    if let Ok(mpv) = player(&state) {
        let _ = mpv.command(&["stop"]);
    }
    if state.sync.lock().unwrap().is_some() {
        sync_send(&state, UiCmd::Close)
    } else {
        Ok(())
    }
}

/// Host ya da yardımcı host: kronik stall eden üyeyi beklemeden devam et.
#[tauri::command]
pub fn sync_continue_without(state: State<'_, AppState>, user: String) -> CmdResult {
    sync_send(&state, UiCmd::ContinueWithout(user))
}

/// Host: üyeyi yardımcı host yap (`on`) ya da al.
#[tauri::command]
pub fn sync_set_cohost(state: State<'_, AppState>, user: String, on: bool) -> CmdResult {
    sync_send(&state, UiCmd::SetCohost(user, on))
}

/// Host: "yalnızca host kontrol eder" (`host_only`) açık/kapalı.
#[tauri::command]
pub fn sync_set_policy(state: State<'_, AppState>, host_only: bool) -> CmdResult {
    let p = if host_only {
        Policy::HostOnly
    } else {
        Policy::Free
    };
    sync_send(&state, UiCmd::SetPolicy(p))
}

/// Oda modu (p0.13): `browse` (Keşfet) | `watch` (Sinema) | `listen` (Müzik). Herkeste değişir.
#[tauri::command]
pub fn sync_set_view(state: State<'_, AppState>, view: String) -> CmdResult {
    if !matches!(view.as_str(), "browse" | "watch" | "listen") {
        return Err(trs("geçersiz oda modu", "invalid room mode").into());
    }
    let cid = format!(
        "view-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis())
    );
    let msg = serde_json::json!({ "t": "cmd", "cid": cid, "op": "set_view", "view": view });
    sync_send(&state, UiCmd::Fun(msg.to_string()))
}

/// Keşfet: YouTube araması (yt-dlp `ytsearch`, anahtar gerekmez).
#[tauri::command]
pub async fn web_search(query: String) -> CmdResult<Vec<koni_resolver::Hit>> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = koni_resolver::YtDlpConfig {
            binary: media::ytdlp_bin(),
            ..Default::default()
        };
        koni_resolver::search(&query, 18, &cfg, &koni_resolver::ProcessRunner)
            .map_err(|e| media::resolve_err(&e))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Çalma listesine ekleme: linkteki şarkılar (YouTube çalma listesi ya da tek video;
/// Spotify parça/albüm/liste linkindeki şarkılar YouTube'da eşlenir).
#[tauri::command]
pub async fn media_list(src: String) -> CmdResult<Vec<koni_resolver::Hit>> {
    if let Some((kind, id)) = crate::spotify::parse(&src) {
        let list = crate::spotify::tracks(kind, &id).await?;
        let hits = tauri::async_runtime::spawn_blocking(move || {
            let cfg = koni_resolver::YtDlpConfig {
                binary: media::ytdlp_bin(),
                ..Default::default()
            };
            crate::spotify::match_youtube(list, |q| {
                koni_resolver::search(q, 1, &cfg, &koni_resolver::ProcessRunner)
                    .ok()
                    .and_then(|h| h.into_iter().find(|h| !h.live))
            })
        })
        .await
        .map_err(|e| e.to_string())?;
        if hits.is_empty() {
            return Err(trs(
                "Spotify şarkıları YouTube'da bulunamadı",
                "Couldn't find the Spotify songs on YouTube",
            )
            .into());
        }
        return Ok(hits);
    }
    let url = match Source::parse(&src)? {
        Source::Web(url) => url,
        Source::File(_) => {
            return Err(trs(
                "çalma listesine yalnızca link eklenir",
                "only links can be added to a playlist",
            )
            .into())
        }
    };
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = koni_resolver::YtDlpConfig {
            binary: media::ytdlp_bin(),
            ..Default::default()
        };
        koni_resolver::playlist(&url, &cfg, &koni_resolver::ProcessRunner)
            .map_err(|e| media::resolve_err(&e))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// "Odayla Senkronla" (DETACHED'dan dönüş).
#[tauri::command]
pub fn sync_resync(state: State<'_, AppState>) -> CmdResult {
    sync_send(&state, UiCmd::Resync)
}

/// "Herkes İndirsin": sayfa herkeste önbelleğe indirilir, hazır olunca oda başlar.
#[tauri::command]
pub fn sync_download(state: State<'_, AppState>, url: String) -> CmdResult {
    match Source::parse(&url)? {
        Source::Web(url) => sync_send(&state, UiCmd::OpenDownload(url)),
        Source::File(_) => Err(trs(
            "indirmek için web adresi gerekli",
            "a web address is needed to download",
        )
        .into()),
    }
}

#[tauri::command]
pub fn sync_leave(state: State<'_, AppState>) -> CmdResult {
    if let Some(h) = state.sync.lock().unwrap().take() {
        let _ = h.tx.send(UiCmd::Leave);
    }
    // Odadan çıkınca oda içeriği arkada çalmaya devam etmesin.
    if let Ok(p) = player(&state) {
        let _ = p.command(&["stop"]);
    }
    Ok(())
}

/// Müzik: sıradaki şarkı çalan bitmeden çözülür, geçişte yt-dlp beklenmez. Odada da
/// herkes kendisi çözer (akış adresleri IP'ye bağlı).
#[tauri::command]
pub async fn media_prefetch(src: String) -> CmdResult {
    let Source::Web(url) = Source::parse(&src)? else {
        return Ok(());
    };
    tauri::async_runtime::spawn_blocking(move || media::prefetch(&url))
        .await
        .map_err(|e| e.to_string())
}

/// Müzikte seek çubuğunun dalga formu (`waveform`, 0–1 arası sütunlar); görüntülü,
/// çok uzun ya da süresi bilinmeyen medyada `null`.
#[tauri::command]
pub async fn media_waveform(app: AppHandle) -> CmdResult<Option<Vec<f32>>> {
    let state = app.state::<AppState>();
    let mpv = player(&state)?;
    let Some(src) = mpv.get_property_string("path") else {
        return Ok(None);
    };
    if let Some(v) = crate::waveform::cached(&src) {
        return Ok(Some(v));
    }
    if mpv.duration().is_none_or(|d| d > crate::waveform::MAX_SECS)
        || !crate::lounge::is_music(&mpv)
    {
        return Ok(None);
    }
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let wav = dir.join("waveform.pcm");
    tauri::async_runtime::spawn_blocking(move || crate::waveform::compute(&src, &wav))
        .await
        .map_err(|e| e.to_string())?
        .map(Some)
}

// ---- Real-Debrid + addon'lar ----------------------

#[derive(serde::Serialize)]
pub struct DebridStatus {
    /// Anahtar kayıtlı mı.
    configured: bool,
    user: Option<crate::debrid::User>,
    /// Anahtar kayıtlı ama hesap okunamadı (geçersiz anahtar, ağ).
    error: Option<String>,
    /// Kullanıcının anahtarını aldığı sayfa.
    key_page: &'static str,
}

#[tauri::command]
pub async fn debrid_status() -> CmdResult<DebridStatus> {
    let mut st = DebridStatus {
        configured: false,
        user: None,
        error: None,
        key_page: crate::debrid::KEY_PAGE,
    };
    if let Some(key) = crate::debrid::key() {
        st.configured = true;
        match crate::debrid::user(&key).await {
            Ok(u) => st.user = Some(u),
            Err(e) => st.error = Some(e),
        }
    }
    Ok(st)
}

/// Anahtarı hesapla doğrulayıp anahtar zincirine yazar.
#[tauri::command]
pub async fn debrid_set_key(key: String) -> CmdResult<crate::debrid::User> {
    let key = key.trim();
    if key.is_empty() {
        return Err(trs("anahtar boş", "the key is empty").into());
    }
    let user = crate::debrid::user(key).await?;
    crate::debrid::set_key(key)?;
    Ok(user)
}

/// Ayarlar: bağlantıyı sunucu üzerinden yönlendir (açılışta ve değişince arayüz söyler).
#[tauri::command]
pub fn set_relay(on: bool) {
    crate::debrid::set_relay(on);
}

#[tauri::command]
pub fn debrid_clear_key() -> CmdResult {
    crate::debrid::clear_key()
}

fn addons_dir(app: &AppHandle) -> CmdResult<std::path::PathBuf> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn addons_list(app: AppHandle) -> CmdResult<Vec<crate::addons::Addon>> {
    Ok(crate::addons::load(&addons_dir(&app)?))
}

#[tauri::command]
pub async fn addon_add(app: AppHandle, url: String) -> CmdResult<Vec<crate::addons::Addon>> {
    crate::addons::add(&addons_dir(&app)?, &url).await
}

#[tauri::command]
pub fn addon_remove(app: AppHandle, url: String) -> CmdResult<Vec<crate::addons::Addon>> {
    crate::addons::remove(&addons_dir(&app)?, &url)
}

#[tauri::command]
pub async fn catalog_search(query: String) -> CmdResult<Vec<crate::addons::Title>> {
    crate::addons::search(&query).await
}

#[tauri::command]
pub async fn catalog_popular() -> CmdResult<Vec<crate::addons::Title>> {
    crate::addons::popular().await
}

#[tauri::command]
pub async fn catalog_episodes(id: String) -> CmdResult<Vec<crate::addons::Episode>> {
    crate::addons::episodes(&id).await
}

/// `kind`: `movie` | `series`; `id`: `tt…` ya da `tt…:sezon:bölüm`.
#[tauri::command]
pub async fn addon_streams(
    app: AppHandle,
    kind: String,
    id: String,
) -> CmdResult<crate::addons::StreamList> {
    let addons = crate::addons::load(&addons_dir(&app)?);
    if addons.is_empty() {
        return Err(trs(
            "Önce bir kaynak addon'u ekle (Sağlayıcı & addon'lar)",
            "Add a source addon first (Provider & addons)",
        )
        .into());
    }
    crate::addons::streams(&addons, &kind, &id).await
}

/// Otomatik kaynak seçimi: torrent kullanıcının Real-Debrid'inde açılıyor mu (önbellekte
/// yoksa ya da açılmıyorsa hata). Oynatmaz; açılan torrent hesapta hazır kalır,
/// "Oynat"taki `debrid_open` onu yeniden kullanır.
#[tauri::command]
pub async fn debrid_check(
    hash: Option<String>,
    url: Option<String>,
    name: Option<String>,
    raw_title: Option<String>,
    file: String,
    idx: Option<u32>,
) -> CmdResult<()> {
    let clean_hash = hash
        .as_deref()
        .map(|h| h.trim().to_ascii_lowercase())
        .filter(|h| h.len() == 40 && h.bytes().all(|b| b.is_ascii_hexdigit()));
    let meta = format!(
        "{}\n{}",
        name.as_deref().unwrap_or_default(),
        raw_title.as_deref().unwrap_or_default()
    );

    if let Some(u) = url.filter(|u| !u.is_empty()) {
        crate::debrid::resolve_addon_url(clean_hash.as_deref(), &u, &meta, "")
            .await
            .map(|_| ())
    } else if let Some(h) = clean_hash.filter(|h| !crate::addons::is_synthetic_hash(h)) {
        crate::debrid::stream(&h, &file, idx, "").await.map(|_| ())
    } else {
        Err(trs("geçersiz kaynak", "invalid source").into())
    }
}

/// Medya kaynağını açar:
/// - Önce açan istemcinin kendisinde çözüldüğünü doğrular (ve önbelleğe alır).
/// - Odadaysa infoHash + `kind:id|hint` bilgisini odaya gönderir; odadaki herkes aynı
///   başlığı kendi eklediği addon'larıyla (veya yerel RD anahtarıyla) çözer.
/// - Tek başınaysa doğrudan yerel oynatıcıya yükler.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri komutu: parametreler doğrudan JS'ten gelir
pub async fn debrid_open(
    app: AppHandle,
    hash: Option<String>,
    url: Option<String>,
    name: Option<String>,
    raw_title: Option<String>,
    file: String,
    idx: Option<u32>,
    title: String,
) -> CmdResult<String> {
    let clean_hash = hash
        .as_deref()
        .map(|h| h.trim().to_ascii_lowercase())
        .filter(|h| h.len() == 40 && h.bytes().all(|b| b.is_ascii_hexdigit()))
        .unwrap_or_else(|| crate::addons::synthetic_hash(&format!("{file}|{title}")));
    let meta = format!(
        "{}\n{}",
        name.as_deref().unwrap_or_default(),
        raw_title.as_deref().unwrap_or_default()
    );

    let s = if let Some(u) = url.filter(|u| !u.is_empty()) {
        let mut s = crate::debrid::resolve_addon_url(Some(&clean_hash), &u, &meta, &title).await?;
        crate::debrid::attach_subtitles(&app, &mut s, &file, &title).await;
        crate::debrid::cache_stream(&clean_hash, &s);
        s
    } else {
        crate::debrid::resolve_room_media(&app, &clean_hash, &file, idx, &title).await?
    };

    let state = app.state::<AppState>();
    let room = state.sync.lock().unwrap().as_ref().map(|h| h.tx.clone());

    if let Some(tx) = room {
        let m = koni_sync::protocol::Media {
            file,
            idx,
            ..koni_sync::protocol::Media::new(
                koni_sync::protocol::MediaKind::Debrid,
                clean_hash,
                title.clone(),
            )
        };
        tx.send(UiCmd::OpenDebrid(m))
            .map_err(|_| trs("oda bağlantısı kapandı", "room connection closed").to_string())?;
        return Ok(title);
    }

    let mpv = player(&state)?;
    media::load(&mpv, &s)?;
    Ok(s.title)
}

#[tauri::command]
pub async fn subs_search_episode(
    app: AppHandle,
    file: Option<String>,
    title: String,
    lang: String,
) -> CmdResult<Vec<crate::addons::FoundSub>> {
    let addons = addons_dir(&app)
        .ok()
        .map(|d| crate::addons::load(&d))
        .unwrap_or_default();
    Ok(
        crate::addons::search_subtitles(
            &addons,
            file.as_deref().unwrap_or_default(),
            &title,
            &lang,
        )
        .await,
    )
}

#[tauri::command]
pub async fn subs_apply_url(app: AppHandle, url: String, name: String) -> CmdResult<String> {
    let text = crate::addons::download_subtitle_text(&url).await?;
    player_add_subtitle_text(app, name, text)
}

#[cfg(any())]
fn _keep_end() -> CmdResult<String> {
    Ok(String::new())
}

/// Arayüz dili: `en` true ise Rust'tan gelen metinler İngilizce olur.
#[tauri::command]
pub fn set_lang(en: bool) {
    crate::i18n::set_en(en);
}
