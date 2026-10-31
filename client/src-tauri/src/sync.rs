//! Oda senkronu: Santral WebSocket bağlantısı + `koni-sync` motoru + libmpv.
//!
//! Bir oturum tek bir async görevdir; motor, soket ve oynatıcı adaptörü o görevin
//! içinde yaşar. UI'dan gelen komutlar kanalla gelir, durum UI'a Tauri olaylarıyla
//! gider (`sync://status`, `sync://server`, `sync://local`, `sync://media`, `sync://closed`).
//! Faz 0 ölçümü için her tick'te bir CSV satırı yazılır (`app_log_dir/sync-*.csv`).

use crate::i18n::trs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use koni_player::Player as Mpv;
use koni_sync::protocol::{
    ClientMsg, DlProgress, Media, MediaKind, Policy, RoomMode, RoomState, ServerMsg, SubFile,
};
use koni_sync::{EngineEvent, LocalState, Player, SyncConfig, SyncEngine};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::{debrid, download, media, page, AppState};

/// UI → oturum komutları.
#[derive(Debug)]
pub enum UiCmd {
    Play,
    Pause,
    Seek(f64),
    /// Odadaki medyayı herkes için kapat (`cmd close`).
    Close,
    /// Dosya yeni yüklendi: oynatıcıyı oda konumuna hizala. DETACHED iken yapılmaz;
    /// odaya dönüş yalnızca kullanıcının `Resync`'iyle olur.
    Realign,
    /// "Odayla Senkronla": DETACHED'dan dönüş (sunucuya `cmd resync`).
    Resync,
    /// Odaya web adresi aç (`cmd media`); yükleme sunucunun `state.media`'sıyla olur.
    /// `referer`: yakalayıcıdan gelen doğrudan akışın sayfası (yoksa boş).
    OpenWeb {
        src: String,
        title: String,
        referer: String,
        /// Yakalayıcının sayfada bulduğu altyazılar (`media.subs`).
        subs: Vec<SubFile>,
    },
    /// "Herkes İndirsin": odaya `download` medyası aç.
    OpenDownload(String),
    /// Odaya Real-Debrid torrenti aç (`kind: debrid`); herkes kendi hesabıyla çözer.
    OpenDebrid(Media),
    /// Tarayıcıda senkron (`kind: page`): herkes sayfayı Keşfet'te açar.
    OpenPage {
        src: String,
        title: String,
    },
    /// İndirme thread'inden: yerel ilerleme (sunucuya `dl_progress`).
    DlProgress(DlProgress),
    /// Kullanıcı yerel bir dosya yükledi (yalnızca dosya adı).
    OpenedFile(String),
    /// Host ya da yardımcı host: kronik stall eden üyeyi beklemeden devam et.
    ContinueWithout(String),
    /// Host: üyeyi yardımcı host yap / al.
    SetCohost(String, bool),
    /// Host: oda politikası.
    SetPolicy(Policy),
    /// Keşif, çark, altyazı (PROTOCOL §3.9–3.10): UI'nın hazırladığı JSON olduğu gibi
    /// gider; senkron motorunu ilgilendirmez.
    Fun(String),
    /// Yüklenen dosyanın görüntüsü yok (müzik): senkron düzeltmesi müziğe göre.
    Music(bool),
    Leave,
}

/// Senkron motorunu ilgilendirmeyen sunucu mesajları: UI'a `sync://fun` ile ham gider.
const FUN_TYPES: &[&str] = &[
    "moods",
    "deck_loading",
    "deck",
    "deck_progress",
    "match",
    "discover_failed",
    "wheel",
    "fun_error",
    "subs_results",
    "subtitle",
    "queue",
    "reaction",
    "view",
    "night",
    "night_previews",
    "night_suggestions",
    "night_results",
    "night_detail",
    "called",
];

/// Hızlı tepki gönderildikten sonra bu süre içinde gelen `bad_message` eski sunucunun
/// `react`'i tanımamasıdır: kullanıcıya gösterilmez.
const REACT_QUIET: Duration = Duration::from_secs(2);

/// "Hazır mısınız?" 3-2-1 bu kadar lead'li countdown'da gösterilir (PROTOCOL p0.14).
const COUNTDOWN_MIN_LEAD_MS: i64 = 2500;

/// Süren 3-2-1 gösterimi.
struct Countdown {
    /// Başlangıç anı, yerel saat (ms).
    start_local: f64,
    /// Sıradaki gösterilecek sayı; 0 = başladı, gösterim biter.
    next: u8,
}

impl Countdown {
    /// `state` mesajı 3-2-1 istiyorsa başlangıcı yerel saate çevirir.
    fn from_state(rs: &RoomState, engine: &SyncEngine, now: f64) -> Option<Self> {
        let lead = rs.start_at - rs.server_ts?;
        if rs.mode != RoomMode::Countdown || lead < COUNTDOWN_MIN_LEAD_MS {
            return None;
        }
        let start_local = engine
            .clock()
            .to_local(rs.start_at as f64)
            .unwrap_or(now + lead as f64);
        Some(Self {
            start_local,
            next: 3,
        })
    }
    fn due(&self) -> f64 {
        self.start_local - f64::from(self.next) * 1000.0
    }
}

/// mpv `show-text` `${…}` ifadelerini açar; kullanıcı metni düz kalsın.
fn osd_escape(s: &str) -> String {
    s.replace('$', "$$")
}

fn fun_msg(text: &str) -> Option<serde_json::Value> {
    #[derive(serde::Deserialize)]
    struct Tag<'a> {
        #[serde(borrow)]
        t: &'a str,
    }
    let tag: Tag = serde_json::from_str(text).ok()?;
    if !FUN_TYPES.contains(&tag.t) {
        return None;
    }
    serde_json::from_str(text).ok()
}

/// Odaya nasıl girileceği.
pub enum Entry {
    Create {
        name: String,
        policy: Option<Policy>,
    },
    Join {
        invite: String,
    },
}

/// Çalışan oturumun tutamağı (`AppState.sync`).
pub struct SyncHandle {
    pub id: u64,
    pub tx: mpsc::UnboundedSender<UiCmd>,
}

