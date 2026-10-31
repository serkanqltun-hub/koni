//! Stremio addon protokolü: kullanıcının eklediği
//! addon'lardan kaynak listesi, Cinemeta'dan film/dizi araması.
//!
//! KONİ hiçbir kaynak addon'u içermez ya da önermez; listeyi kullanıcı kendisi ekler
//! Cinemeta yalnızca başlık/IMDb kimliği verir.

use crate::i18n::trs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Stremio'nun resmi meta addon'u: arama ve dizi bölümleri.
const CINEMETA: &str = "https://v3-cinemeta.strem.io";
const OPENSUBTITLES_V3: &str = "https://opensubtitles-v3.strem.io";
const MANIFEST: &str = "manifest.json";

/// reqwest `rustls-no-provider` ile gelir: ilk istekten önce ring sağlayıcısını kurar.
fn ensure_tls() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

/// Addon'lar ve Real-Debrid için ortak istemci.
pub fn http() -> &'static reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        ensure_tls();
        reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("KONI/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP istemcisi kurulamadı")
    });
    &CLIENT
}

async fn get_json<T: for<'de> Deserialize<'de>>(url: &str) -> Result<T, String> {
    let res = http()
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.without_url().to_string())?;
    res.json()
        .await
        .map_err(|e| tr!("geçersiz yanıt: {e}", "invalid response: {e}"))
}

/// Kullanıcının eklediği bir addon. `url` manifest adresidir; addon ayarlarını (bazen
/// gizli anahtarları) içerebilir, bu yüzden yalnızca bu bilgisayarda durur, odaya gitmez.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Addon {
    pub url: String,
    pub name: String,
}

impl Addon {
    /// Kaynak isteklerinin kökü (`…/manifest.json` olmadan).
    fn base(&self) -> &str {
        self.url
            .strip_suffix(MANIFEST)
            .unwrap_or(&self.url)
            .trim_end_matches('/')
    }
}

/// `stremio://` ve sonundaki `/` farklarını giderir; `manifest.json` ile bitmeli.
pub fn normalize_manifest(url: &str) -> Result<String, String> {
    let url = url.trim();
    let url = match url.strip_prefix("stremio://") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_string(),
    };
    let parsed = reqwest::Url::parse(&url)
        .map_err(|_| trs("geçersiz adres", "invalid address").to_string())?;
    if !matches!(parsed.scheme(), "https" | "http") {
        return Err(trs(
            "addon adresi http(s) olmalı",
            "addon address must be http(s)",
        )
        .into());
    }
    let url = url.trim_end_matches('/');
    if url.ends_with(MANIFEST) {
        Ok(url.to_string())
    } else {
        Ok(format!("{url}/{MANIFEST}"))
    }
}

fn store(dir: &Path) -> PathBuf {
    dir.join("addons.json")
}

