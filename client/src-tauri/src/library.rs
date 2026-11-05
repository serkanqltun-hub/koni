//! Yerel kütüphane: Ayarlar'da eklenen klasörlerdeki videolar. Dosya adından ad, yıl ve
//! SxxEyy çözülür, Cinemeta'da eşlenir; sonuç `library.json`'da tutulur (eşleşmeler
//! önbellekte, yeniden taramada tekrar sorulmaz). Tarama komutlar gibi async çalışır,
//! klasör gezme `spawn_blocking`'de: ana iş parçacığını (macOS'ta çizim) tutmaz.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::i18n::trs;

const VIDEO_EXT: &[&str] = &[
    "mkv", "mp4", "m4v", "avi", "mov", "webm", "wmv", "ts", "m2ts", "mpg", "mpeg", "flv",
];

/// Cinemeta eşleşmesi.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Match {
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub poster: Option<String>,
    #[serde(default)]
    pub year: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub path: String,
    /// Dosya adından çözülen ad (eşleşme yoksa gösterilir).
    pub name: String,
    #[serde(default)]
    pub year: Option<u32>,
    #[serde(default)]
    pub season: Option<u32>,
    #[serde(default)]
    pub episode: Option<u32>,
    pub size: u64,
    /// Değiştirilme zamanı (Unix sn): "yeni eklenen" sırası.
    pub added: u64,
    #[serde(default)]
    pub m: Option<Match>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Library {
    pub folders: Vec<String>,
    pub items: Vec<Item>,
    /// `tür|ad|yıl` → eşleşme (`None`: bulunamadı, bir daha sorulmaz).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub cache: HashMap<String, Option<Match>>,
}

/// Arayüze giden görünüm (önbellek hariç).
#[derive(Serialize)]
pub struct View<'a> {
    pub folders: &'a [String],
    pub items: &'a [Item],
}

/// Aynı anda tek tarama; dosyaya yazanlar da bunu tutar.
static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("library.json"))
}

fn load(path: &Path) -> Library {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(path: &Path, lib: &Library) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(lib).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn view(lib: &Library) -> serde_json::Value {
    serde_json::to_value(View {
        folders: &lib.folders,
        items: &lib.items,
    })
    .unwrap_or_default()
}

pub async fn get(app: &AppHandle) -> Result<serde_json::Value, String> {
    let path = file(app)?;
    let _g = LOCK.lock().await;
    Ok(view(&load(&path)))
}

/// Klasörü ekler (varsa yalnızca tarar) ve hepsini yeniden tarar.
pub async fn add(app: &AppHandle, folder: PathBuf) -> Result<serde_json::Value, String> {
    if !folder.is_dir() {
        return Err(trs("klasör bulunamadı", "folder not found").into());
    }
    let path = file(app)?;
    let _g = LOCK.lock().await;
    let mut lib = load(&path);
    let f = folder.to_string_lossy().into_owned();
    if !lib.folders.contains(&f) {
        lib.folders.push(f);
    }
    rescan(&mut lib, false).await;
    save(&path, &lib)?;
    Ok(view(&lib))
}

pub async fn remove(app: &AppHandle, folder: &str) -> Result<serde_json::Value, String> {
    let path = file(app)?;
    let _g = LOCK.lock().await;
    let mut lib = load(&path);
    lib.folders.retain(|f| f != folder);
    let root = Path::new(folder);
    lib.items.retain(|i| !Path::new(&i.path).starts_with(root));
    save(&path, &lib)?;
    Ok(view(&lib))
}

/// Elle "Yeniden tara": bulunamayanlar yeniden sorulur.
pub async fn scan(app: &AppHandle) -> Result<serde_json::Value, String> {
    let path = file(app)?;
    let _g = LOCK.lock().await;
    let mut lib = load(&path);
    rescan(&mut lib, true).await;
    save(&path, &lib)?;
    Ok(view(&lib))
}

async fn rescan(lib: &mut Library, retry: bool) {
    let folders = lib.folders.clone();
    let mut items = tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        for f in &folders {
            walk(Path::new(f), &mut out);
        }
        out
    })
    .await
    .unwrap_or_default();
    if retry {
        lib.cache.retain(|_, m| m.is_some());
    }
    // Eşlenmemiş başlıkları sor (aynı anda birkaç istek).
    let mut todo: Vec<(String, String, Option<u32>, bool)> = Vec::new();
    for i in &items {
        let k = key(i);
        if !lib.cache.contains_key(&k) && !todo.iter().any(|t| t.0 == k) {
            todo.push((k, i.name.clone(), i.year, i.season.is_some()));
        }
    }
    let found: Vec<(String, Option<Match>)> =
        futures_util::stream::iter(todo.into_iter().map(|(k, name, year, series)| async move {
            match lookup(&name, year, series).await {
                Ok(m) => Some((k, m)),
                Err(_) => None, // ağ yok: önbelleğe yazma, sonra yeniden denenir
            }
        }))
        .buffer_unordered(6)
        .filter_map(|x| async move { x })
        .collect()
        .await;
    lib.cache.extend(found);
    for i in &mut items {
        i.m = lib.cache.get(&key(i)).cloned().flatten();
    }
    items.sort_by_key(|i| std::cmp::Reverse(i.added));
    lib.items = items;
}

