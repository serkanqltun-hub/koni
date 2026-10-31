//! "Herkes İndirsin" (docs/design/media.md §6.1): oda medyası `download` olunca her
//! istemci sayfayı kendi yt-dlp'siyle önbelleğe indirir, ilerlemeyi raporlar ve
//! bitince yerel dosyaları yükler.
//!
//! ffmpeg paketlenmediği için video ve ses ayrı dosyalar olarak indirilir
//! (`-f "<video>,<ses>"`) ve mpv'ye `audio-file` ile birlikte verilir; birleştirme yok.

use crate::i18n::trs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use koni_sync::protocol::DlState;

/// Önbellek kotası; aşılırsa en eski indirmeler silinir.
const CACHE_QUOTA: u64 = 50 * 1024 * 1024 * 1024;
/// Klasördeki son kullanım damgası (LRU).
const USED_STAMP: &str = "used";
/// İlerleme raporu sıklığı (durum değişimi hemen gider).
const REPORT_EVERY: Duration = Duration::from_secs(1);

/// Yerel ilerleme (sunucuya `dl_progress`).
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub state: DlState,
    pub pct: f64,
    pub speed: f64,
    pub eta: f64,
    pub error: String,
}

impl Progress {
    fn new(state: DlState, pct: f64) -> Self {
        Self {
            state,
            pct,
            speed: 0.0,
            eta: 0.0,
            error: String::new(),
        }
    }
}

/// İndirilmiş, oynatılmaya hazır dosyalar.
#[derive(Debug)]
pub struct Files {
    pub video: PathBuf,
    pub audio: Option<PathBuf>,
    pub title: String,
}

/// Süren indirme. Düşürülünce (oda başka medyaya geçti, oturum kapandı) yt-dlp durdurulur;
/// `.part` dosyaları kalır, aynı sayfa yeniden istenirse kaldığı yerden sürer.
pub struct Job {
    cancel: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(c) = self.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }
}

/// İndirmeyi arka planda başlatır. `report` en çok saniyede bir (durum değişiminde hemen)
/// çağrılır. `done` bir kez (iptal edilmediyse) dosyaları yükler; `ready` ancak yükleme
/// başarılıysa raporlanır, böylece oda geri sayımı yüklenmemiş oynatıcıyla başlamaz.
pub fn start(
    ytdlp: String,
    cache: PathBuf,
    url: String,
    report: impl Fn(Progress) + Send + 'static,
    done: impl FnOnce(Result<Files, String>) -> Result<(), String> + Send + 'static,
) -> Job {
    let cancel = Arc::new(AtomicBool::new(false));
    let child = Arc::new(Mutex::new(None));
    let job = Job {
        cancel: cancel.clone(),
        child: child.clone(),
    };
    std::thread::Builder::new()
        .name("koni-download".into())
        .spawn(move || {
            report(Progress::new(DlState::Queued, 0.0));
            let res = run(&ytdlp, &cache, &url, &cancel, &child, &report);
            if cancel.load(Ordering::SeqCst) {
                return;
            }
            let keep = res
                .as_ref()
                .ok()
                .and_then(|f| f.video.parent().map(Path::to_path_buf));
            match done(res) {
                Ok(()) => report(Progress::new(DlState::Ready, 100.0)),
                Err(e) => report(Progress {
                    error: e,
                    ..Progress::new(DlState::Failed, 0.0)
                }),
            }
            prune(&cache, keep.as_deref());
        })
        .expect("indirme thread'i başlatılamadı");
    job
}

/// İndirilecek biçimler (`--print` ile tek satır).
#[derive(Debug, PartialEq)]
struct Plan {
    key: String,
    title: String,
    /// (format_id, tahmini bayt; bilinmiyorsa 0). İlk eleman video.
    formats: Vec<(String, u64)>,
}

const PLAN_TEMPLATE: &str = "KONIPLAN|%(extractor_key)s|%(id)s|%(is_live)s|%(format_id)s|\
%(requested_formats.0.filesize,requested_formats.0.filesize_approx)s|\
%(requested_formats.1.filesize,requested_formats.1.filesize_approx)s|\
%(filesize,filesize_approx)s|%(title)s";

fn parse_plan(line: &str) -> Result<Plan, String> {
    let rest = line.strip_prefix("KONIPLAN|").ok_or(trs(
        "yt-dlp beklenmeyen çıktı verdi",
        "yt-dlp gave unexpected output",
    ))?;
    let f: Vec<&str> = rest.splitn(8, '|').collect();
    if f.len() != 8 {
        return Err(trs(
            "yt-dlp beklenmeyen çıktı verdi",
            "yt-dlp gave unexpected output",
        )
        .into());
    }
    if f[2] == "True" {
        return Err(trs(
            "canlı yayın indirilemez",
            "live streams can't be downloaded",
        )
        .into());
    }
    let size = |s: &str| s.parse::<f64>().map_or(0, |v| v.max(0.0) as u64);
    let ids: Vec<&str> = f[3].split('+').collect();
    let formats = match ids.as_slice() {
        [v, a] => vec![(v.to_string(), size(f[4])), (a.to_string(), size(f[5]))],
        [one] => vec![(one.to_string(), size(f[6]))],
        _ => return Err(tr!("beklenmeyen biçim: {}", "unexpected format: {}", f[3])),
    };
    let key = format!("{}-{}", safe(f[0]), safe(f[1]));
    Ok(Plan {
        key,
        title: f[7].trim().to_owned(),
        formats,
    })
}