pub fn load(dir: &Path) -> Vec<Addon> {
    std::fs::read(store(dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(dir: &Path, addons: &[Addon]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_vec_pretty(addons).map_err(|e| e.to_string())?;
    std::fs::write(store(dir), json).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    resources: Vec<Resource>,
}

/// `resources` öğesi: düz ad ya da `{ name, types, idPrefixes }`.
#[derive(Deserialize)]
#[serde(untagged)]
enum Resource {
    Name(String),
    Full { name: String },
}

/// Manifesti doğrular (kaynak sunmalı) ve listeye ekler; aynı adres yeniden eklenmez.
pub async fn add(dir: &Path, url: &str) -> Result<Vec<Addon>, String> {
    let url = normalize_manifest(url)?;
    let m: Manifest = get_json(&url)
        .await
        .map_err(|e| tr!("manifest okunamadı: {e}", "couldn't read manifest: {e}"))?;
    let streams = m.resources.iter().any(|r| match r {
        Resource::Name(n) | Resource::Full { name: n } => n == "stream" || n == "subtitles",
    });
    if !streams {
        return Err(tr!(
            "{} kaynak (stream) sunmuyor",
            "{} doesn't offer streams",
            m.name
        ));
    }
    let mut list = load(dir);
    if !list.iter().any(|a| a.url == url) {
        let name = if m.name.trim().is_empty() {
            url.clone()
        } else {
            m.name.trim().to_string()
        };
        list.push(Addon { url, name });
        save(dir, &list)?;
    }
    Ok(list)
}

pub fn remove(dir: &Path, url: &str) -> Result<Vec<Addon>, String> {
    let mut list = load(dir);
    list.retain(|a| a.url != url);
    save(dir, &list)?;
    Ok(list)
}

#[derive(Deserialize)]
struct Streams {
    #[serde(default)]
    streams: Vec<RawStream>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStream {
    #[serde(default)]
    name: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    url: Option<String>,
    info_hash: Option<String>,
    file_idx: Option<u32>,
    #[serde(default)]
    behavior_hints: Hints,
}

#[derive(Deserialize, Default)]
struct Hints {
    filename: Option<String>,
}

/// UI'daki bir kaynak satırı.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub addon: String,
    /// Addon'un kısa etiketi (ör. kalite).
    pub name: String,
    /// Ayrıntı satırları (dosya adı, boyut, eşler…).
    pub title: String,
    /// Torrent infoHash'i (küçük harf). Yoksa kaynak odada paylaşılamaz.
    pub hash: Option<String>,
    /// Çözülmüş akış adresi (debrid addonları doğrudan video linki verir).
    pub url: Option<String>,
    pub idx: Option<u32>,
    pub file: String,
}

/// URL tabanlı (infoHash içermeyen) kaynakların oda protokolünde taşınabilmesi için
/// sentetik 40 haneli hex ön eki.
pub const SYNTHETIC_PREFIX: &str = "0000000000000000";

pub fn is_synthetic_hash(h: &str) -> bool {
    h.len() == 40 && h.starts_with(SYNTHETIC_PREFIX)
}

pub fn synthetic_hash(seed: &str) -> String {
    let mut h1: u64 = 0xcbf29ce484222325;
    let mut h2: u64 = 0x84222325cbf29ce4;
    for &b in seed.as_bytes() {
        h1 ^= b as u64;
        h1 = h1.wrapping_mul(0x100000001b3);
        h2 = h2.wrapping_mul(0x100000001b3) ^ (b as u64);
    }
    format!("{SYNTHETIC_PREFIX}{h1:016x}{:08x}", (h2 as u32))
}

fn extract_hash_from_url(url: &str) -> Option<String> {
    url.split(['/', '?', '&', '='])
        .find(|seg| seg.len() == 40 && seg.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|s| s.to_ascii_lowercase())
}

/// `"series:tt2234222:1:1|Orphan.Black.S01E01.mkv"` -> `Some(("series", "tt2234222:1:1", "Orphan.Black.S01E01.mkv"))`.
pub fn parse_content_file(file: &str) -> Option<(&str, &str, &str)> {
    let (meta, hint) = file.split_once('|')?;
    let (kind, id) = meta.split_once(':')?;
    if matches!(kind, "movie" | "series") && valid_id(id) {
        Some((kind, id, hint))
    } else {
        None
    }
}

impl RawStream {
    #[cfg(test)]
    fn into_source(self, addon: &str) -> Source {
        let hash = self
            .info_hash
            .map(|h| h.to_ascii_lowercase())
            .filter(|h| h.len() == 40 && h.bytes().all(|b| b.is_ascii_hexdigit()));
        let title = if self.title.is_empty() {
            self.description
        } else {
            self.title
        };
        Source {
            addon: addon.to_string(),
            name: self.name,
            title,
            idx: hash.as_ref().and(self.file_idx),
            hash,
            url: self.url,
            file: self.behavior_hints.filename.unwrap_or_default(),
        }
    }

    fn into_source_with_meta(self, addon: &str, kind: &str, id: &str) -> Option<Source> {
        let title = if self.title.is_empty() {
            self.description
        } else {
            self.title
        };
        let combined_lower = format!("{}\n{}", self.name, title).to_ascii_lowercase();
        if self.name.contains('⛔')
            || combined_lower.contains("non-debrid searches disabled")
            || combined_lower.contains("invalid token")
            || combined_lower.contains("invalid api key")
        {
            return None;
        }
        let url = self
            .url
            .map(|u| u.trim().to_string())
            .filter(|u| u.starts_with("http://") || u.starts_with("https://"));
        let mut hash = self
            .info_hash
            .map(|h| h.trim().to_ascii_lowercase())
            .filter(|h| h.len() == 40 && h.bytes().all(|b| b.is_ascii_hexdigit()))
            .or_else(|| url.as_deref().and_then(extract_hash_from_url));

        let raw_hint = self
            .behavior_hints
            .filename
            .filter(|f| !f.trim().is_empty())
            .unwrap_or_else(|| {
                title
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or_default()
                    .to_string()
            });
        let clean_hint: String = raw_hint
            .chars()
            .map(|c| {
                if matches!(c, '/' | '\\' | '|') {
                    ' '
                } else {
                    c
                }
            })
            .collect();
        let file: String = format!("{kind}:{id}|{}", clean_hint.trim())
            .chars()
            .take(240)
            .collect();

        if hash.is_none() {
            if let Some(ref u) = url {
                hash = Some(synthetic_hash(&format!("{file}|{u}")));
            }
        }
        if hash.is_none() && url.is_none() {
            return None;
        }
        Some(Source {
            addon: addon.to_string(),
            name: self.name,
            title,
            idx: self.file_idx,
            hash,
            url,
            file,
        })
    }
}

#[derive(Serialize)]
pub struct StreamList {
    pub sources: Vec<Source>,
    /// Yanıt vermeyen addon'lar ("ad: hata").
    pub errors: Vec<String>,
}

/// Tüm addon'lardan `kind` (`movie` | `series`) ve `id` (`tt…` ya da `tt…:sezon:bölüm`)
/// için kaynakları paralel çeker.
pub async fn streams(addons: &[Addon], kind: &str, id: &str) -> Result<StreamList, String> {
    if !matches!(kind, "movie" | "series") || !valid_id(id) {
        return Err(trs("geçersiz başlık", "invalid title").into());
    }
    let calls = addons.iter().map(|a| async move {
        let url = format!("{}/stream/{kind}/{id}.json", a.base());
        (a, get_json::<Streams>(&url).await)
    });
    let mut out = StreamList {
        sources: Vec::new(),
        errors: Vec::new(),
    };
    for (addon, res) in futures_util::future::join_all(calls).await {
        match res {
            Ok(s) => out.sources.extend(
                s.streams
                    .into_iter()
                    .filter_map(|r| r.into_source_with_meta(&addon.name, kind, id)),
            ),
            Err(e) => out.errors.push(format!("{}: {e}", addon.name)),
        }
    }
    Ok(out)
}

/// `tt1234567` ya da `tt1234567:1:2`.
fn valid_id(id: &str) -> bool {
    let mut parts = id.split(':');
    let imdb = parts.next().unwrap_or_default();
    let rest: Vec<&str> = parts.collect();
    imdb.len() > 2
        && imdb.starts_with("tt")
        && imdb[2..].bytes().all(|b| b.is_ascii_digit())
        && (rest.is_empty()
            || rest.len() == 2
                && rest
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())))
}

/// Arama sonucu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Title {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub poster: Option<String>,
    #[serde(default, rename = "releaseInfo")]
    pub year: Option<String>,
    /// Cinemeta verirse: arka plan görseli, IMDb puanı, türler (Sinema'daki başlık ekranı).
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default, rename = "imdbRating")]
    pub rating: Option<String>,
    #[serde(default)]
    pub genres: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Metas {
    #[serde(default)]
    metas: Vec<Title>,
}