/// UI'a 4 Hz giden özet (`sync://status`).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Status {
    me: Option<String>,
    local_state: String,
    room_mode: Option<RoomMode>,
    local_pos: f64,
    room_pos: Option<f64>,
    drift_ms: Option<f64>,
    speed: f64,
    rtt_ms: Option<f64>,
    offset_ms: Option<f64>,
    cache_ahead: f64,
    backward_blocked: u64,
    forward_skips: u64,
    stalls: u64,
    csv_path: Option<String>,
}

/// libmpv'yi motorun `Player` trait'ine bağlar. mpv hataları loglanır, motor
/// bir sonraki tick'te gerçek durumu yeniden okur. Oda medyası `page` iken
/// (tarayıcıda senkron) okuma ve komutlar Keşfet'teki sayfanın videosuna gider.
struct MpvAdapter {
    mpv: Arc<Mpv>,
    app: AppHandle,
    /// Sarma / oynatma başlangıcından sonraki kısa pencerede HTTP akışın yeniden
    /// bağlanmasını "tampon bitti" sayma (aksi halde 250 ms sonra `Stalled` + `Hold` tetiklenir).
    grace_until: std::cell::Cell<Option<Instant>>,
    /// Duraklatılmışken (`Hold`/`Refilling`) libmpv önbelleği ayrık ses/video akışında
    /// (ör. YouTube `--audio-files-append`) `reserve_sec` (12 sn) eşiğine ulaşmadan
    /// sabitlenirse sonsuz tamponlamada kilitlenmemek için plato takibi:
    /// `(son_değer, son_artış_anı, duraklama_başlangıcı)`.
    paused_plateau: std::cell::Cell<Option<(f64, Instant, Instant)>>,
}

impl MpvAdapter {
    fn new(mpv: Arc<Mpv>, app: AppHandle) -> Self {
        Self {
            mpv,
            app,
            grace_until: std::cell::Cell::new(None),
            paused_plateau: std::cell::Cell::new(None),
        }
    }

    fn in_grace(&self) -> bool {
        self.grace_until
            .get()
            .is_some_and(|until| Instant::now() < until)
            || self.mpv.is_seeking()
    }

    fn arm_grace(&self) {
        self.grace_until
            .set(Some(Instant::now() + Duration::from_millis(2500)));
    }
}

fn log_err<T>(what: &str, r: koni_player::Result<T>) {
    if let Err(e) = r {
        eprintln!("[koni] sync: {what}: {e}");
    }
}

impl Player for MpvAdapter {
    fn position(&self) -> f64 {
        if page::active() {
            return page::position();
        }
        self.mpv.time_pos().unwrap_or(0.0)
    }
    fn is_paused(&self) -> bool {
        if page::active() {
            return page::paused();
        }
        self.mpv.is_paused()
    }
    fn cache_ahead(&self) -> f64 {
        if page::active() {
            if !page::buffering() {
                self.paused_plateau.set(None);
                return 30.0;
            }
            // Sayfa tamponlarken 3 sn'den uzun sürerse odayı sonsuz bekletme.
            let now = Instant::now();
            let (_, _, since) = self.paused_plateau.get().unwrap_or((0.0, now, now));
            self.paused_plateau.set(Some((0.0, now, since)));
            return if now.duration_since(since) >= Duration::from_millis(3000) {
                30.0
            } else {
                0.0
            };
        }
        let raw = self.mpv.demuxer_cache_duration();
        if self.in_grace() {
            self.paused_plateau.set(None);
            return raw.max(1.0);
        }
        if self.mpv.is_paused() && !self.mpv.paused_for_cache() && !self.mpv.is_seeking() {
            let now = Instant::now();
            let state = match self.paused_plateau.get() {
                Some((prev, last_up, since)) => {
                    if raw > prev + 0.15 {
                        (raw, now, since)
                    } else {
                        (prev.max(raw), last_up, since)
                    }
                }
                None => (raw, now, now),
            };
            self.paused_plateau.set(Some(state));
            let (_, last_up, since) = state;
            let plateaued =
                raw >= 0.5 && now.duration_since(last_up) >= Duration::from_millis(1200);
            let timed_out = now.duration_since(since) >= Duration::from_millis(2500);
            if plateaued || timed_out {
                return raw.max(15.0);
            }
        } else {
            self.paused_plateau.set(None);
        }
        raw
    }
    fn paused_for_cache(&self) -> bool {
        if page::active() {
            return page::buffering() && !self.in_grace();
        }
        if self.in_grace() {
            return false;
        }
        self.mpv.paused_for_cache()
    }
    fn duration(&self) -> Option<f64> {
        if page::active() {
            return page::duration();
        }
        self.mpv.duration()
    }
    fn eof_reached(&self) -> bool {
        if page::active() {
            return page::ended();
        }
        self.mpv.eof_reached()
    }
    fn set_pause(&mut self, paused: bool) {
        if !paused {
            self.arm_grace();
            self.paused_plateau.set(None);
        }
        if page::active() {
            return page::command(&self.app, if paused { "pause" } else { "play" }, 0.0);
        }
        log_err("pause", self.mpv.set_pause(paused));
    }
    fn set_speed(&mut self, speed: f64) {
        if page::active() {
            return page::command(&self.app, "rate", speed);
        }
        log_err("speed", self.mpv.set_speed(speed));
    }
    fn seek_exact(&mut self, pos: f64) {
        self.arm_grace();
        self.paused_plateau.set(None);
        if page::active() {
            return page::command(&self.app, "seek", pos);
        }
        log_err("seek", self.mpv.seek_absolute(pos));
    }
    fn show_text(&mut self, text: &str) {
        if page::active() {
            return; // mpv gizli; etkinlik Keşfet bandında görünür
        }
        log_err("show-text", self.mpv.show_text(text, 3000));
    }
}

/// UI'a giden medya durumu (`sync://media`).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MediaEvent {
    /// `resolving` | `downloading` | `ready` | `error` | `need_file` | `no_provider`
    status: &'static str,
    kind: MediaKind,
    src: String,
    title: String,
    /// `download`: görev kimliği (`dl_status.taskId` ile eşleşir).
    #[serde(skip_serializing_if = "String::is_empty")]
    task: String,
    /// `debrid`: torrentteki dosya ipucu (yarım kalan kaydı yeniden açmak için).
    #[serde(skip_serializing_if = "String::is_empty")]
    file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    idx: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Odanın medyasını yerel oynatıcıyla eşler.