/// Dosya adında güvenli parça.
fn safe(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if t.is_empty() {
        "x".into()
    } else {
        t
    }
}

fn command(bin: &str) -> Command {
    let mut cmd = Command::new(bin);
    cmd.args(["--no-config", "--no-playlist"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn spawn(
    cmd: &mut Command,
    slot: &Mutex<Option<Child>>,
    cancel: &AtomicBool,
) -> Result<std::process::ChildStdout, String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| tr!("yt-dlp çalıştırılamadı: {e}", "couldn't run yt-dlp: {e}"))?;
    let out = child.stdout.take().expect("piped");
    let mut guard = slot.lock().unwrap();
    if cancel.load(Ordering::SeqCst) {
        let _ = child.kill();
    }
    *guard = Some(child);
    Ok(out)
}

/// Biten sürecin çıkış kodu ve stderr'deki son hata satırı.
fn finish(slot: &Mutex<Option<Child>>) -> Result<(), String> {
    let Some(mut child) = slot.lock().unwrap().take() else {
        return Err(trs("iptal edildi", "cancelled").into());
    };
    let mut err = String::new();
    if let Some(e) = child.stderr.take() {
        for line in BufReader::new(e).lines().map_while(Result::ok) {
            if line.starts_with("ERROR") {
                err = line;
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else if err.is_empty() {
        Err(tr!(
            "yt-dlp başarısız ({status})",
            "yt-dlp failed ({status})"
        ))
    } else {
        Err(err)
    }
}

/// `<dir>/.lock` üzerinde özel kilit; iptal edilene kadar bekler.
fn lock_dir(dir: &Path, cancel: &AtomicBool) -> Result<std::fs::File, String> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(".lock"))
        .map_err(|e| tr!("önbellek kilidi: {e}", "cache lock: {e}"))?;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) => {
                if cancel.load(Ordering::SeqCst) {
                    return Err(trs("iptal edildi", "cancelled").into());
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(std::fs::TryLockError::Error(e)) => {
                return Err(tr!("önbellek kilidi: {e}", "cache lock: {e}"))
            }
        }
    }
}

fn run(
    bin: &str,
    cache: &Path,
    url: &str,
    cancel: &AtomicBool,
    child: &Mutex<Option<Child>>,
    report: &dyn Fn(Progress),
) -> Result<Files, String> {
    // 1. Hangi biçimler, ne büyüklükte?
    let out = spawn(
        command(bin).args([
            "-f",
            koni_resolver::DEFAULT_FORMAT,
            "--print",
            PLAN_TEMPLATE,
            url,
        ]),
        child,
        cancel,
    )?;
    let lines: Vec<String> = BufReader::new(out).lines().map_while(Result::ok).collect();
    finish(child)?;
    let plan = parse_plan(
        lines
            .iter()
            .find(|l| l.starts_with("KONIPLAN|"))
            .ok_or(trs(
                "yt-dlp biçim bilgisi vermedi",
                "yt-dlp gave no format info",
            ))?,
    )?;
    let dir = cache.join(&plan.key);
    std::fs::create_dir_all(&dir).map_err(|e| tr!("önbellek klasörü: {e}", "cache folder: {e}"))?;
    // LRU için son kullanım damgası (klasörün kendi mtime'ı Windows'ta ayarlanamaz)
    let _ = std::fs::write(dir.join(USED_STAMP), b"");
    // Aynı klasöre aynı anda iki süreç (aynı makinede iki KONİ) yazmasın; kilit süreç
    // ölünce işletim sistemince bırakılır. Bekleyen ikinci süreç, biten dosyaları bulur.
    let _lock = lock_dir(&dir, cancel)?;

    // 2. Ayrı ayrı indir (ffmpeg yok, birleştirme yok).
    let ids: Vec<&str> = plan.formats.iter().map(|(id, _)| id.as_str()).collect();
    let template = dir.join("media.f%(format_id)s.%(ext)s");
    let out = spawn(
        command(bin).args([
            "--progress",
            "--newline",
            "-f",
            &ids.join(","),
            "-o",
            &template.to_string_lossy(),
            "--progress-template",
            "download:KONI %(info.format_id)s %(progress.status)s %(progress.downloaded_bytes)s \
             %(progress.total_bytes,progress.total_bytes_estimate)s %(progress.speed)s",
            "--print",
            "after_move:KONIFILE %(format_id)s %(filepath)s",
            url,
        ]),
        child,
        cancel,
    )?;
    let mut meter = Meter::new(&plan.formats);
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    let mut last = Instant::now() - REPORT_EVERY;
    for line in BufReader::new(out).lines().map_while(Result::ok) {
        if let Some(rest) = line.strip_prefix("KONIFILE ") {
            if let Some((id, path)) = rest.split_once(' ') {
                files.push((id.to_owned(), PathBuf::from(path.trim())));
            }
        } else if let Some(rest) = line.strip_prefix("KONI ") {
            meter.update(rest);
            if last.elapsed() >= REPORT_EVERY {
                last = Instant::now();
                report(meter.progress());
            }
        }
    }
    finish(child)?;
    let path_of = |id: &str| files.iter().find(|(f, _)| f == id).map(|(_, p)| p.clone());
    let video = path_of(ids[0]).ok_or(trs(
        "yt-dlp dosya yolunu bildirmedi",
        "yt-dlp didn't report the file path",
    ))?;
    let audio = match ids.get(1) {
        Some(a) => Some(path_of(a).ok_or(trs(
            "yt-dlp ses dosyasının yolunu bildirmedi",
            "yt-dlp didn't report the audio file path",
        ))?),
        None => None,
    };
    Ok(Files {
        video,
        audio,
        title: plan.title,
    })
}

/// Biçim başına bayt sayacı → toplam yüzde.
struct Meter {
    /// (format_id, indirilen, toplam)
    parts: Vec<(String, u64, u64)>,
    speed: f64,
}

impl Meter {
    fn new(formats: &[(String, u64)]) -> Self {
        Self {
            parts: formats
                .iter()
                .map(|(id, size)| (id.clone(), 0, *size))
                .collect(),
            speed: 0.0,
        }
    }

    /// `<format_id> <status> <downloaded> <total> <speed>` (bilinmeyenler `NA`).
    fn update(&mut self, line: &str) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let [id, status, done, total, speed] = f[..] else {
            return;
        };
        let num = |s: &str| s.parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0);
        let Some(p) = self.parts.iter_mut().find(|(f, _, _)| f == id) else {
            return;
        };
        if let Some(t) = num(total) {
            p.2 = t as u64;
        }
        if let Some(d) = num(done) {
            p.1 = d as u64;
        }
        if status == "finished" {
            p.1 = p.2.max(p.1);
        }
        self.speed = num(speed).unwrap_or(0.0);
    }

    fn progress(&self) -> Progress {
        let done: u64 = self.parts.iter().map(|p| p.1).sum();
        let total: u64 = self.parts.iter().map(|p| p.2.max(p.1)).sum();
        let pct = if total > 0 {
            done as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        let eta = if self.speed > 0.0 {
            total.saturating_sub(done) as f64 / self.speed
        } else {
            0.0
        };
        Progress {
            pct: pct.min(99.9), // %100 yalnızca dosyalar hazır olunca
            speed: self.speed,
            eta,
            ..Progress::new(DlState::Downloading, 0.0)
        }
    }
}