/// Cinemeta'da film ve dizi arar (IMDb kimlikli sonuçlar).
pub async fn search(query: &str) -> Result<Vec<Title>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let url = |kind: &str| {
        let mut u = reqwest::Url::parse(CINEMETA).expect("sabit adres");
        u.path_segments_mut().expect("http adresi").extend([
            "catalog",
            kind,
            "top",
            &format!("search={q}.json"),
        ]);
        u.to_string()
    };
    let (movies, series) = futures_util::future::join(
        get_json::<Metas>(&url("movie")),
        get_json::<Metas>(&url("series")),
    )
    .await;
    if let (Err(e), Err(_)) = (&movies, &series) {
        return Err(tr!("arama: {e}", "search: {e}"));
    }
    let mut out: Vec<Title> = Vec::new();
    for t in [movies, series].into_iter().flatten().flat_map(|m| m.metas) {
        if valid_id(&t.id) && !out.iter().any(|o| o.id == t.id) {
            out.push(t);
        }
    }
    // Film ve dizi listeleri ayrı gelir: adı sorgunun kendisi olan, sonra adında sorgu
    // geçen başlıklar öne (sıralama kararlı, Cinemeta'nın kendi sırası korunur).
    let q = q.to_lowercase();
    out.sort_by_key(|t| {
        let name = t.name.to_lowercase();
        if name == q {
            0
        } else if name.contains(&q) {
            1
        } else {
            2
        }
    });
    Ok(out)
}

