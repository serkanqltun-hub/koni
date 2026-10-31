//! Real-Debrid, "herkes kendi anahtarı" (ADR 0007, docs/design/debrid.md §8): odaya
//! yalnızca torrent infoHash'i gider; her üye onu kendi hesabıyla doğrudan çözer.
//! Proxy yok, içerik VPS'ten geçmez, anahtar bu bilgisayardan çıkmaz.
//!
//! RD'nin önbellek sorgusu (`instantAvailability`) kapatıldığı için önbellek kontrolü
//! açma anında yapılır: torrent eklenir, dosya seçilir; birkaç saniyede `downloaded`
//! olmazsa önbellekte değildir ve torrent hesaptan silinir.

use crate::i18n::trs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::addons::http;

const DIRECT_API: &str = "https://api.real-debrid.com/rest/1.0";
/// Doğrudan bağlantı kurulamazsa giriş yapılan Santral'ın `/api/rd/` adresi kullanılır.
/// Yalnızca KONİ hesabı olanlara açık: bu sunucudaki oturum `X-Koni-Token` ile gider.
static SERVER: Mutex<String> = Mutex::new(String::new());

/// Giriş yapılan Santral adresi (`wss://…/ws`).
pub fn set_server(url: &str) {
    *SERVER.lock().unwrap() = url.to_string();
}

/// Santral API adresi ve oradaki oturum belirteci; giriş yoksa `None`.
fn proxy() -> Option<(String, String)> {
    let ws = SERVER.lock().unwrap().clone();
    if ws.is_empty() {
        return None;
    }
    let acc = crate::account::get(&ws)?;
    Some((
        format!("{}/api/rd", crate::account::http_base(&ws)),
        acc.token,
    ))
}

fn no_session() -> &'static str {
    trs(
        "Bağlantı için KONİ hesabınla giriş yap",
        "Sign in with your KONİ account to connect",
    )
}

/// Doğrudan bağlantı kurulamadıysa sonraki çağrılar Santral üzerinden gider.
static USE_PROXY: AtomicBool = AtomicBool::new(false);
/// Ayarlar → "Bağlantıyı sunucu üzerinden yönlendir" (varsayılan kapalı).
static RELAY: AtomicBool = AtomicBool::new(false);

pub fn set_relay(on: bool) {
    RELAY.store(on, Ordering::Relaxed);
    if !on {
        USE_PROXY.store(false, Ordering::Relaxed);
    }
}
/// Kullanıcı anahtarını buradan alır.
pub const KEY_PAGE: &str = "https://real-debrid.com/apitoken";
const SERVICE: &str = "app.koni.desktop";
const ACCOUNT: &str = "real-debrid";

const VIDEO_EXT: &[&str] = &[
    "mkv", "mp4", "m4v", "avi", "mov", "webm", "ts", "m2ts", "wmv", "flv", "mpg", "mpeg",
];

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| tr!("anahtar zinciri: {e}", "keychain: {e}"))
}

/// Kayıtlı API anahtarı (yoksa `None`).
pub fn key() -> Option<String> {
    entry().ok()?.get_password().ok().filter(|k| !k.is_empty())
}

pub fn set_key(key: &str) -> Result<(), String> {
    entry()?
        .set_password(key)
        .map_err(|e| tr!("anahtar zinciri: {e}", "keychain: {e}"))
}

pub fn clear_key() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(tr!("anahtar zinciri: {e}", "keychain: {e}")),
    }
}

/// Kayıtlı anahtar yoksa kullanıcıya gösterilecek hata.
pub fn no_key() -> &'static str {
    trs(
        "Kaynak yok: Ayarlar → Sağlayıcı & addon'lar'dan kendi addon'unu ekle",
        "No source: add your own addon in Settings → Provider & addons",
    )
}

// Arayüz bu metinlerde anahtar sözcük arar (`sources.ts` `failKind`): Türkçede
// "çok fazla istek" / "önbellek", İngilizcede "too many requests" / "not cached".
fn too_many() -> &'static str {
    trs(
        "Servis çok fazla istek dedi, biraz sonra dene",
        "The service says too many requests, try again in a bit",
    )
}