/// Önbellek kotayı aşarsa en eski klasörleri sil (`keep` hariç: şu an oynatılan).
fn prune(cache: &Path, keep: Option<&Path>) {
    let Ok(rd) = std::fs::read_dir(cache) else {
        return;
    };
    let mut dirs: Vec<(SystemTime, u64, PathBuf)> = rd
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir() && Some(e.path().as_path()) != keep)
        .map(|e| {
            let p = e.path();
            let t = std::fs::metadata(p.join(USED_STAMP))
                .or_else(|_| e.metadata())
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            (t, dir_size(&p), p)
        })
        .collect();
    let mut total: u64 = dirs.iter().map(|d| d.1).sum();
    dirs.sort_by_key(|d| d.0);
    for (_, size, p) in dirs {
        if total <= CACHE_QUOTA {
            break;
        }
        if std::fs::remove_dir_all(&p).is_ok() {
            total -= size;
        }
    }
}

fn dir_size(p: &Path) -> u64 {
    std::fs::read_dir(p).map_or(0, |rd| {
        rd.filter_map(Result::ok)
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_split_formats_and_title_with_pipes() {
        let p = parse_plan("KONIPLAN|Youtube|jNQXAC9IVRw|False|395+251|223779|NA|475961|a | b")
            .unwrap();
        assert_eq!(p.key, "Youtube-jNQXAC9IVRw");
        assert_eq!(p.title, "a | b");
        assert_eq!(p.formats, vec![("395".into(), 223779), ("251".into(), 0)]);
        let one = parse_plan("KONIPLAN|Generic|../x|NA|18|NA|NA|1000|t").unwrap();
        assert_eq!(one.key, "Generic-___x");
        assert_eq!(one.formats, vec![("18".into(), 1000)]);
        assert!(parse_plan("KONIPLAN|Twitch|x|True|best|NA|NA|NA|t").is_err());
    }

    #[test]
    fn meter_weights_by_bytes_and_never_reports_done() {
        let mut m = Meter::new(&[("v".into(), 900), ("a".into(), 100)]);
        m.update("v downloading 450 900 1000.5");
        let p = m.progress();
        assert!((p.pct - 45.0).abs() < 1e-9 && p.eta > 0.0);
        m.update("v finished 900 900 NA");
        m.update("a finished 100 NA NA");
        assert_eq!(m.progress().pct, 99.9);
    }
}