struct MediaSync {
    /// Yerelde yüklü (ya da yüklenmekte olan) oda medyasının anahtarı (`media_key`).
    loaded: Option<String>,
    /// UI'a son bildirilen durum (`src`, `need_file` | `ready` | `no_provider`); tekrar yayını önler.
    file_shown: Option<(String, &'static str)>,
    /// Her yeni çözümlemede artar; eskiyen çözümleme oynatıcıya yüklenmez.
    generation: Arc<AtomicU64>,
    /// Süren "Herkes İndirsin" indirmesi; düşürülünce durur.
    job: Option<download::Job>,
    /// İndirme ilerlemesi oturum döngüsüne buradan döner.
    tx: mpsc::WeakUnboundedSender<UiCmd>,
    /// İndirme önbelleği (`%LOCALAPPDATA%\app.koni.desktop\downloads`).
    cache: std::path::PathBuf,
}

impl Drop for MediaSync {
    /// Oturum bitti: süren URL çözümlemesi sonradan oynatıcıya yüklenmesin.
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        page::set_room(None);
    }
}

/// Medyanın yerel yükleme anahtarı: indirmede görev (aynı adres yeniden indirilebilir),
/// diğerlerinde `src`.
fn media_key(m: &Media) -> &str {
    match m.kind {
        MediaKind::Download => &m.task,
        _ => &m.src,
    }
}

const CLOSED_MEDIA_SRC: &str = "__koni_closed__";

fn normalize_closed_media(msg: &mut ServerMsg) {
    match msg {
        ServerMsg::State(rs) => {
            if rs.media.as_ref().is_some_and(|m| m.src == CLOSED_MEDIA_SRC) {
                rs.media = None;
            }
        }
        ServerMsg::Joined { snapshot, .. } => {
            if snapshot
                .state
                .media
                .as_ref()
                .is_some_and(|m| m.src == CLOSED_MEDIA_SRC)
            {
                snapshot.state.media = None;
            }
        }
        ServerMsg::Activity { kind, text, .. }
            if kind == "media"
                && (text.contains(CLOSED_MEDIA_SRC) || text.ends_with("açtı: Video kapatıldı")) =>
        {
            if let Some((who, _)) = text.split_once(" açtı:") {
                *text = tr!("{who} videoyu kapattı", "{who} closed the video");
            }
        }
        _ => {}
    }
}

impl MediaSync {
    fn new(app: &AppHandle, tx: mpsc::WeakUnboundedSender<UiCmd>) -> Self {
        let cache = app
            .path()
            .app_local_data_dir()
            .unwrap_or_else(|_| std::env::temp_dir().join("koni"))
            .join("downloads");
        Self {
            loaded: None,
            file_shown: None,
            generation: Arc::default(),
            job: None,
            tx,
            cache,
        }
    }

    fn clear(&mut self, app: &AppHandle, mpv: &Arc<Mpv>, force_emit: bool) {
        let had = self.loaded.take().is_some()
            || self.file_shown.take().is_some()
            || self.job.take().is_some();
        self.job = None;
        self.file_shown = None;
        self.generation.fetch_add(1, Ordering::SeqCst);
        page::set_room(None);
        crate::tracks::clear();
        log_err("stop", mpv.command(&["stop"]));
        if had || force_emit {
            let _ = app.emit("sync://media", None::<MediaEvent>);
        }
    }

    /// "Tekrar dene" / "Odayla Senkronla": sağlayıcısız açılmayan medya bir sonraki
    /// eşlemede yeniden denenir.
    fn retry_unprovided(&mut self) {
        if self
            .file_shown
            .as_ref()
            .is_some_and(|(_, s)| *s == "no_provider")
        {
            self.loaded = None;
            self.file_shown = None;
        }
    }