fn unavailable() -> &'static str {
    trs(
        "Servis bu kaynağı açmadı; başka bir kaynak seç",
        "The service didn't open this source; pick another one",
    )
}

fn not_cached() -> &'static str {
    trs(
        "Bu kaynak şu an hazır değil (önbellekte yok); başka bir kaynak seç",
        "This source isn't ready right now (not cached); pick another one",
    )
}

#[derive(Deserialize)]
struct ApiError {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_code: i64,
}

/// RD hata kodlarını kullanıcı diline çevirir.
fn api_error(status: reqwest::StatusCode, body: &[u8]) -> String {
    let e: Option<ApiError> = serde_json::from_slice(body).ok();
    match (status.as_u16(), e.as_ref().map(|e| e.error_code)) {
        // JSON'suz 401 RD'den değil Santral'dan gelir: KONİ oturumu yok ya da eski.
        (401, None) => no_session().into(),
        (401, _) | (_, Some(8)) => trs("API anahtarı geçersiz", "Invalid API key").into(),
        (_, Some(20)) | (403, _) => trs(
            "Hesap premium değil ya da kilitli",
            "Account isn't premium or is locked",
        )
        .into(),
        (_, Some(34)) | (429, _) => too_many().into(),
        (_, Some(35)) => unavailable().into(),
        _ => tr!(
            "Bağlantı hatası: {}",
            "Connection error: {}",
            e.map(|e| e.error)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| status.to_string())
        ),
    }
}

async fn api_request(
    method: reqwest::Method,
    path: &str,
    key: &str,
    form: Option<&[(&str, &str)]>,
) -> Result<reqwest::Response, String> {
    let make = |base: &str, token: Option<&str>| {
        let mut r = http()
            .request(method.clone(), format!("{base}{path}"))
            .bearer_auth(key);
        if let Some(t) = token {
            r = r.header("X-Koni-Token", t);
        }
        if let Some(f) = form {
            r = r.form(f);
        }
        r
    };

    let relay = RELAY.load(Ordering::Relaxed);
    if relay && USE_PROXY.load(Ordering::Relaxed) {
        let Some((api, token)) = proxy() else {
            return Err(no_session().into());
        };
        return make(&api, Some(&token)).send().await.map_err(|e| {
            tr!(
                "Bağlantı proxy hatası: {}",
                "Proxy connection error: {}",
                e.without_url()
            )
        });
    }

    match make(DIRECT_API, None).send().await {
        Ok(res) => Ok(res),
        Err(err) if !relay => Err(tr!(
            "Servise ulaşılamadı: {}",
            "Couldn't reach the service: {}",
            err.without_url()
        )),
        Err(err) => {
            // Doğrudan bağlantı kurulamadı: Santral üzerinden dene, sonraki çağrılarda da onu kullan.
            USE_PROXY.store(true, Ordering::Relaxed);
            let Some((api, token)) = proxy() else {
                return Err(no_session().into());
            };
            make(&api, Some(&token)).send().await.map_err(|e| {
                tr!(
                    "Servise ulaşılamadı (doğrudan: {}, proxy: {})",
                    "Couldn't reach the service (direct: {}, proxy: {})",
                    err.without_url(),
                    e.without_url()
                )
            })
        }
    }
}

async fn call<T: for<'de> Deserialize<'de>>(
    method: reqwest::Method,
    path: &str,
    key: &str,
    form: Option<&[(&str, &str)]>,
) -> Result<T, String> {
    // 429 (dakikada ~250 istek sınırı): kısa bekleyip iki kez daha dene.
    let mut tries = 0;
    let (status, body) = loop {
        let res = api_request(method.clone(), path, key, form).await?;
        let status = res.status();
        let body = res.bytes().await.map_err(|e| e.to_string())?;
        if status.as_u16() == 429 && tries < 2 {
            tries += 1;
            tokio::time::sleep(Duration::from_secs(3 * tries)).await;
            continue;
        }
        break (status, body);
    };
    if !status.is_success() {
        return Err(api_error(status, &body));
    }
    // 204 No Content: T = () için "null" gibi davran.
    let body: &[u8] = if body.is_empty() { b"null" } else { &body };
    serde_json::from_slice(body).map_err(|e| {
        tr!(
            "Servis yanıtı okunamadı: {e}",
            "Couldn't read the service response: {e}"
        )
    })
}