/// Cinemeta'da popüler film ve dizileri getirir.
pub async fn popular() -> Result<Vec<Title>, String> {
    let (movies, series) = futures_util::future::join(
        get_json::<Metas>(&format!("{CINEMETA}/catalog/movie/top.json")),
        get_json::<Metas>(&format!("{CINEMETA}/catalog/series/top.json")),
    )
    .await;
    if let (Err(e), Err(_)) = (&movies, &series) {
        return Err(tr!("popüler listesi: {e}", "popular list: {e}"));
    }
    let mut out: Vec<Title> = Vec::new();
    let m_list = movies.ok().map(|m| m.metas).unwrap_or_default();
    let s_list = series.ok().map(|s| s.metas).unwrap_or_default();

    let max_len = m_list.len().max(s_list.len());
    for i in 0..max_len {
        if let Some(m) = m_list.get(i) {
            if valid_id(&m.id) && !out.iter().any(|o| o.id == m.id) {
                out.push(m.clone());
            }
        }
        if let Some(s) = s_list.get(i) {
            if valid_id(&s.id) && !out.iter().any(|o| o.id == s.id) {
                out.push(s.clone());
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct Episode {
    /// Kaynak isteği kimliği: `tt…:sezon:bölüm`.
    pub id: String,
    pub season: u32,
    pub episode: u32,
    pub name: String,
    pub thumbnail: Option<String>,
}

#[derive(Deserialize)]
struct SeriesMeta {
    meta: SeriesVideos,
}

#[derive(Deserialize)]
struct SeriesVideos {
    #[serde(default)]
    videos: Vec<RawEpisode>,
}

/// Cinemeta bölümü: `episode` ve eski `number` birlikte gelir; ad `name` ya da `title`
/// (biri `null` olabilir).
#[derive(Deserialize)]
struct RawEpisode {
    #[serde(default)]
    season: u32,
    #[serde(default)]
    episode: u32,
    name: Option<String>,
    title: Option<String>,
    thumbnail: Option<String>,
}

/// Dizinin bölümleri, sezon ve bölüme göre sıralı (özel bölümler, sezon 0, sonda).
pub async fn episodes(id: &str) -> Result<Vec<Episode>, String> {
    if !valid_id(id) || id.contains(':') {
        return Err(trs("geçersiz dizi", "invalid series").into());
    }
    let meta: SeriesMeta = get_json(&format!("{CINEMETA}/meta/series/{id}.json")).await?;
    let mut out: Vec<Episode> = meta
        .meta
        .videos
        .into_iter()
        .filter(|v| v.episode > 0)
        .map(|v| Episode {
            id: format!("{id}:{}:{}", v.season, v.episode),
            season: v.season,
            episode: v.episode,
            name: v
                .name
                .filter(|n| !n.is_empty())
                .or(v.title)
                .unwrap_or_default(),
            thumbnail: v.thumbnail.filter(|t| t.starts_with("https://")),
        })
        .collect();
    out.sort_by_key(|e| (e.season == 0, e.season, e.episode));
    out.dedup_by(|a, b| a.id == b.id);
    Ok(out)
}

#[derive(Deserialize)]
struct RawSubtitles {
    #[serde(default)]
    subtitles: Vec<RawSubtitle>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RawSubtitle {
    #[serde(default)]
    id: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    lang: String,
    #[serde(default)]
    subtitle_file_name: String,
    #[serde(default)]
    movie_release_name: String,
    #[serde(default)]
    release_group: String,
    #[serde(default)]
    release_format: String,
}

/// Seçilen dizi bölümü / film için addon'lardan bulunan altyazı.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundSub {
    pub id: String,
    pub url: String,
    pub name: String,
    pub lang: String,
    pub downloads: u32,
    pub hash_match: bool,
}

fn parse_ep_token(tok: &str) -> Option<(u32, u32)> {
    let low = tok
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_ascii_lowercase();
    if let Some(rest) = low.strip_prefix('s') {
        if let Some((s, e)) = rest.split_once('e') {
            let e_digits: String = e.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let (Ok(sn), Ok(en)) = (s.parse::<u32>(), e_digits.parse::<u32>()) {
                if sn <= 99 && (1..=999).contains(&en) {
                    return Some((sn, en));
                }
            }
        }
    }
    if let Some((s, e)) = low.split_once('x') {
        if !s.is_empty() && s.len() <= 2 && s.bytes().all(|b| b.is_ascii_digit()) {
            let e_digits: String = e.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let (Ok(sn), Ok(en)) = (s.parse::<u32>(), e_digits.parse::<u32>()) {
                if sn <= 99 && (1..=999).contains(&en) {
                    return Some((sn, en));
                }
            }
        }
    }
    None
}

fn is_release_tag(tok: &str) -> bool {
    matches!(
        tok.to_ascii_lowercase().as_str(),
        "2160p"
            | "1440p"
            | "1080p"
            | "720p"
            | "480p"
            | "4k"
            | "uhd"
            | "web"
            | "web-dl"
            | "webdl"
            | "webrip"
            | "bluray"
            | "bdrip"
            | "brrip"
            | "hdrip"
            | "dvdrip"
            | "hdtv"
            | "x264"
            | "x265"
            | "h264"
            | "h265"
            | "hevc"
            | "aac"
            | "ddp5"
            | "dd5"
            | "ac3"
            | "remux"
            | "proper"
            | "repack"
            | "extended"
            | "multisub"
            | "dual"
    )
}

fn is_year(tok: &str) -> bool {
    tok.len() == 4
        && (tok.starts_with("19") || tok.starts_with("20"))
        && tok.bytes().all(|b| b.is_ascii_digit())
}

/// Dosya ya da torrent adından temiz dizi/film adını ve varsa `(sezon, bölüm)` bilgisini çıkarır.
pub fn clean_title_and_episode(raw: &str) -> (String, Option<(u32, u32)>) {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let base = base.split_once('|').map(|(_, h)| h).unwrap_or(base);
    let stripped = match base.rsplit_once('.') {
        Some((stem, ext)) if ext.len() <= 4 && ext.chars().all(|c| c.is_ascii_alphanumeric()) => {
            stem
        }
        _ => base,
    };
    let normalized: String = stripped
        .chars()
        .map(|c| {
            if matches!(c, '.' | '_' | '[' | ']' | '(' | ')') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let tokens: Vec<&str> = normalized.split_whitespace().collect();

    for (i, &t) in tokens.iter().enumerate() {
        if let Some(ep) = parse_ep_token(t) {
            let mut head = &tokens[..i];
            if let Some(&last) = head.last() {
                if is_year(last) && head.len() > 1 {
                    head = &head[..head.len() - 1];
                }
            }
            let name = head.join(" ").trim().to_string();
            if !name.is_empty() {
                return (name, Some(ep));
            }
        }
    }

    let mut out = Vec::new();
    for &t in &tokens {
        if is_release_tag(t) {
            break;
        }
        out.push(t);
        if is_year(t) && out.len() > 1 {
            break;
        }
    }
    let name = out.join(" ").trim().to_string();
    (
        if name.is_empty() {
            normalized.trim().to_string()
        } else {
            name
        },
        None,
    )
}

/// `file` (`series:tt2234222:1:1|hint`) ya da `title`'dan (`Orphan Black S01E01`)
/// Stremio `(kind, id, hint)` üçlüsünü çözer.
pub async fn resolve_content_meta(file: &str, title: &str) -> Option<(String, String, String)> {
    if let Some((kind, id, hint)) = parse_content_file(file) {
        let effective_hint = if hint.trim().is_empty() { title } else { hint };
        return Some((kind.to_string(), id.to_string(), effective_hint.to_string()));
    }

    let (mut name, mut ep) = clean_title_and_episode(title);
    if ep.is_none() && !file.is_empty() {
        let (fn_name, fn_ep) = clean_title_and_episode(file);
        if fn_ep.is_some() {
            ep = fn_ep;
            if name.is_empty() || name.len() > fn_name.len() + 10 {
                name = fn_name;
            }
        }
    }
    if name.is_empty() {
        return None;
    }

    let search_q = if ep.is_none() {
        let words: Vec<&str> = name.split_whitespace().collect();
        if words.len() > 1 && words.last().is_some_and(|w| is_year(w)) {
            words[..words.len() - 1].join(" ")
        } else {
            name.clone()
        }
    } else {
        name.clone()
    };

    let hits = search(&search_q).await.ok()?;
    let hint = if !file.is_empty() { file } else { title };
    if let Some((s, e)) = ep {
        let hit = hits
            .iter()
            .find(|t| t.kind == "series")
            .or_else(|| hits.first())?;
        let base_id = hit.id.split(':').next().unwrap_or(&hit.id);
        Some((
            "series".into(),
            format!("{base_id}:{s}:{e}"),
            hint.to_string(),
        ))
    } else {
        let hit = hits
            .iter()
            .find(|t| t.kind == "movie")
            .or_else(|| hits.first())?;
        let base_id = hit.id.split(':').next().unwrap_or(&hit.id);
        Some(("movie".into(), base_id.to_string(), hint.to_string()))
    }
}

fn lang_matches(sub_lang: &str, target: &str) -> bool {
    let s = sub_lang.trim().to_ascii_lowercase();
    match target.trim().to_ascii_lowercase().as_str() {
        "tr" | "tur" => s == "tur" || s == "tr" || s.starts_with("tur"),
        "en" | "eng" => s == "eng" || s == "en" || s.starts_with("eng"),
        other => s == other,
    }
}

fn score_subtitle(sub: &RawSubtitle, hint: &str) -> (i32, String) {
    let hint_low = hint.to_ascii_lowercase();
    let rel_clean = sub
        .movie_release_name
        .split('|')
        .next()
        .unwrap_or(&sub.movie_release_name)
        .trim();
    let file_clean = sub.subtitle_file_name.trim();
    let sub_low = format!(
        "{} {} {} {}",
        rel_clean, file_clean, sub.release_group, sub.release_format
    )
    .to_ascii_lowercase();

    let mut score: i32 = 0;

    if file_clean.to_ascii_lowercase().ends_with(".srt") {
        score += 20;
    } else if file_clean.to_ascii_lowercase().ends_with(".sub")
        || file_clean.to_ascii_lowercase().ends_with(".txt")
    {
        score -= 15;
    }

    let grp = sub.release_group.trim().to_ascii_lowercase();
    if grp.len() >= 2 && hint_low.contains(&grp) {
        score += 55;
    } else if let Some((_, tail)) = hint_low.rsplit_once('-') {
        let tail_grp = tail
            .split(['.', ' ', ']', ')'])
            .next()
            .unwrap_or_default()
            .trim();
        if tail_grp.len() >= 2 && sub_low.contains(tail_grp) {
            score += 55;
        }
    }

    for fmt_tags in [
        &["web-dl", "webdl"][..],
        &["webrip"][..],
        &["bluray", "bdrip", "brrip"][..],
        &["hdtv"][..],
        &["dvdrip", "dvd"][..],
    ] {
        let in_hint = fmt_tags.iter().any(|t| hint_low.contains(t));
        let in_sub = fmt_tags.iter().any(|t| sub_low.contains(t));
        if in_hint && in_sub {
            score += 30;
            break;
        }
    }

    for res in ["2160p", "1080p", "720p", "480p"] {
        if hint_low.contains(res) && sub_low.contains(res) {
            score += 15;
            break;
        }
    }

    for tok in hint_low.split(|c: char| !c.is_ascii_alphanumeric()) {
        if tok.len() >= 3
            && !matches!(tok, "mkv" | "mp4" | "avi" | "the" | "and")
            && sub_low.contains(tok)
        {
            score += 3;
        }
    }

    let display = if !rel_clean.is_empty() && !rel_clean.eq_ignore_ascii_case("1.srt") {
        rel_clean.to_string()
    } else if !file_clean.is_empty() && !file_clean.eq_ignore_ascii_case("1.srt") {
        file_clean.to_string()
    } else if !sub.release_format_or_group().is_empty() {
        sub.release_format_or_group()
    } else {
        format!("Altyazı #{}", sub.id)
    };

    (score, display)
}

impl RawSubtitle {
    fn release_format_or_group(&self) -> String {
        match (self.release_format.trim(), self.release_group.trim()) {
            ("", "") => String::new(),
            (f, "") => f.to_string(),
            ("", g) => g.to_string(),
            (f, g) => format!("{f} · {g}"),
        }
    }
}

async fn fetch_raw_subtitles(addons: &[Addon], kind: &str, id: &str) -> Vec<RawSubtitle> {
    let mut bases = vec![OPENSUBTITLES_V3.to_string()];
    for a in addons {
        let b = a.base().to_string();
        if !bases.contains(&b) {
            bases.push(b);
        }
    }
    let calls = bases.into_iter().map(|base| async move {
        let url = format!("{base}/subtitles/{kind}/{id}.json");
        let res = http()
            .get(&url)
            .timeout(Duration::from_secs(6))
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .ok()?;
        res.json::<RawSubtitles>().await.ok().map(|s| s.subtitles)
    });
    let mut out = Vec::new();
    for list in futures_util::future::join_all(calls)
        .await
        .into_iter()
        .flatten()
    {
        for s in list {
            if (s.url.starts_with("https://") || s.url.starts_with("http://"))
                && !out.iter().any(|o: &RawSubtitle| o.url == s.url)
            {
                out.push(s);
            }
        }
    }
    out
}

/// Seçilen dizi bölümü (`series:tt…:s:e`) ya da film (`movie:tt…`) için tüm altyazıları
/// getirir ve oynatılan sürüme (`hint`) uyumuna göre sıralar.
pub async fn search_subtitles(
    addons: &[Addon],
    file: &str,
    title: &str,
    lang: &str,
) -> Vec<FoundSub> {
    let Some((kind, id, hint)) = resolve_content_meta(file, title).await else {
        return Vec::new();
    };
    let raw = fetch_raw_subtitles(addons, &kind, &id).await;
    let norm_lang = if lang.to_ascii_lowercase().starts_with("tr") {
        "tr"
    } else if lang.to_ascii_lowercase().starts_with("en") {
        "en"
    } else {
        lang
    };

    let mut scored: Vec<(i32, FoundSub)> = raw
        .into_iter()
        .filter(|s| lang_matches(&s.lang, norm_lang))
        .map(|s| {
            let (score, name) = score_subtitle(&s, &hint);
            (
                score,
                FoundSub {
                    id: s.id,
                    url: s.url,
                    name,
                    lang: norm_lang.to_string(),
                    downloads: 0,
                    hash_match: score >= 50,
                },
            )
        })
        .collect();

    scored.sort_by_key(|(sc, _)| std::cmp::Reverse(*sc));
    scored.into_iter().take(15).map(|(_, f)| f).collect()
}

/// Video açılırken `Stream.subs`'a eklenecek en uygun Türkçe ve İngilizce altyazılar.
pub async fn subtitles_for_stream(
    addons: &[Addon],
    file: &str,
    title: &str,
) -> Vec<koni_sync::protocol::SubFile> {
    let Some((kind, id, hint)) = resolve_content_meta(file, title).await else {
        return Vec::new();
    };
    let raw = fetch_raw_subtitles(addons, &kind, &id).await;
    if raw.is_empty() {
        return Vec::new();
    }

    let mut tr_subs: Vec<(i32, RawSubtitle, String)> = Vec::new();
    let mut en_subs: Vec<(i32, RawSubtitle, String)> = Vec::new();
    for s in raw {
        if lang_matches(&s.lang, "tr") {
            let (sc, disp) = score_subtitle(&s, &hint);
            tr_subs.push((sc, s, disp));
        } else if lang_matches(&s.lang, "en") {
            let (sc, disp) = score_subtitle(&s, &hint);
            en_subs.push((sc, s, disp));
        }
    }
    tr_subs.sort_by_key(|(sc, _, _)| std::cmp::Reverse(*sc));
    en_subs.sort_by_key(|(sc, _, _)| std::cmp::Reverse(*sc));

    let mut out = Vec::new();
    for (i, (sc, s, disp)) in tr_subs.into_iter().take(4).enumerate() {
        let tag = s.release_format_or_group();
        let label = if !tag.is_empty() {
            format!(
                "Türkçe #{} · {}{}",
                i + 1,
                tag,
                if sc >= 50 { " ★" } else { "" }
            )
        } else {
            let short: String = disp.chars().take(42).collect();
            format!("Türkçe #{} · {}", i + 1, short)
        };
        out.push(koni_sync::protocol::SubFile {
            url: s.url,
            lang: "tr".into(),
            title: label,
        });
    }
    for (i, (_, s, _)) in en_subs.into_iter().take(2).enumerate() {
        let tag = s.release_format_or_group();
        let label = if !tag.is_empty() {
            format!("İngilizce #{} · {}", i + 1, tag)
        } else {
            format!("İngilizce #{}", i + 1)
        };
        out.push(koni_sync::protocol::SubFile {
            url: s.url,
            lang: "en".into(),
            title: label,
        });
    }
    out
}

/// Altyazı URL'sini indirir ve UTF-8 metne çevirir (eski Türkçe CP1254 kodlamasını da korur).
pub async fn download_subtitle_text(url: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|_| trs("geçersiz altyazı adresi", "invalid subtitle address").to_string())?;
    if !matches!(parsed.scheme(), "https" | "http") {
        return Err(trs(
            "altyazı adresi http(s) olmalı",
            "subtitle address must be http(s)",
        )
        .into());
    }
    let res = http()
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| {
            tr!(
                "altyazı indirilemedi: {}",
                "couldn't download subtitles: {}",
                e.without_url()
            )
        })?;
    let bytes = res
        .bytes()
        .await
        .map_err(|e| tr!("altyazı okunamadı: {e}", "couldn't read subtitles: {e}"))?;
    if bytes.len() > (1 << 20) {
        return Err(trs("altyazı dosyası çok büyük", "subtitle file is too large").into());
    }
    Ok(decode_sub_bytes(&bytes))
}

fn decode_sub_bytes(b: &[u8]) -> String {
    let b = b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(b);
    if let Ok(s) = std::str::from_utf8(b) {
        return s.to_string();
    }
    b.iter()
        .map(|&c| match c {
            0xD0 => 'Ğ',
            0xDD => 'İ',
            0xDE => 'Ş',
            0xF0 => 'ğ',
            0xFD => 'ı',
            0xFE => 'ş',
            0x80 => '€',
            0x85 => '…',
            0x91 => '‘',
            0x92 => '’',
            0x93 => '“',
            0x94 => '”',
            0x96 => '–',
            0x97 => '—',
            _ => c as char,
        })
        .collect()
}

pub const _SUB_KEEP: () = ();
#[cfg(any())]
fn _unused_ok(out: Vec<Episode>) -> Result<Vec<Episode>, String> {
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_urls_normalize() {
        assert_eq!(
            normalize_manifest("stremio://addon.example/cfg/manifest.json").unwrap(),
            "https://addon.example/cfg/manifest.json"
        );
        assert_eq!(
            normalize_manifest(" https://addon.example/ ").unwrap(),
            "https://addon.example/manifest.json"
        );
        assert!(normalize_manifest("file:///etc/manifest.json").is_err());
        let a = Addon {
            url: "https://addon.example/cfg/manifest.json".into(),
            name: "A".into(),
        };
        assert_eq!(a.base(), "https://addon.example/cfg");
    }

    #[test]
    fn only_torrent_streams_are_shareable() {
        let raw: Streams = serde_json::from_str(
            r#"{"streams":[
                {"name":"A 1080p","title":"Film.mkv\n2 GB","infoHash":"ABCDEF0123456789ABCDEF0123456789ABCDEF01","fileIdx":2,
                 "behaviorHints":{"filename":"Film.2020.mkv"}},
                {"name":"B","description":"own account","url":"https://debrid.example/KEY/abc"},
                {"name":"C","infoHash":"nothex","fileIdx":1}
            ]}"#,
        )
        .unwrap();
        let s: Vec<Source> = raw
            .streams
            .into_iter()
            .map(|r| r.into_source("X"))
            .collect();
        assert_eq!(
            s[0].hash.as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef01")
        );
        assert_eq!((s[0].idx, s[0].file.as_str()), (Some(2), "Film.2020.mkv"));
        assert_eq!(
            (
                s[1].hash.as_deref(),
                s[1].title.as_str(),
                s[1].url.as_deref()
            ),
            (None, "own account", Some("https://debrid.example/KEY/abc"))
        );
        assert_eq!((s[2].hash.as_deref(), s[2].idx), (None, None));
    }

    #[test]
    fn content_ids() {
        assert!(valid_id("tt0111161"));
        assert!(valid_id("tt0944947:1:2"));
        assert!(!valid_id("tt0944947:1"));
        assert!(!valid_id("tt/../x"));
        assert!(!valid_id("kitsu:1"));
        assert_eq!(
            clean_title_and_episode(
                "Orphan.Black.S01E01.Natural.Selection.1080p.WEB-DL.AAC.2.0.H.264-ECI.mkv"
            ),
            ("Orphan Black".into(), Some((1, 1)))
        );
        assert_eq!(
            clean_title_and_episode("The.Boys.2019.S02E03.1080p.WEB-DL.mkv"),
            ("The Boys".into(), Some((2, 3)))
        );
        assert_eq!(
            clean_title_and_episode("Orphan Black S01E01"),
            ("Orphan Black".into(), Some((1, 1)))
        );
        assert_eq!(
            clean_title_and_episode("Fight.Club.1999.1080p.BluRay.x264.mkv"),
            ("Fight Club 1999".into(), None)
        );
    }
}