    fn reconcile(&mut self, app: &AppHandle, room: Option<&Media>, mpv: &Arc<Mpv>) {
        let room = room.filter(|m| m.src != CLOSED_MEDIA_SRC);
        let Some(m) = room else {
            // Oda boşaldı (biri kapattı, p0.21): oynatıcı durur, arayüz medyayı unutur.
            // Oda boşaldı (biri kapattı, p0.21): oynatıcı durur, arayüz medyayı unutur.
            self.clear(app, mpv, false);
            if false && self.loaded.take().is_some() {
                self.job = None;
                self.file_shown = None;
                self.generation.fetch_add(1, Ordering::SeqCst);
                page::set_room(None);
                log_err("stop", mpv.command(&["stop"]));
                let _ = app.emit("sync://media", None::<MediaEvent>);
            }
            return;
        };
        page::set_room((m.kind == MediaKind::Page).then(|| m.src.clone()));
        let event = |status, error| MediaEvent {
            status,
            kind: m.kind,
            src: m.src.clone(),
            title: m.title.clone(),
            task: m.task.clone(),
            file: m.file.clone(),
            idx: m.idx,
            error,
        };
        let mut show_file = |status: &'static str| {
            let key = (m.src.clone(), status);
            if self.file_shown.as_ref() != Some(&key) {
                self.file_shown = Some(key);
                let _ = app.emit("sync://media", event(status, None));
            }
        };
        if self.loaded.as_deref() == Some(media_key(m)) {
            if m.kind == MediaKind::File {
                show_file("ready");
            }
            return;
        }
        self.job = None; // oda başka medyaya geçti: süren indirme durur
        if matches!(m.kind, MediaKind::Url | MediaKind::Debrid) {
            // Çözüm birkaç sn sürer; eski parça bu arada yüklü kalırsa motor onun konumunu
            // (biten şarkıda sonu) yeni medyanın konumu sanır: oda 0'dan başlarken "öndeyim"
            // diye başlangıcı o kadar erteler, yeni şarkı dakikalarca başlamaz. Durdurulunca
            // konum 0'dır; dosya gelene dek oda bu üyeyi bekler (takılma → HOLD).
            log_err("stop", mpv.command(&["stop"]));
        }
        match m.kind {
            MediaKind::Url => {
                self.loaded = Some(m.src.clone());
                let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
                let _ = app.emit("sync://media", event("resolving", None));
                let (app, mpv, latest) = (app.clone(), mpv.clone(), self.generation.clone());
                let base = event("ready", None);
                let referer = (!m.referer.is_empty()).then(|| m.referer.clone());
                let subs = m.subs.clone();
                // Sunucu boş başlığı adresle doldurur; o durumda dosya adı gösterilsin.
                let hint = if m.title == m.src {
                    String::new()
                } else {
                    m.title.clone()
                };
                tauri::async_runtime::spawn_blocking(move || {
                    let res = media::resolve_web(&base.src, referer.as_deref(), &hint, &subs)
                        .map_err(String::from)
                        .and_then(|s| {
                            if latest.load(Ordering::SeqCst) != generation {
                                return Ok(None); // oda bu arada başka medyaya geçti
                            }
                            media::load(&mpv, &s).map(|()| Some(s.title))
                        });
                    if latest.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    let ev = match res {
                        Ok(None) => return,
                        Ok(Some(title)) => MediaEvent { title, ..base },
                        Err(e) => MediaEvent {
                            status: "error",
                            error: Some(e),
                            ..base
                        },
                    };
                    let _ = app.emit("sync://media", ev);
                });
            }
            MediaKind::Download => {
                self.loaded = Some(m.task.clone());
                self.generation.fetch_add(1, Ordering::SeqCst); // süren URL çözümlemesi yüklenmesin
                                                                // Eski medya indirme boyunca oynamasın (oda yeni medyada 0'dan başlar).
                let _ = mpv.command(&["stop"]);
                let _ = app.emit("sync://media", event("downloading", None));
                let (tx, task) = (self.tx.clone(), m.task.clone());
                let report = move |p: download::Progress| {
                    let Some(tx) = tx.upgrade() else { return }; // oturum bitti
                    let _ = tx.send(UiCmd::DlProgress(DlProgress {
                        task_id: task.clone(),
                        state: p.state,
                        pct: p.pct,
                        speed: p.speed,
                        eta: p.eta,
                        error: p.error,
                    }));
                };
                let (app, mpv, base) = (app.clone(), mpv.clone(), event("ready", None));
                let done = move |res: Result<download::Files, String>| {
                    let res = res.and_then(|f| {
                        let s = media::Stream::plain(
                            f.video.display().to_string(),
                            f.audio.map(|a| a.display().to_string()),
                            f.title,
                        );
                        media::load(&mpv, &s)?;
                        wait_opened(&mpv, &s.video)?;
                        Ok(s.title)
                    });
                    let (ev, res) = match res {
                        Ok(title) => (MediaEvent { title, ..base }, Ok(())),
                        Err(e) => (
                            MediaEvent {
                                status: "error",
                                error: Some(e.clone()),
                                ..base
                            },
                            Err(e),
                        ),
                    };
                    let _ = app.emit("sync://media", ev);
                    res
                };
                self.job = Some(download::start(
                    media::ytdlp_bin(),
                    self.cache.clone(),
                    m.src.clone(),
                    report,
                    done,
                ));
            }
            MediaKind::File => show_file("need_file"),
            MediaKind::Page => {
                // Video sitenin oynatıcısında kalır: mpv boşaltılır, UI sayfayı Keşfet'te açar.
                self.loaded = Some(m.src.clone());
                log_err("stop", mpv.command(&["stop"]));
                let _ = app.emit("sync://media", event("ready", None));
            }
            MediaKind::Debrid => {
                self.loaded = Some(m.src.clone());
                // Anahtarı da eklentisi de yoksa bağlanmaya çalışma: UI sağlayıcı eklemeyi
                // söyler, oda bu üyeyi beklemeden devam eder (track_end failed → detached).
                if !debrid::can_open_room_media(app, &m.src) {
                    show_file("no_provider");
                    return;
                }
                self.file_shown = None;
                let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
                let _ = app.emit("sync://media", event("resolving", None));
                let (app, mpv, latest) = (app.clone(), mpv.clone(), self.generation.clone());
                let base = event("ready", None);
                let (file, idx) = (m.file.clone(), m.idx);
                tauri::async_runtime::spawn(async move {
                    let res =
                        match debrid::resolve_room_media(&app, &base.src, &file, idx, &base.title)
                            .await
                        {
                            // oda bu arada başka medyaya geçti
                            // oda bu arada başka medyaya geçti
                            _ if latest.load(Ordering::SeqCst) != generation => return,
                            Ok(_) if false => return,
                            Ok(s) => media::load(&mpv, &s).map(|()| s.title),
                            Err(e) => Err(e),
                        };
                    if latest.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    let ev = match res {
                        Ok(title) => MediaEvent { title, ..base },
                        Err(e) => MediaEvent {
                            status: "error",
                            error: Some(e),
                            ..base
                        },
                    };
                    let _ = app.emit("sync://media", ev);
                });
            }
        }
    }
}

/// mpv `loadfile` eşzamansızdır; `path` yeni dosya olup süre ve konum okunana dek bekler.
/// İndirmede `ready` buna bağlı: oda geri sayımı (yerelde 250 ms) dosya açılırken başlarsa
/// son hazır olan ~1 s geride başlıyordu.
fn wait_opened(mpv: &Mpv, path: &str) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if mpv.get_property_string("path").as_deref() == Some(path)
            && mpv.duration().is_some()
            && mpv.time_pos().is_some()
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(trs(
        "oynatıcı indirilen dosyayı açamadı",
        "the player couldn't open the downloaded file",
    )
    .into())
}

/// Odadaki medya `src` ile aynı türde ve aynı mı.
fn room_has(engine: &SyncEngine, kind: MediaKind, src: &str) -> bool {
    engine
        .room()
        .and_then(|r| r.media.as_ref())
        .is_some_and(|m| m.kind == kind && m.src == src)
}

/// Monoton yerel saat (ms). Duvar saati atlamaları ofset tahminini bozmasın diye
/// başlangıçtaki epoch'a `Instant` farkı eklenir.
struct LocalClock {
    base_ms: f64,
    start: Instant,
}

impl LocalClock {
    fn new() -> Self {
        let base_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64() * 1000.0);
        Self {
            base_ms,
            start: Instant::now(),
        }
    }
    fn now(&self) -> f64 {
        self.base_ms + self.start.elapsed().as_secs_f64() * 1000.0
    }
}

/// Odadaki kimlik: hesapsız sunucuda seçilen ad; hesaplı sunucuda oturum belirteci
/// (ad hesaptan gelir).
pub struct Who {
    pub username: String,
    pub auth: Option<String>,
}