async fn get<T: for<'de> Deserialize<'de>>(key: &str, path: &str) -> Result<T, String> {
    call(reqwest::Method::GET, path, key, None).await
}

async fn post<T: for<'de> Deserialize<'de>>(
    key: &str,
    path: &str,
    form: &[(&str, &str)],
) -> Result<T, String> {
    call(reqwest::Method::POST, path, key, Some(form)).await
}

#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    /// `premium` | `free`
    #[serde(rename = "type")]
    pub kind: String,
    /// Premium bitişi (ISO 8601).
    #[serde(default)]
    pub expiration: String,
}

pub async fn user(key: &str) -> Result<User, String> {
    get(key, "/user").await
}

#[derive(Debug, Deserialize)]
struct Added {
    id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Listed {
    id: String,
    hash: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct Info {
    status: String,
    #[serde(default)]
    files: Vec<File>,
    #[serde(default)]
    links: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct File {
    /// 1 tabanlı, torrentteki sıra.
    pub id: u32,
    pub path: String,
    pub bytes: u64,
    #[serde(default)]
    pub selected: u8,
}

#[derive(Deserialize)]
struct Unrestricted {
    download: String,
    #[serde(default)]
    filename: String,
}

/// Çözülmüş doğrudan adres.
pub struct Resolved {
    pub url: String,
    pub filename: String,
}

fn is_video(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, e)| VIDEO_EXT.contains(&e.to_ascii_lowercase().as_str()))
}

fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Oynatılacak dosya: önce addon'un verdiği dosya adı, sonra Stremio `fileIdx`
/// (0 tabanlı; RD kimliği 1 tabanlı), yoksa en büyük video dosyası.
pub fn pick_file(files: &[File], name: &str, idx: Option<u32>) -> Option<u32> {
    let ep_tag = crate::addons::parse_content_file(name).and_then(|(kind, id, _)| {
        if kind == "series" {
            let mut parts = id.split(':');
            let _imdb = parts.next()?;
            let s: u32 = parts.next()?.parse().ok()?;
            let e: u32 = parts.next()?.parse().ok()?;
            Some(format!("s{s:02}e{e:02}"))
        } else {
            None
        }
    });
    let clean_name = name.split_once('|').map(|(_, f)| f).unwrap_or(name);
    let videos = || files.iter().filter(|f| is_video(&f.path));
    if !clean_name.is_empty() {
        if let Some(f) = videos().find(|f| base_name(&f.path).eq_ignore_ascii_case(clean_name)) {
            return Some(f.id);
        }
    }
    if let Some(i) = idx {
        if let Some(f) = videos().find(|f| f.id == i + 1) {
            return Some(f.id);
        }
    }
    if let Some(ref tag) = ep_tag {
        if let Some(f) = videos().find(|f| f.path.to_ascii_lowercase().contains(tag)) {
            return Some(f.id);
        }
    }
    videos().max_by_key(|f| f.bytes).map(|f| f.id)
}

/// Seçili dosyanın `links` içindeki sırası: RD, seçili dosyaların bağlantılarını
/// dosya sırasıyla verir.
fn link_index(files: &[File], id: u32) -> Option<usize> {
    let mut selected: Vec<u32> = files
        .iter()
        .filter(|f| f.selected == 1)
        .map(|f| f.id)
        .collect();
    selected.sort_unstable();
    selected.iter().position(|&s| s == id)
}

async fn info(key: &str, id: &str) -> Result<Info, String> {
    get(key, &format!("/torrents/info/{id}")).await
}

/// `status` istenen durumlardan birine gelene (ya da hataya düşene) kadar bekler.
/// `give_up` durumlarından birinde beklemeden döner (ör. önbellekte olmayan torrent
/// seçimden hemen sonra `downloading` olur; 6 sn beklemek boşuna istek harcar).
async fn wait_status(
    key: &str,
    id: &str,
    until: &[&str],
    give_up: &[&str],
    max: Duration,
) -> Result<Info, String> {
    let deadline = tokio::time::Instant::now() + max;
    loop {
        let i = info(key, id).await?;
        if until.contains(&i.status.as_str()) || give_up.contains(&i.status.as_str()) {
            return Ok(i);
        }
        if matches!(
            i.status.as_str(),
            "magnet_error" | "error" | "virus" | "dead"
        ) {
            return Err(tr!(
                "Bağlantı açılamadı ({})",
                "Couldn't open the link ({})",
                i.status
            ));
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(i);
        }
        tokio::time::sleep(Duration::from_millis(1200)).await;
    }
}

async fn delete(key: &str, id: &str) {
    let _: Result<(), String> = call(
        reqwest::Method::DELETE,
        &format!("/torrents/delete/{id}"),
        key,
        None,
    )
    .await;
}

async fn unrestrict(key: &str, link: &str) -> Result<Resolved, String> {
    let u: Unrestricted = post(key, "/unrestrict/link", &[("link", link)]).await?;
    Ok(Resolved {
        url: u.download,
        filename: u.filename,
    })
}

/// Hesaptaki torrent listesi; kaynak ararken her adayda yeniden istenmesin diye 30 sn saklanır.
static LISTED: std::sync::Mutex<Option<(std::time::Instant, Vec<Listed>)>> =
    std::sync::Mutex::new(None);

async fn listed(key: &str) -> Option<Vec<Listed>> {
    if let Some((at, l)) = LISTED.lock().unwrap().as_ref() {
        if at.elapsed() < Duration::from_secs(30) {
            return Some(l.clone());
        }
    }
    let list: Vec<Listed> = get(key, "/torrents?limit=100").await.ok()?;
    *LISTED.lock().unwrap() = Some((std::time::Instant::now(), list.clone()));
    Some(list)
}

/// Hesapta bu torrent zaten hazırsa ve istenen dosya seçiliyse onun bağlantısı;
/// her yeniden açılışta hesaba yeni torrent eklenmesin.
async fn existing(key: &str, hash: &str, file: &str, idx: Option<u32>) -> Option<String> {
    let list = listed(key).await?;
    for t in list
        .iter()
        .filter(|t| t.hash.eq_ignore_ascii_case(hash) && t.status == "downloaded")
    {
        let Ok(i) = info(key, &t.id).await else {
            continue;
        };
        let Some(fid) = pick_file(&i.files, file, idx) else {
            continue;
        };
        if let Some(link) = link_index(&i.files, fid).and_then(|n| i.links.get(n)) {
            return Some(link.clone());
        }
    }
    None
}

/// infoHash + dosya ipucunu kullanıcının kendi hesabıyla doğrudan bir adrese çevirir.
pub async fn resolve(
    key: &str,
    hash: &str,
    file: &str,
    idx: Option<u32>,
) -> Result<Resolved, String> {
    if let Some(link) = existing(key, hash, file, idx).await {
        return unrestrict(key, &link).await;
    }
    let magnet = format!("magnet:?xt=urn:btih:{hash}");
    let added: Added = post(key, "/torrents/addMagnet", &[("magnet", &magnet)]).await?;
    let id = added.id;
    let res = async {
        let i = wait_status(
            key,
            &id,
            &["waiting_files_selection", "downloaded"],
            &[],
            Duration::from_secs(15),
        )
        .await?;
        let fid = pick_file(&i.files, file, idx).ok_or(trs(
            "kaynakta oynatılacak video dosyası yok",
            "no playable video file in this source",
        ))?;
        if i.status == "waiting_files_selection" {
            let fid = fid.to_string();
            call::<()>(
                reqwest::Method::POST,
                &format!("/torrents/selectFiles/{id}"),
                key,
                Some(&[("files", &fid)]),
            )
            .await?;
        }
        let i = wait_status(
            key,
            &id,
            &["downloaded"],
            &["downloading", "queued"],
            Duration::from_secs(6),
        )
        .await?;
        if i.status != "downloaded" {
            return Err(not_cached().to_string());
        }
        let link = link_index(&i.files, fid)
            .and_then(|n| i.links.get(n))
            .or(i.links.first())
            .ok_or(trs("Bağlantı alınamadı", "Couldn't get the link"))?
            .clone();
        unrestrict(key, &link).await
    }
    .await;
    if res.is_ok() {
        *LISTED.lock().unwrap() = None; // yeni hazır torrent listede görünsün
    }
    if res.is_err() {
        delete(key, &id).await; // önbellekte olmayan / hatalı torrent hesapta kalmasın
    }
    res
}

/// Oda (ya da tek başına) medyası: kayıtlı anahtarla çözüp oynatıcıya hazır akış.
/// `title` boşsa ya da sunucunun koyduğu hash ise dosya adı gösterilir.
/// Çözülmüş akış önbelleği (debrid_check sonrası debrid_open ve oda senkronunda
/// aynı kaynağı tekrar çözmemek için 90 sn tutulur).
static RECENT_STREAMS: std::sync::Mutex<Vec<(String, std::time::Instant, crate::media::Stream)>> =
    std::sync::Mutex::new(Vec::new());

pub fn cache_stream(key: &str, stream: &crate::media::Stream) {
    if key.is_empty() {
        return;
    }
    let mut guard = RECENT_STREAMS.lock().unwrap();
    guard.retain(|(k, at, _)| {
        at.elapsed() < Duration::from_secs(90) && !k.eq_ignore_ascii_case(key)
    });
    guard.push((
        key.to_ascii_lowercase(),
        std::time::Instant::now(),
        stream.clone(),
    ));
}

pub fn cached_stream(key: &str) -> Option<crate::media::Stream> {
    if key.is_empty() {
        return None;
    }
    let mut guard = RECENT_STREAMS.lock().unwrap();
    guard.retain(|(_, at, _)| at.elapsed() < Duration::from_secs(90));
    guard
        .iter()
        .find(|(k, _, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, _, s)| s.clone())
}

pub async fn stream(
    hash: &str,
    file: &str,
    idx: Option<u32>,
    title: &str,
) -> Result<crate::media::Stream, String> {
    if let Some(mut s) = cached_stream(hash) {
        if !title.is_empty() && !title.eq_ignore_ascii_case(hash) {
            s.title = title.to_string();
        }
        return Ok(s);
    }
    let key = key().ok_or(no_key())?;
    let r = resolve(&key, hash, file, idx).await?;
    let title = if title.is_empty() || title.eq_ignore_ascii_case(hash) {
        r.filename
    } else {
        title.to_string()
    };
    let s = crate::media::Stream::plain(r.url, None, title);
    cache_stream(hash, &s);
    Ok(s)
}

/// Debrid'li addon'un verdiği hazır url akışını doğrular, yönlendirmeyi takip eder ve
/// sahte uyarı videolarını (önbellekte yok / slate.mp4 vb.) eler.
pub async fn resolve_addon_url(
    hash: Option<&str>,
    url: &str,
    meta_text: &str,
    title: &str,
) -> Result<crate::media::Stream, String> {
    let meta_low = meta_text.to_ascii_lowercase();
    if meta_low.contains("download]")
        || meta_text.contains('⏳')
        || meta_text.contains("⬇️")
        || meta_low.contains("not cached")
    {
        return Err(not_cached().into());
    }
    if let Some(h) = hash {
        if let Some(mut s) = cached_stream(h) {
            if !title.is_empty() && !title.eq_ignore_ascii_case(h) {
                s.title = title.to_string();
            }
            return Ok(s);
        }
    }
    if let Some(mut s) = cached_stream(url) {
        if !title.is_empty() {
            s.title = title.to_string();
        }
        return Ok(s);
    }

    let res = http()
        .get(url)
        .header("Range", "bytes=0-0")
        .send()
        .await
        .map_err(|e| {
            tr!(
                "Bağlantı: kaynak açılamadı ({})",
                "Link: couldn't open the source ({})",
                e.without_url()
            )
        })?;

    let status = res.status();
    if status.as_u16() == 429 {
        return Err(too_many().into());
    }
    if matches!(status.as_u16(), 401 | 403) {
        return Err(trs(
            "API anahtarı geçersiz ya da hesap premium değil",
            "Invalid API key or the account isn't premium",
        )
        .into());
    }
    if status.as_u16() == 451 {
        return Err(unavailable().into());
    }
    if !status.is_success() && status.as_u16() != 206 {
        return Err(tr!(
            "Bağlantı: kaynak yanıt vermedi ({status})",
            "Link: the source didn't respond ({status})"
        ));
    }

    let final_url = res.url().to_string();
    let final_low = final_url.to_ascii_lowercase();
    if final_low.contains("infringing") || final_low.contains("dmca") {
        return Err(unavailable().into());
    }
    if final_low.contains("slate.elfhosted.com")
        || final_low.contains("slate.mp4")
        || final_low.contains("not-cached")
        || final_low.contains("not_cached")
        || final_low.contains("nocache")
        || final_low.contains("/videos/downloading")
        || final_low.contains("/videos/nocache")
        || final_low.contains("/videos/error")
    {
        return Err(not_cached().into());
    }

    let total_bytes = res
        .headers()
        .get("content-range")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.rsplit_once('/'))
        .and_then(|(_, tot)| tot.trim().parse::<u64>().ok())
        .or_else(|| {
            if status.as_u16() == 200 {
                res.headers()
                    .get("content-length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.trim().parse::<u64>().ok())
            } else {
                None
            }
        });
    if let Some(tot) = total_bytes {
        if tot > 0 && tot < 15_000_000 {
            return Err(not_cached().into());
        }
    }

    let display_title = if !title.is_empty() {
        title.to_string()
    } else {
        base_name(&final_url).to_string()
    };
    let s = crate::media::Stream::plain(final_url, None, display_title);
    cache_stream(url, &s);
    if let Some(h) = hash {
        cache_stream(h, &s);
    }
    Ok(s)
}

fn quality_tag(text: &str) -> &'static str {
    let low = text.to_ascii_lowercase();
    if low.contains("2160p") || low.contains("4k") || low.contains("uhd") {
        "4K"
    } else if low.contains("1440p") || low.contains("2k") {
        "2K"
    } else if low.contains("1080p") {
        "1080p"
    } else if low.contains("720p") {
        "720p"
    } else {
        "SD"
    }
}