fn key(i: &Item) -> String {
    let kind = if i.season.is_some() {
        "series"
    } else {
        "movie"
    };
    format!("{kind}|{}|{}", i.name.to_lowercase(), i.year.unwrap_or(0))
}

/// Cinemeta'da ilk uygun sonuç: tür aynı, yıl verildiyse başlangıç yılı ±1.
async fn lookup(name: &str, year: Option<u32>, series: bool) -> Result<Option<Match>, String> {
    if name.is_empty() {
        return Ok(None);
    }
    let kind = if series { "series" } else { "movie" };
    let hits = crate::addons::search(name).await?;
    let start = |s: &Option<String>| -> Option<u32> { s.as_deref()?.get(..4)?.parse().ok() };
    let pick = hits.iter().filter(|t| t.kind == kind).find(|t| match year {
        Some(y) => start(&t.year).is_some_and(|s| s.abs_diff(y) <= 1),
        None => true,
    });
    Ok(pick.map(|t| Match {
        id: t.id.clone(),
        kind: t.kind.clone(),
        name: t.name.clone(),
        poster: t.poster.clone().filter(|p| p.starts_with("https://")),
        year: t.year.clone(),
    }))
}

fn walk(root: &Path, out: &mut Vec<Item>) {
    let it = walkdir::WalkDir::new(root)
        .max_depth(8)
        .into_iter()
        .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'));
    for e in it.flatten() {
        if !e.file_type().is_file() {
            continue;
        }
        let p = e.path();
        let ext = p
            .extension()
            .map(|x| x.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !VIDEO_EXT.contains(&ext.as_str()) {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        let stem = p.file_stem().unwrap_or_default().to_string_lossy();
        // Örnek parçalar ("sample") kütüphaneye girmez.
        if md.len() < 150_000_000 && stem.to_lowercase().contains("sample") {
            continue;
        }
        let parents: Vec<String> = p
            .ancestors()
            .skip(1)
            .take(2)
            .filter(|a| a.starts_with(root) && *a != root)
            .filter_map(|a| a.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        let parsed = parse_with_parents(&stem, &parents);
        out.push(Item {
            path: p.to_string_lossy().into_owned(),
            name: parsed.name,
            year: parsed.year,
            season: parsed.season,
            episode: parsed.episode,
            size: md.len(),
            added: md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
            m: None,
        });
    }
}

// ---- Dosya adı çözme ------------------------------------------------------------------

#[derive(Debug, Default, PartialEq)]
pub struct Parsed {
    pub name: String,
    pub year: Option<u32>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

/// Adın bittiği yerdeki kalite/kaynak etiketleri.
const STOP: &[&str] = &[
    "2160p",
    "1080p",
    "1080i",
    "720p",
    "576p",
    "480p",
    "4k",
    "uhd",
    "hdr",
    "hdr10",
    "dv",
    "x264",
    "x265",
    "h264",
    "h265",
    "hevc",
    "avc",
    "xvid",
    "divx",
    "bluray",
    "bdrip",
    "brrip",
    "webrip",
    "web",
    "webdl",
    "hdtv",
    "hdrip",
    "dvdrip",
    "dvd",
    "remux",
    "proper",
    "repack",
    "extended",
    "unrated",
    "remastered",
    "imax",
    "10bit",
    "aac",
    "ac3",
    "dts",
    "ddp5",
    "multi",
    "dual",
    "complete",
];

fn year_of(t: &str) -> Option<u32> {
    let y: u32 = t.parse().ok()?;
    (t.len() == 4 && (1900..=2099).contains(&y)).then_some(y)
}

/// `S01E02`, `s1e2`, `1x02` → (sezon, bölüm).
fn ep_of(t: &str) -> Option<(u32, u32)> {
    let l = t.to_lowercase();
    if let Some((s, rest)) = l.strip_prefix('s').and_then(|r| r.split_once('e')) {
        let e: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if (1..=2).contains(&s.len()) && (1..=3).contains(&e.len()) {
            if let (Ok(s), Ok(e)) = (s.parse(), e.parse()) {
                return Some((s, e));
            }
        }
    }
    let (s, e) = l.split_once('x')?;
    if !s.is_empty()
        && s.len() <= 2
        && e.len() == 2
        && (s.chars().chain(e.chars())).all(|c| c.is_ascii_digit())
    {
        return Some((s.parse().ok()?, e.parse().ok()?));
    }
    None
}

/// Dosya adını çözer (uzantısız).
pub fn parse(stem: &str) -> Parsed {
    let toks: Vec<&str> = stem
        .split(['.', '_', ' ', '(', ')', '[', ']', '{', '}', '-', '+', ','])
        .filter(|t| !t.is_empty())
        .collect();
    let mut end = toks.len();
    let mut out = Parsed::default();
    for (i, t) in toks.iter().enumerate() {
        if let Some((s, e)) = ep_of(t) {
            out.season = Some(s);
            out.episode = Some(e);
            end = end.min(i);
            break;
        }
        // "S01 E02" ayrı yazılmış.
        if let (Some(s), Some(n)) = (t.strip_prefix(['s', 'S']), toks.get(i + 1)) {
            if let (Ok(s), Some(e)) = (s.parse::<u32>(), n.strip_prefix(['e', 'E'])) {
                if let Ok(e) = e.parse::<u32>() {
                    out.season = Some(s);
                    out.episode = Some(e);
                    end = end.min(i);
                    break;
                }
            }
        }
        if i > 0 {
            if let Some(y) = year_of(t) {
                out.year = Some(y);
                end = i;
                continue;
            }
        }
        if STOP.contains(&t.to_lowercase().as_str()) {
            end = end.min(i);
            break;
        }
    }
    out.name = toks[..end.min(toks.len())].join(" ");
    out
}

/// Bölüm dosyasının adında dizi adı yoksa ("S01E02.mkv", "Season 1/…") üst klasörden alınır.
fn parse_with_parents(stem: &str, parents: &[String]) -> Parsed {
    let mut p = parse(stem);
    if p.name.is_empty() || p.season.is_some() && p.name.chars().all(|c| c.is_ascii_digit()) {
        for dir in parents {
            let d = parse(dir);
            let l = d.name.to_lowercase();
            if d.name.is_empty()
                || l.starts_with("season")
                || l.starts_with("sezon")
                || l == "specials"
            {
                continue;
            }
            p.name = d.name;
            p.year = p.year.or(d.year);
            break;
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, year: Option<u32>, se: Option<(u32, u32)>) -> Parsed {
        Parsed {
            name: name.into(),
            year,
            season: se.map(|x| x.0),
            episode: se.map(|x| x.1),
        }
    }

    #[test]
    fn movies() {
        assert_eq!(
            parse("The.Matrix.1999.1080p.BluRay.x264"),
            p("The Matrix", Some(1999), None)
        );
        assert_eq!(
            parse("Blade Runner 2049 (2017) 2160p"),
            p("Blade Runner 2049", Some(2017), None)
        );
        assert_eq!(parse("1917.2019.720p"), p("1917", Some(2019), None));
        assert_eq!(parse("Amélie"), p("Amélie", None, None));
        assert_eq!(parse("Inception_720p_WEB"), p("Inception", None, None));
    }

    #[test]
    fn episodes() {
        assert_eq!(
            parse("Breaking.Bad.S01E03.720p"),
            p("Breaking Bad", None, Some((1, 3)))
        );
        assert_eq!(
            parse("the office 2x05"),
            p("the office", None, Some((2, 5)))
        );
        assert_eq!(
            parse("Dark (2017) - S02E01"),
            p("Dark", Some(2017), Some((2, 1)))
        );
        assert_eq!(parse("Show S1 E12"), p("Show", None, Some((1, 12))));
        let parents = ["Season 2".to_string(), "Severance (2022)".to_string()];
        assert_eq!(
            parse_with_parents("S02E04", &parents),
            p("Severance", Some(2022), Some((2, 4)))
        );
    }
}