/// Oturumu başlatır. Önceki oturum varsa çağıran kapatmalıdır.
pub fn spawn(app: AppHandle, mpv: Arc<Mpv>, url: String, who: Who, entry: Entry) -> SyncHandle {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (tx, rx) = mpsc::unbounded_channel();
    // Zayıf: UI tutamağı düşünce `rx` kapanmalı (oturum biter); indirme thread'i engellemesin.
    let tx_loop = tx.downgrade();
    tauri::async_runtime::spawn(async move {
        let reason = match run(&app, mpv, &url, who, entry, rx, tx_loop).await {
            Ok(()) => None,
            Err(e) => {
                eprintln!("[koni] sync: {e}");
                Some(e)
            }
        };
        let state = app.state::<AppState>();
        let mut slot = state.sync.lock().unwrap();
        if slot.as_ref().is_some_and(|h| h.id == id) {
            *slot = None;
        }
        drop(slot);
        let _ = app.emit("sync://closed", reason);
    });
    SyncHandle { id, tx }
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type WsError = tokio_tungstenite::tungstenite::Error;

/// Bağlantı kurma zaman aşımı.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Sunucudan bu kadar süre mesaj gelmezse bağlantı ölü sayılır (pong 3 sn'de bir gelir).
const RX_IDLE: Duration = Duration::from_secs(25);
/// Sunucu hayalet üyeyi 60 sn tutar; bu süreden sonra yeniden bağlanma denenmez.
const RECONNECT_FOR: Duration = Duration::from_secs(70);

/// Açık bir Santral bağlantısı.
struct Link {
    sink: futures_util::stream::SplitSink<Ws, Message>,
    stream: futures_util::stream::SplitStream<Ws>,
    last_rx: Instant,
}

async fn dial(url: &str, hello: &ClientMsg) -> Result<Link, String> {
    let (ws, _) = tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(url))
        .await
        .map_err(|_| {
            tr!(
                "{url} bağlantısı zaman aşımına uğradı",
                "Connection to {url} timed out"
            )
        })?
        .map_err(|e| {
            tr!(
                "{url} bağlantısı kurulamadı: {e}",
                "Couldn't connect to {url}: {e}"
            )
        })?;
    let (mut sink, stream) = ws.split();
    send(&mut sink, hello).await?;
    Ok(Link {
        sink,
        stream,
        last_rx: Instant::now(),
    })
}

async fn recv(link: &mut Option<Link>) -> Option<Result<Message, WsError>> {
    match link {
        Some(l) => l.stream.next().await,
        None => std::future::pending().await,
    }
}

/// Kopan bağlantının yeniden kurulma durumu.
struct Reconnect {
    since: Instant,
    attempt: u32,
    at: tokio::time::Instant,
    /// `resume` reddedildi: aynı davetle yeni üye olarak katıl.
    rejoin: bool,
}

/// Odaya dönmek için gerekenler (son `joined`'dan).
struct Seat {
    room: String,
    member: String,
    token: String,
    invite: String,
}

/// `sync://link`: bağlantı durumu (`online` | `reconnecting`).
#[derive(Serialize, Clone)]
struct LinkEvent {
    state: &'static str,
    attempt: u32,
}

enum Step {
    Server(Option<Result<Message, WsError>>),
    Ui(Option<UiCmd>),
    Tick,
    Deadline,
    Retry,
    Countdown,
}