/// Odadaki her üye (ya da kaldığı yerden devam eden kullanıcı) aynı içeriği kendi
/// eklediği addon'lardan arayıp kendi Debrid linkiyle (veya yerel RD anahtarıyla) çözer.
pub async fn attach_subtitles(
    app: &tauri::AppHandle,
    s: &mut crate::media::Stream,
    file: &str,
    title: &str,
) {
    use tauri::Manager;
    if !s.subs.is_empty() {
        return;
    }
    let addons = app
        .path()
        .app_config_dir()
        .ok()
        .map(|d| crate::addons::load(&d))
        .unwrap_or_default();
    let effective_title = if !title.is_empty() { title } else { &s.title };
    s.subs = crate::addons::subtitles_for_stream(&addons, file, effective_title).await;
}

/// Bu bilgisayar odadaki kişisel kaynaklı medyayı açabilir mi: anahtar ya da en az bir
/// eklenti var (veya akış zaten çözülmüş). Yoksa hiç denenmez; oda bu üyeyi beklemez.
pub fn can_open_room_media(app: &tauri::AppHandle, hash: &str) -> bool {
    use tauri::Manager;
    cached_stream(hash).is_some()
        || key().is_some()
        || app
            .path()
            .app_config_dir()
            .is_ok_and(|d| !crate::addons::load(&d).is_empty())
}