async fn run(
    app: &AppHandle,
    mpv: Arc<Mpv>,
    url: &str,
    who: Who,
    entry: Entry,
    mut rx: mpsc::UnboundedReceiver<UiCmd>,
    tx: mpsc::WeakUnboundedSender<UiCmd>,
) -> Result<(), String> {
    let hello = match entry {
        Entry::Create { name, policy } => ClientMsg::CreateRoom {
            name,
            username: who.username.clone(),
            policy,
            auth: who.auth.clone(),
        },
        Entry::Join { invite } => ClientMsg::Join {
            invite,
            username: who.username.clone(),
            auth: who.auth.clone(),
        },
    };
    let mut link = Some(dial(url, &hello).await?);
    let mut seat: Option<Seat> = None;
    let mut lost: Option<Reconnect> = None;

    let cfg = SyncConfig::default();
    let mut tick = tokio::time::interval(Duration::from_millis(cfg.tick_ms as u64));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut engine = SyncEngine::new(cfg);
    let mut player = MpvAdapter::new(mpv, app.clone());
    let clock = LocalClock::new();
    let mut csv: Option<Csv> = None;
    let mut room_media = MediaSync::new(app, tx);
    // Son indirme durumu: yeniden bağlanınca (ya da yeni üye olarak dönünce) tekrar gider.
    let mut last_dl: Option<DlProgress> = None;
    let mut last_tick = Instant::now();
    let mut countdown: Option<Countdown> = None;
    let mut react_at: Option<Instant> = None;
    let mut close_cid: Option<String> = None;

    loop {
        let deadline = engine
            .next_deadline_ms()
            .map(|d| Duration::from_secs_f64(((d - clock.now()) / 1000.0).max(0.0)));
        let retry_at = lost.as_ref().map(|l| l.at);
        let cd_in = countdown
            .as_ref()
            .map(|c| Duration::from_secs_f64(((c.due() - clock.now()) / 1000.0).max(0.0)));
        let step = tokio::select! {
            msg = recv(&mut link) => Step::Server(msg),
            cmd = rx.recv() => Step::Ui(cmd),
            _ = tick.tick() => Step::Tick,
            _ = tokio::time::sleep(deadline.unwrap_or_default()), if deadline.is_some() => Step::Deadline,
            _ = tokio::time::sleep_until(retry_at.unwrap_or_else(tokio::time::Instant::now)),
                if retry_at.is_some() && link.is_none() => Step::Retry,
            _ = tokio::time::sleep(cd_in.unwrap_or_default()), if cd_in.is_some() => Step::Countdown,
        };

        let mut drop_link = false;
        let mut out: Vec<ClientMsg> = Vec::new();
        let events = match step {
            Step::Server(Some(Ok(Message::Text(text)))) => {
                if let Some(l) = link.as_mut() {
                    l.last_rx = Instant::now();
                }
                if let Some(v) = fun_msg(text.as_str()) {
                    // Tam ekranda kokpit (ve tepki baloncukları) gizli: OSD'ye kısa metin.
                    if v["t"] == "reaction" && is_fullscreen(app) {
                        let name = v["name"].as_str().unwrap_or_default();
                        let emoji = v["emoji"].as_str().unwrap_or_default();
                        let text = osd_escape(&format!("{name} {emoji}"));
                        log_err("show-text", player.mpv.show_text(&text, 1800));
                    }
                    let _ = app.emit("sync://fun", v);
                    continue;
                }
                let mut msg: ServerMsg = match serde_json::from_str(text.as_str()) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("[koni] sync: çözülemeyen mesaj ({e}): {}", text.as_str());
                        continue;
                    }
                };
                normalize_closed_media(&mut msg);
                match &msg {
                    ServerMsg::Denied { ack, reason }
                        if reason == "bad_op"
                            && close_cid.is_some()
                            && close_cid.as_deref() == Some(ack.as_str()) =>
                    {
                        close_cid = None;
                        let m = Media::new(
                            MediaKind::File,
                            CLOSED_MEDIA_SRC.into(),
                            "Video kapatıldı".into(),
                        );
                        out.push(engine.user_media(m));
                        if let Some(l) = link.as_mut() {
                            for m in out.drain(..) {
                                let _ = send(&mut l.sink, &m).await;
                            }
                        }
                        continue;
                    }
                    ServerMsg::Denied { reason, .. } if reason == "no_media" => {
                        close_cid = None;
                        continue;
                    }
                    ServerMsg::Joined {
                        you,
                        invite,
                        snapshot,
                        resume: Some(token),
                        ..
                    } => {
                        seat = Some(Seat {
                            room: snapshot.room.clone(),
                            member: you.clone(),
                            token: token.clone(),
                            invite: invite.clone(),
                        });
                        if let Some(p) = &last_dl {
                            out.push(ClientMsg::DlProgress(p.clone()));
                        }
                        if lost.take().is_some() {
                            let _ = app.emit(
                                "sync://link",
                                LinkEvent {
                                    state: "online",
                                    attempt: 0,
                                },
                            );
                        }
                    }
                    ServerMsg::Error { code, .. } if code == "resume_failed" => {
                        if let Some(l) = lost.as_mut() {
                            l.rejoin = true;
                        }
                    }
                    ServerMsg::Error { code, .. }
                        if code == "bad_message"
                            && react_at.is_some_and(|t| t.elapsed() < REACT_QUIET) =>
                    {
                        continue; // eski sunucu `react`'i tanımıyor: sessizce geç
                    }
                    ServerMsg::Error { code, message } if lost.is_some() => {
                        // yeni üye olarak da girilemiyor (oda kapandı, dolu, ad alınmış)
                        return Err(tr!(
                            "odaya geri dönülemedi: {message} ({code})",
                            "couldn't rejoin the room: {message} ({code})"
                        ));
                    }
                    _ => {}
                }
                // Sunucu yeni medyaya geçti (kuyrukta sıradaki şarkı): aynı link art arda
                // gelse de (tek şarkılık liste, çalan yeniden eklendi) baştan yüklenir.
                if let ServerMsg::State(rs) = &msg {
                    let newer = engine.room().is_none_or(|r| rs.seq > r.seq);
                    let stream = rs
                        .media
                        .as_ref()
                        .is_some_and(|m| matches!(m.kind, MediaKind::Url | MediaKind::Debrid));
                    if newer && stream && rs.cause == "media" {
                        room_media.loaded = None;
                    }
                }
                let ev = engine.on_server_msg(&msg, &mut player, clock.now());
                if let ServerMsg::State(rs) = &msg {
                    // Her yeni `state` öncekini geçersiz kılar (iptal ya da yeniden zamanlama).
                    if countdown.is_some() {
                        let _ = app.emit("sync://countdown", 0u8);
                    }
                    countdown = Countdown::from_state(rs, &engine, clock.now());
                }
                if csv.is_none() && matches!(msg, ServerMsg::Joined { .. }) {
                    csv = Csv::open(app);
                }
                if !matches!(msg, ServerMsg::Pong { .. }) {
                    let _ = app.emit("sync://server", &msg);
                }
                // Tam ekranda kokpit gizli: etkinlik video üstünde görünsün.
                if let Some(text) = osd_text(&msg) {
                    if is_fullscreen(app) {
                        player.show_text(&text);
                    }
                }
                let media = engine.room().and_then(|r| r.media.as_deref());
                room_media.reconcile(app, media, &player.mpv);
                ev
            }
            Step::Server(Some(Ok(m))) if !matches!(m, Message::Close(_)) => continue,
            Step::Server(end) => {
                if seat.is_none() {
                    // odaya hiç girilmedi: sunucu hatası UI'a `sync://server` ile gitti
                    return match end {
                        Some(Err(e)) => Err(tr!("bağlantı koptu: {e}", "connection lost: {e}")),
                        _ => Ok(()),
                    };
                }
                drop_link = true;
                Vec::new()
            }
            Step::Ui(None | Some(UiCmd::Leave)) => {
                if let Some(l) = link.as_mut() {
                    let _ = send(&mut l.sink, &ClientMsg::Leave).await;
                    let _ = l.sink.send(Message::Close(None)).await;
                }
                return Ok(());
            }
            Step::Ui(Some(UiCmd::DlProgress(p))) => {
                last_dl = Some(p.clone());
                out.push(ClientMsg::DlProgress(p)); // bağlantı yoksa resume'da gider
                Vec::new()
            }
            Step::Ui(Some(cmd)) if link.is_none() && !matches!(cmd, UiCmd::Music(_)) => {
                let _ = app.emit(
                    "sync://local",
                    trs(
                        "Bağlantı yok, komut gönderilemedi",
                        "No connection, command not sent",
                    ),
                );
                if matches!(cmd, UiCmd::OpenedFile(_)) {
                    // yerel yükleme yine de bilinen oda konumuna hizalansın
                    engine.user_resync(&mut player, clock.now())
                } else {
                    Vec::new()
                }
            }
            Step::Ui(Some(cmd)) => match cmd {
                UiCmd::Play => vec![EngineEvent::Send(engine.user_play(&player))],
                UiCmd::Pause => vec![EngineEvent::Send(engine.user_pause(&player))],
                UiCmd::Seek(pos) => vec![EngineEvent::Send(engine.user_seek(pos))],
                UiCmd::Close => {
                    room_media.clear(app, &player.mpv, true);
                    let msg = engine.user_close();
                    if let ClientMsg::Cmd(c) = &msg {
                        close_cid = Some(c.cid.clone());
                    }
                    vec![EngineEvent::Send(msg)]
                }
                UiCmd::Realign if engine.state() == LocalState::Detached => Vec::new(),
                UiCmd::Realign => engine.user_resync(&mut player, clock.now()),
                UiCmd::Resync => {
                    // Sağlayıcı sonradan eklendiyse oda medyası yeniden denensin.
                    room_media.retry_unprovided();
                    engine.user_resync(&mut player, clock.now())
                }
                UiCmd::Music(on) => {
                    engine.set_music(on);
                    Vec::new()
                }
                UiCmd::ContinueWithout(user) => {
                    vec![EngineEvent::Send(engine.user_continue_without(user))]
                }
                UiCmd::SetCohost(user, on) => {
                    vec![EngineEvent::Send(engine.user_set_cohost(user, on))]
                }
                UiCmd::SetPolicy(p) => vec![EngineEvent::Send(engine.user_set_policy(p))],
                UiCmd::OpenDownload(src) => {
                    let m = Media::new(MediaKind::Download, src, String::new());
                    vec![EngineEvent::Send(engine.user_media(m))]
                }
                UiCmd::OpenDebrid(m) => {
                    if room_has(&engine, MediaKind::Debrid, &m.src) {
                        // Aynı torrent yeniden açıldı: yerelde tekrar çöz ve yükle.
                        room_media.loaded = None;
                        let media = engine.room().and_then(|r| r.media.as_deref());
                        room_media.reconcile(app, media, &player.mpv);
                        Vec::new()
                    } else {
                        vec![EngineEvent::Send(engine.user_media(m))]
                    }
                }
                UiCmd::OpenPage { src, title } => {
                    if room_has(&engine, MediaKind::Page, &src) {
                        Vec::new()
                    } else {
                        let m = Media::new(MediaKind::Page, src, title);
                        vec![EngineEvent::Send(engine.user_media(m))]
                    }
                }
                UiCmd::DlProgress(_) => unreachable!("handled above"),
                UiCmd::OpenWeb {
                    src,
                    title,
                    referer,
                    subs,
                } => {
                    if room_has(&engine, MediaKind::Url, &src) {
                        // Aynı adres yeniden açıldı: yerelde tekrar çöz ve yükle.
                        room_media.loaded = None;
                        let media = engine.room().and_then(|r| r.media.as_deref());
                        room_media.reconcile(app, media, &player.mpv);
                        Vec::new()
                    } else {
                        let m = Media {
                            referer,
                            subs,
                            ..Media::new(MediaKind::Url, src, title)
                        };
                        vec![EngineEvent::Send(engine.user_media(m))]
                    }
                }
                UiCmd::OpenedFile(name) => {
                    room_media.loaded = Some(name.clone());
                    if room_has(&engine, MediaKind::File, &name) {
                        let media = engine.room().and_then(|r| r.media.as_deref());
                        room_media.reconcile(app, media, &player.mpv);
                        Vec::new()
                    } else {
                        let m = Media::new(MediaKind::File, name.clone(), name);
                        vec![EngineEvent::Send(engine.user_media(m))]
                    }
                }
                UiCmd::Fun(text) => {
                    if text.contains(r#""t":"react""#) {
                        react_at = Some(Instant::now());
                    }
                    if let Some(l) = link.as_mut() {
                        if let Err(e) = l.sink.send(Message::Text(text.into())).await {
                            eprintln!("[koni] sync: gönderilemedi: {e}");
                        }
                    }
                    Vec::new()
                }
                UiCmd::Leave => unreachable!("handled above"),
            },
            Step::Tick => {
                let now = clock.now();
                // Döngü kendisi takıldıysa (ör. mpv dosya açarken property okuması
                // bekledi) gelen mesajlar sokette bekliyordur: bağlantıyı suçlama.
                let stalled_loop = last_tick.elapsed() > Duration::from_secs(2);
                last_tick = Instant::now();
                if stalled_loop {
                    if let Some(l) = link.as_mut() {
                        l.last_rx = last_tick;
                    }
                }
                let ev = engine.tick(&mut player, now);
                let status = status(&engine, &player, now, csv.as_ref());
                if let Some(c) = csv.as_mut() {
                    c.row(&engine, &status, now);
                }
                let _ = app.emit("sync://status", &status);
                if seat.is_some() && link.as_ref().is_some_and(|l| l.last_rx.elapsed() > RX_IDLE) {
                    drop_link = true;
                }
                ev
            }
            Step::Deadline => engine.tick(&mut player, clock.now()),
            Step::Countdown => {
                // OSD'de büyük 3-2-1; UI başlık pilinde gösterir (`sync://countdown`, 0 = bitti).
                if let Some(c) = countdown.as_mut() {
                    // geç kalındıysa (döngü takıldı) zamanı geçen sayılar atlanır
                    let now = clock.now();
                    while c.next > 0 && now >= c.start_local - f64::from(c.next - 1) * 1000.0 {
                        c.next -= 1;
                    }
                    let n = c.next;
                    if n > 0 {
                        let big = format!("${{osd-ass-cc/0}}{{\\an5\\fs140\\b1}}{n}");
                        log_err("show-text", player.mpv.show_text(&big, 900));
                        c.next -= 1;
                    } else {
                        countdown = None;
                    }
                    let _ = app.emit("sync://countdown", n);
                }
                Vec::new()
            }
            Step::Retry => {
                let (Some(l), Some(s)) = (lost.as_mut(), seat.as_ref()) else {
                    continue;
                };
                if l.since.elapsed() > RECONNECT_FOR {
                    return Err(trs(
                        "bağlantı koptu, odaya geri dönülemedi",
                        "connection lost, couldn't rejoin the room",
                    )
                    .into());
                }
                let hello = if l.rejoin {
                    ClientMsg::Join {
                        invite: s.invite.clone(),
                        username: who.username.clone(),
                        auth: who.auth.clone(),
                    }
                } else {
                    ClientMsg::Resume {
                        room: s.room.clone(),
                        member: s.member.clone(),
                        token: s.token.clone(),
                    }
                };
                match dial(url, &hello).await {
                    Ok(new) => link = Some(new),
                    Err(e) => {
                        eprintln!("[koni] sync: yeniden bağlanılamadı: {e}");
                        l.attempt += 1;
                        let backoff = (0.5 * 2f64.powi(l.attempt as i32)).min(8.0);
                        l.at = tokio::time::Instant::now() + Duration::from_secs_f64(backoff);
                        let ev = LinkEvent {
                            state: "reconnecting",
                            attempt: l.attempt,
                        };
                        let _ = app.emit("sync://link", ev);
                    }
                }
                Vec::new()
            }
        };

        for ev in events {
            match ev {
                EngineEvent::Send(m) => out.push(m),
                EngineEvent::ForwardSkip { skipped_sec } => {
                    let text = tr!(
                        "{skipped_sec:.0} s ileri atladın",
                        "Skipped ahead {skipped_sec:.0} s"
                    );
                    player.show_text(&text);
                    let _ = app.emit("sync://local", text);
                }
                EngineEvent::ChronicStall { stalls } => {
                    let _ = app.emit(
                        "sync://local",
                        tr!(
                            "Son 4 dk'da {stalls} kez takıldın",
                            "You stalled {stalls} times in the last 4 min"
                        ),
                    );
                }
                EngineEvent::StateChanged { .. } => {}
            }
        }
        if let Some(l) = link.as_mut() {
            for m in &out {
                if send(&mut l.sink, m).await.is_err() {
                    drop_link = true;
                    break;
                }
            }
        }
        if drop_link && link.take().is_some() {
            let at = tokio::time::Instant::now() + Duration::from_millis(500);
            match lost.as_mut() {
                // resume reddedildi ya da yeni bağlantı hemen düştü: kısa bekle, yeniden dene
                Some(l) => l.at = at,
                None => {
                    lost = Some(Reconnect {
                        since: Instant::now(),
                        attempt: 0,
                        at,
                        rejoin: false,
                    });
                }
            }
            let ev = LinkEvent {
                state: "reconnecting",
                attempt: 0,
            };
            let _ = app.emit("sync://link", ev);
            player.show_text(trs(
                "Bağlantı koptu, yeniden bağlanılıyor…",
                "Connection lost, reconnecting…",
            ));
        }
    }
}

async fn send<S>(sink: &mut S, msg: &ClientMsg) -> Result<(), String>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let json = serde_json::to_string(msg).map_err(|e| e.to_string())?;
    sink.send(Message::Text(json.into()))
        .await
        .map_err(|e| tr!("gönderilemedi: {e}", "couldn't send: {e}"))
}

fn status(engine: &SyncEngine, player: &MpvAdapter, now: f64, csv: Option<&Csv>) -> Status {
    let room_mode = engine.room().map(|r| r.mode);
    let local_pos = player.position();
    let room_pos = engine.room_position(now);
    let playing = matches!(room_mode, Some(RoomMode::Playing));
    let t = engine.telemetry();
    Status {
        me: engine.me().map(str::to_owned),
        local_state: format!("{:?}", engine.state()),
        room_mode,
        local_pos,
        room_pos,
        drift_ms: room_pos
            .filter(|_| playing)
            .map(|r| (local_pos - r) * 1000.0),
        speed: player.mpv.speed(),
        rtt_ms: engine.clock().rtt_ms(),
        offset_ms: engine.clock().offset_ms(),
        cache_ahead: player.cache_ahead(),
        backward_blocked: t.backward_correction_seeks_blocked,
        forward_skips: t.forward_skips,
        stalls: t.stalls,
        csv_path: csv.map(|c| c.path.display().to_string()),
    }
}