pub async fn resolve_room_media(
    app: &tauri::AppHandle,
    hash: &str,
    file: &str,
    idx: Option<u32>,
    title: &str,
) -> Result<crate::media::Stream, String> {
    let mut s = resolve_room_media_inner(app, hash, file, idx, title).await?;
    attach_subtitles(app, &mut s, file, title).await;
    cache_stream(hash, &s);
    Ok(s)
}

async fn resolve_room_media_inner(
    app: &tauri::AppHandle,
    hash: &str,
    file: &str,
    idx: Option<u32>,
    title: &str,
) -> Result<crate::media::Stream, String> {
    use tauri::Manager;
    if let Some(mut s) = cached_stream(hash) {
        if !title.is_empty() && !title.eq_ignore_ascii_case(hash) {
            s.title = title.to_string();
        }
        return Ok(s);
    }

    let has_local_key = key().is_some();
    let mut last_err = String::new();

    if let Some((kind, id, hint)) = crate::addons::parse_content_file(file) {
        let addons = app
            .path()
            .app_config_dir()
            .ok()
            .map(|d| crate::addons::load(&d))
            .unwrap_or_default();

        if !addons.is_empty() {
            if let Ok(list) = crate::addons::streams(&addons, kind, id).await {
                let target_q = quality_tag(hint);
                let hint_low = hint.to_ascii_lowercase();
                let mut candidates = list.sources;

                candidates.sort_by_key(|c| {
                    let all = format!("{}\n{}", c.name, c.title);
                    let all_low = all.to_ascii_lowercase();
                    let uncached =
                        all_low.contains("download]") || all.contains('⏳') || all.contains("⬇️");
                    let has_url = c.url.is_some() && !uncached;
                    let exact_hash = c
                        .hash
                        .as_deref()
                        .is_some_and(|h| h.eq_ignore_ascii_case(hash));
                    let hint_match = !hint_low.is_empty()
                        && (c.file.to_ascii_lowercase().contains(&hint_low)
                            || all_low.contains(&hint_low));
                    let cq = quality_tag(&all);
                    let q_score: i32 = if cq == target_q {
                        5
                    } else {
                        match cq {
                            "1080p" => 4,
                            "4K" => 3,
                            "2K" => 2,
                            "720p" => 1,
                            _ => 0,
                        }
                    };
                    std::cmp::Reverse((!uncached, has_url, exact_hash, hint_match, q_score))
                });

                for c in candidates.into_iter().take(12) {
                    if let Some(ref u) = c.url {
                        let meta = format!("{}\n{}", c.name, c.title);
                        match resolve_addon_url(c.hash.as_deref(), u, &meta, title).await {
                            Ok(s) => {
                                cache_stream(hash, &s);
                                return Ok(s);
                            }
                            Err(e) => last_err = e,
                        }
                    }
                    if has_local_key {
                        if let Some(ref ch) = c.hash {
                            if !crate::addons::is_synthetic_hash(ch) {
                                match stream(ch, &c.file, c.idx, title).await {
                                    Ok(s) => {
                                        cache_stream(hash, &s);
                                        return Ok(s);
                                    }
                                    Err(e) => last_err = e,
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !crate::addons::is_synthetic_hash(hash) && has_local_key {
        match stream(hash, file, idx, title).await {
            Ok(s) => {
                cache_stream(hash, &s);
                return Ok(s);
            }
            Err(e) => last_err = e,
        }
    }

    if !last_err.is_empty() {
        Err(last_err)
    } else if !has_local_key {
        Err(trs(
            "Oda medyasını açmak için kendi addon'unu ekle",
            "Add your own addon to open the room's media",
        )
        .into())
    } else {
        Err(trs(
            "Addon'larında bu başlık için açılabilen kaynak bulunamadı",
            "Your addons have no playable source for this title",
        )
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(id: u32, path: &str, bytes: u64, selected: u8) -> File {
        File {
            id,
            path: path.into(),
            bytes,
            selected,
        }
    }

    #[test]
    fn picks_named_then_indexed_then_largest_video() {
        let files = [
            f(1, "/Show/Sample.mkv", 50, 0),
            f(2, "/Show/S01E01.mkv", 900, 0),
            f(3, "/Show/S01E02.MKV", 800, 0),
            f(4, "/Show/huge.nfo", 9_999, 0),
        ];
        assert_eq!(
            pick_file(&files, "s01e02.mkv", Some(0)),
            Some(3),
            "name wins"
        );
        assert_eq!(
            pick_file(&files, "", Some(0)),
            Some(1),
            "fileIdx is 0-based"
        );
        assert_eq!(
            pick_file(&files, "missing.mkv", Some(3)),
            Some(2),
            "idx on a non-video falls back"
        );
        assert_eq!(
            pick_file(&files, "", None),
            Some(2),
            "largest video, never .nfo"
        );
        assert_eq!(pick_file(&[f(1, "a.txt", 1, 0)], "", None), None);
    }

    #[test]
    fn link_follows_selected_file_order() {
        let files = [
            f(5, "c.mkv", 1, 1),
            f(2, "a.mkv", 1, 1),
            f(3, "b.mkv", 1, 0),
        ];
        assert_eq!(link_index(&files, 5), Some(1));
        assert_eq!(link_index(&files, 2), Some(0));
        assert_eq!(link_index(&files, 3), None);
    }
}