/// Faz 0 ölçüm logu: tick başına bir satır.
struct Csv {
    path: PathBuf,
    out: BufWriter<File>,
}

impl Csv {
    fn open(app: &AppHandle) -> Option<Csv> {
        let dir = app.path().app_log_dir().ok()?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis();
        let path = dir.join(format!("sync-{stamp}.csv"));
        let res = std::fs::create_dir_all(&dir).and_then(|_| File::create(&path));
        let mut out = match res {
            Ok(f) => BufWriter::new(f),
            Err(e) => {
                eprintln!("[koni] sync: CSV açılamadı {}: {e}", path.display());
                return None;
            }
        };
        let _ = writeln!(
            out,
            "local_ms,server_ms,room_mode,local_state,local_pos,room_pos,drift_ms,speed,\
             cache_ahead,rtt_ms,offset_ms,backward_blocked,scrub_back_seeks,forward_seeks,\
             forward_skips,stalls"
        );
        eprintln!("[koni] sync: ölçüm logu {}", path.display());
        Some(Csv { path, out })
    }

    fn row(&mut self, engine: &SyncEngine, s: &Status, now: f64) {
        let opt = |v: Option<f64>| v.map(|v| format!("{v:.3}")).unwrap_or_default();
        let mode = s
            .room_mode
            .and_then(|m| serde_json::to_value(m).ok())
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        let t = engine.telemetry();
        let res = writeln!(
            self.out,
            "{:.0},{},{},{},{:.3},{},{},{:.4},{:.2},{},{},{},{},{},{},{}",
            now,
            opt(s.offset_ms.map(|o| now + o)),
            mode,
            s.local_state,
            s.local_pos,
            opt(s.room_pos),
            opt(s.drift_ms),
            s.speed,
            s.cache_ahead,
            opt(s.rtt_ms),
            opt(s.offset_ms),
            t.backward_correction_seeks_blocked,
            t.scrub_backward_seeks,
            t.forward_seeks,
            t.forward_skips,
            t.stalls,
        )
        .and_then(|_| self.out.flush());
        if let Err(e) = res {
            eprintln!("[koni] sync: CSV yazılamadı: {e}");
        }
    }
}

/// Tam ekranda OSD'de gösterilecek oda olayı metni.
fn osd_text(msg: &ServerMsg) -> Option<String> {
    if let ServerMsg::Activity { text, .. } = msg {
        Some(text.clone())
    } else {
        None
    }
}

fn is_fullscreen(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let mode = state.layout.lock().unwrap().mode;
    mode == koni_surface::LayoutMode::Fullscreen
}
