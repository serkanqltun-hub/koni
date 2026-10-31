//! Simülasyon altyapısı: sahte oynatıcı, bellek-içi oda sunucusu, gecikmeli ağ ve
//! birden çok istemciyi aynı sanal zamanda yürüten [`Harness`].
//!
//! Her şey deterministiktir (küçük LCG, harici crate yok). Mesajlar ağ kuyruğundan
//! geçerken `serde_json` ile string'e çevrilip geri çözülür; böylece protokol tipleri
//! gerçekten sınanır.
//!
//! Zaman modeli: `Harness::now_ms` sanal zamandır (0'dan başlar). Sunucu saati
//! `EPOCH_MS + now_ms`, her istemcinin yerel saati `sunucu + clock_offset_ms`.

use std::collections::{BTreeMap, VecDeque};

use crate::config::SyncConfig;
use crate::engine::{EngineEvent, LocalState, SyncEngine, Telemetry};
use crate::player::Player;
use crate::protocol::*;

/// Sunucu saatinin sanal zaman 0'daki değeri (gerçekçi bir Unix epoch ms).
pub const EPOCH_MS: i64 = 1_730_130_000_000;

// ---------------------------------------------------------------------------- RNG

/// Tohumlu, bağımsız küçük LCG (PCG benzeri sabitler).
#[derive(Debug, Clone)]
pub struct Lcg(u64);

impl Lcg {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_add(0x9E37_79B9_7F4A_7C15))
    }
    pub fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }
    /// [0, 1)
    pub fn next_f64(&mut self) -> f64 {
        self.next_u32() as f64 / (1u64 << 31) as f64
    }
}

// ---------------------------------------------------------------------------- SimPlayer

/// `SimPlayer::seek_exact` kaydı.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeekRecord {
    /// Oynatıcının sanal zamanı (ms).
    pub at_ms: i64,
    pub from: f64,
    pub to: f64,
}

/// mpv benzeri sahte oynatıcı.
///
/// - Konum, `speed` ile ilerler (duraklatılmamış, cache beklemiyor, eof değilse).
/// - İndirme: `bandwidth` (medya-sn / gerçek sn) ile `cache_ahead`'i doldurur,
///   `cache_cap` ve dosya sonu ile sınırlı.
/// - Cache 0'a inince `paused_for_cache = true` olur, cache >= 1 s olunca (veya dosya
///   sonu tamamen inince) devam eder (mpv `cache-pause-wait`).
/// - `seek_exact`: hedef zaten tamponlu aralıktaysa cache korunur, değilse temizlenir
///   (yeniden indirme). Her seek `(from, to)` olarak kaydedilir.
#[derive(Debug, Clone)]
pub struct SimPlayer {
    pos: f64,
    paused: bool,
    speed: f64,
    duration: Option<f64>,
    buffered_end: f64,
    bandwidth: f64,
    cache_cap: f64,
    pfc: bool,
    eof: bool,
    clock_ms: f64,
    seeks: Vec<SeekRecord>,
    min_speed: f64,
    max_speed: f64,
    osd: Vec<String>,
}

impl SimPlayer {
    /// `duration` saniye, `bandwidth` medya-sn / gerçek sn. Başlangıçta duraklatılmış, cache boş.
    pub fn new(duration: f64, bandwidth: f64) -> Self {
        Self {
            pos: 0.0,
            paused: true,
            speed: 1.0,
            duration: Some(duration),
            buffered_end: 0.0,
            bandwidth,
            cache_cap: 120.0,
            pfc: false,
            eof: false,
            clock_ms: 0.0,
            seeks: Vec::new(),
            min_speed: 1.0,
            max_speed: 1.0,
            osd: Vec::new(),
        }
    }

    /// İndirme hızını değiştir (ağ kısma/geri açma).
    pub fn set_bandwidth(&mut self, media_sec_per_sec: f64) {
        self.bandwidth = media_sec_per_sec;
    }
    pub fn bandwidth(&self) -> f64 {
        self.bandwidth
    }
    /// Ahead-cache üst sınırı (s). Varsayılan 120.
    pub fn set_cache_cap(&mut self, cap: f64) {
        self.cache_cap = cap;
    }
    /// Test müdahalesi: konumu seek kaydı OLMADAN değiştirir (oynatıcı "kendiliğinden"
    /// kaymış gibi). Tampon ileri uzunluğu korunur.
    pub fn teleport(&mut self, pos: f64) {
        let ahead = self.cache_ahead();
        self.pos = pos.max(0.0);
        self.buffered_end = self.pos + ahead;
        self.eof = false;
    }
    pub fn speed(&self) -> f64 {
        self.speed
    }
    /// Uygulanan en düşük / en yüksek hız (başlangıç 1.0 dahil).
    pub fn speed_range(&self) -> (f64, f64) {
        (self.min_speed, self.max_speed)
    }
    pub fn seeks(&self) -> &[SeekRecord] {
        &self.seeks
    }
    /// Geri yönlü (`to < from`) seek sayısı; sebebi ayırt etmez.
    pub fn backward_seeks(&self) -> usize {
        self.seeks.iter().filter(|s| s.to < s.from - 1e-9).count()
    }
    pub fn osd_messages(&self) -> &[String] {
        &self.osd
    }
    pub fn clock_ms(&self) -> f64 {
        self.clock_ms
    }

    /// Gerçek zamanı `dt_sec` ilerlet: önce indirme, sonra oynatma.
    pub fn advance(&mut self, dt_sec: f64) {
        self.clock_ms += dt_sec * 1000.0;
        // indirme
        let mut limit = self.pos + self.cache_cap;
        if let Some(d) = self.duration {
            limit = limit.min(d);
        }
        if self.buffered_end < limit {
            self.buffered_end = (self.buffered_end + self.bandwidth * dt_sec).min(limit);
        }
        let all_downloaded = self.duration.is_some_and(|d| self.buffered_end >= d - 1e-9);
        // cache-pause-wait
        if self.pfc && (self.cache_ahead() >= 1.0 || all_downloaded) {
            self.pfc = false;
        }
        if !self.paused && !self.pfc && !self.eof {
            let adv = self.speed * dt_sec;
            let avail = self.cache_ahead();
            if adv > avail && !all_downloaded {
                self.pos += avail;
                self.pfc = true;
            } else {
                self.pos += adv;
            }
        }
        if let Some(d) = self.duration {
            if self.pos >= d {
                self.pos = d;
                self.eof = true;
            }
        }
    }
}

impl Player for SimPlayer {
    fn position(&self) -> f64 {
        self.pos
    }
    fn is_paused(&self) -> bool {
        self.paused
    }
    fn cache_ahead(&self) -> f64 {
        (self.buffered_end - self.pos).max(0.0)
    }
    fn paused_for_cache(&self) -> bool {
        self.pfc
    }
    fn duration(&self) -> Option<f64> {
        self.duration
    }
    fn eof_reached(&self) -> bool {
        self.eof
    }
    fn set_pause(&mut self, paused: bool) {
        self.paused = paused;
    }
    fn set_speed(&mut self, speed: f64) {
        self.speed = speed;
        self.min_speed = self.min_speed.min(speed);
        self.max_speed = self.max_speed.max(speed);
    }
    fn seek_exact(&mut self, pos: f64) {
        let to = match self.duration {
            Some(d) => pos.clamp(0.0, d),
            None => pos.max(0.0),
        };
        self.seeks.push(SeekRecord {
            at_ms: self.clock_ms as i64,
            from: self.pos,
            to,
        });
        let in_cache = to >= self.pos && to <= self.buffered_end;
        if !in_cache {
            self.buffered_end = to;
            self.pfc = false;
        }
        self.pos = to;
        self.eof = self.duration.is_some_and(|d| to >= d);
    }
    fn show_text(&mut self, text: &str) {
        self.osd.push(text.to_string());
    }
}

// ---------------------------------------------------------------------------- SimRoom

struct SimMember {
    conn: usize,
    id: String,
    name: String,
    host: bool,
    rtt_ms: Option<f64>,
}

/// PROTOCOL.md §5 (Faz 0) odasının bellek-içi sunucu tarafı. Zaman sunucu ms'idir.
///
/// `handle`/`poll` çıktısı `(bağlantı_no, mesaj)` listesidir; yönlendirmeyi çağıran yapar.
pub struct SimRoom {
    members: Vec<SimMember>,
    state: RoomState,
    policy: Policy,
    created: bool,
    room_id: String,
    invite: String,
    seq: u64,
}

type Out = Vec<(usize, ServerMsg)>;

impl Default for SimRoom {
    fn default() -> Self {
        Self::new()
    }
}

impl SimRoom {
    pub fn new() -> Self {
        Self {
            members: Vec::new(),
            state: RoomState {
                seq: 1,
                mode: RoomMode::Paused,
                anchor_pos: 0.0,
                anchor_ts: 0,
                start_at: 0,
                rate: 1.0,
                by: None,
                cause: "snapshot".into(),
                ack: None,
                server_ts: None,
                hold_pos: None,
                waiting_for: Vec::new(),
                reason: None,
                media: None,
            },
            policy: Policy::Free,
            created: false,
            room_id: "r-sim".into(),
            invite: "k3F9sim".into(),
            seq: 1,
        }
    }

    pub fn state(&self) -> &RoomState {
        &self.state
    }
    pub fn mode(&self) -> RoomMode {
        self.state.mode
    }
    /// Oda konumu (s), sunucu saati `server_ms`'te.
    pub fn position(&self, server_ms: i64) -> f64 {
        self.state.position_at(server_ms as f64)
    }
    /// Üyenin sunucu tarafı RTT ölçümü (`startAt` hesabı için).
    pub fn set_rtt(&mut self, conn: usize, rtt_ms: f64) {
        if let Some(m) = self.members.iter_mut().find(|m| m.conn == conn) {
            m.rtt_ms = Some(rtt_ms);
        }
    }
    pub fn member_id(&self, conn: usize) -> Option<&str> {
        self.members
            .iter()
            .find(|m| m.conn == conn)
            .map(|m| m.id.as_str())
    }

    fn rtt_p95(&self) -> f64 {
        self.members
            .iter()
            .filter_map(|m| m.rtt_ms)
            .fold(250.0_f64, f64::max)
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            room: self.room_id.clone(),
            name: "sim".into(),
            policy: self.policy,
            view: String::new(),
            host: self
                .members
                .iter()
                .find(|m| m.host)
                .map(|m| m.id.clone())
                .unwrap_or_default(),
            members: self
                .members
                .iter()
                .map(|m| Member {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    host: m.host,
                    cohost: false,
                    away: false,
                    detached: false,
                    admin: false,
                })
                .collect(),
            state: RoomState {
                cause: "snapshot".into(),
                ack: None,
                by: None,
                ..self.state.clone()
            },
            downloads: Vec::new(),
        }
    }

    fn add_member(&mut self, conn: usize, name: &str, host: bool) -> String {
        let id = format!("u{}", self.members.len() + 1);
        self.members.push(SimMember {
            conn,
            id: id.clone(),
            name: name.to_string(),
            host,
            rtt_ms: None,
        });
        id
    }

    /// Yeni `seq` ile state yayını; `ack` yalnızca `sender`a gider.
    fn broadcast(&mut self, now: i64, sender: Option<(usize, &str)>) -> Out {
        self.seq += 1;
        self.state.seq = self.seq;
        self.state.server_ts = Some(now);
        let mut out = Vec::new();
        for m in &self.members {
            let mut st = self.state.clone();
            st.ack = match sender {
                Some((c, cid)) if c == m.conn => Some(cid.to_string()),
                _ => None,
            };
            out.push((m.conn, ServerMsg::State(st)));
        }
        out
    }

    fn by_of(&self, conn: usize) -> Option<By> {
        self.members.iter().find(|m| m.conn == conn).map(|m| By {
            id: m.id.clone(),
            name: m.name.clone(),
        })
    }

    fn clear_hold(&mut self) {
        self.state.hold_pos = None;
        self.state.waiting_for.clear();
        self.state.reason = None;
    }

    fn start_countdown(&mut self, now: i64, pos: f64, cause: &str) {
        self.clear_hold();
        self.state.mode = RoomMode::Countdown;
        self.state.anchor_pos = pos;
        self.state.anchor_ts = now;
        self.state.start_at = now + self.rtt_p95().max(250.0).round() as i64;
        self.state.cause = cause.into();
    }

    /// Zamanla tetiklenen geçişler: COUNTDOWN → PLAYING (`cause: "start"`).
    pub fn poll(&mut self, now: i64) -> Out {
        if self.state.mode == RoomMode::Countdown && now >= self.state.start_at {
            self.state.mode = RoomMode::Playing;
            self.state.anchor_ts = self.state.start_at;
            self.state.cause = "start".into();
            self.state.by = None;
            return self.broadcast(now, None);
        }
        Vec::new()
    }

    /// Bir bağlantıdan gelen mesajı işle.
    pub fn handle(&mut self, now: i64, conn: usize, msg: ClientMsg) -> Out {
        match msg {
            ClientMsg::CreateRoom {
                name,
                username,
                policy,
                ..
            } => {
                if self.created {
                    return vec![(conn, err("bad_message"))];
                }
                self.created = true;
                self.policy = policy.unwrap_or(Policy::Free);
                self.state.anchor_ts = now;
                let id = self.add_member(conn, &username, true);
                vec![
                    (
                        conn,
                        ServerMsg::RoomCreated {
                            room: self.room_id.clone(),
                            name,
                            invite: self.invite.clone(),
                            you: id.clone(),
                        },
                    ),
                    (
                        conn,
                        ServerMsg::Joined {
                            you: id,
                            invite: self.invite.clone(),
                            snapshot: Box::new(self.snapshot()),
                            server_ts: now,
                            resume: None,
                            resumed: false,
                        },
                    ),
                ]
            }
            ClientMsg::Join {
                invite, username, ..
            } => {
                if !self.created || invite != self.invite {
                    return vec![(conn, err("bad_invite"))];
                }
                let id = self.add_member(conn, &username, false);
                let mut out = vec![(
                    conn,
                    ServerMsg::Joined {
                        you: id.clone(),
                        invite: self.invite.clone(),
                        snapshot: Box::new(self.snapshot()),
                        server_ts: now,
                        resume: None,
                        resumed: false,
                    },
                )];
                for m in self.members.iter().filter(|m| m.conn != conn) {
                    out.push((
                        m.conn,
                        ServerMsg::MemberJoined {
                            member: Member {
                                id: id.clone(),
                                name: username.clone(),
                                host: false,
                                cohost: false,
                                away: false,
                                detached: false,
                                admin: false,
                            },
                        },
                    ));
                }
                out
            }
            ClientMsg::Ping { t0, .. } => vec![(conn, ServerMsg::Pong { t0, server_ts: now })],
            ClientMsg::Cmd(cmd) => self.on_cmd(now, conn, cmd),
            ClientMsg::Report(r) => self.on_report(now, conn, r),
            // Simülasyon tek bağlantılı; yeniden bağlanma, indirme ve hesap motoru etkilemez.
            ClientMsg::Resume { .. }
            | ClientMsg::Leave
            | ClientMsg::DlProgress(_)
            | ClientMsg::Register { .. }
            | ClientMsg::Login { .. }
            | ClientMsg::Logout { .. }
            | ClientMsg::Stats { .. }
            | ClientMsg::Feed { .. }
            | ClientMsg::Lists { .. }
            | ClientMsg::Friends { .. }
            | ClientMsg::Profile { .. }
            | ClientMsg::Admin { .. }
            | ClientMsg::Hello { .. }
            | ClientMsg::BugReport { .. } => Vec::new(),
        }
    }

    fn on_cmd(&mut self, now: i64, conn: usize, cmd: Cmd) -> Out {
        let Some(me) = self.members.iter().find(|m| m.conn == conn) else {
            return vec![(conn, err("not_joined"))];
        };
        let is_host = me.host;
        let denied = |reason: &str| {
            vec![(
                conn,
                ServerMsg::Denied {
                    ack: cmd.cid.clone(),
                    reason: reason.into(),
                },
            )]
        };
        if cmd.op == CmdOp::SetPolicy {
            if !is_host {
                return denied("not_host");
            }
            if let Some(p) = cmd.policy {
                self.policy = p;
            }
            return Vec::new();
        }
        if self.policy == Policy::HostOnly && !is_host {
            return denied("host_only");
        }
        let cur = self.position(now);
        let by = self.by_of(conn);
        let cid = cmd.cid.clone();
        match cmd.op {
            CmdOp::Play => {
                if self.state.mode == RoomMode::Playing {
                    return Vec::new();
                }
                // HOLD'da play: bekleyenleri bırakmadan geri sayım (basit davranış).
                let pos = cmd.pos.unwrap_or(cur);
                self.start_countdown(now, pos, "play");
                self.state.by = by;
            }
            CmdOp::Pause => {
                let pos = cmd.pos.unwrap_or(cur);
                self.clear_hold();
                self.state.mode = RoomMode::Paused;
                self.state.anchor_pos = pos;
                self.state.anchor_ts = now;
                self.state.start_at = 0;
                self.state.cause = "pause".into();
                self.state.by = by;
            }
            CmdOp::Seek => {
                let pos = cmd.pos.unwrap_or(cur);
                self.start_countdown(now, pos, "seek");
                self.state.by = by;
            }
            CmdOp::Media => {
                self.clear_hold();
                self.state.mode = RoomMode::Paused;
                self.state.anchor_pos = 0.0;
                self.state.anchor_ts = now;
                self.state.start_at = 0;
                self.state.cause = "media".into();
                self.state.by = by;
                self.state.media = cmd.media.clone();
            }
            CmdOp::SetPolicy => unreachable!(),
            // Simülasyon DETACHED/kronik stall akışını ve yardımcı hostu modellemiyor.
            CmdOp::ContinueWithout | CmdOp::Resync | CmdOp::SetCohost | CmdOp::Close => {
                return Vec::new()
            }
        }
        self.broadcast(now, Some((conn, &cid)))
    }

    fn on_report(&mut self, now: i64, conn: usize, r: Report) -> Out {
        let Some(id) = self.member_id(conn).map(str::to_string) else {
            return Vec::new();
        };
        match r.state {
            ReportState::Stalled => match self.state.mode {
                RoomMode::Playing | RoomMode::Countdown => {
                    let hold_pos = self.position(now);
                    self.state.mode = RoomMode::Hold;
                    self.state.anchor_pos = hold_pos;
                    self.state.anchor_ts = now;
                    self.state.start_at = 0;
                    self.state.hold_pos = Some(hold_pos);
                    self.state.waiting_for = vec![id];
                    self.state.reason = Some("stall".into());
                    self.state.cause = "stall".into();
                    self.state.by = None;
                    self.broadcast(now, None)
                }
                RoomMode::Hold if !self.state.waiting_for.contains(&id) => {
                    self.state.waiting_for.push(id);
                    self.state.cause = "stall".into();
                    self.state.by = None;
                    self.broadcast(now, None)
                }
                _ => Vec::new(),
            },
            ReportState::Ready if self.state.mode == RoomMode::Hold => {
                if !self.state.waiting_for.contains(&id) {
                    return Vec::new();
                }
                self.state.waiting_for.retain(|w| *w != id);
                if self.state.waiting_for.is_empty() {
                    let pos = self.state.hold_pos.unwrap_or(self.state.anchor_pos);
                    self.start_countdown(now, pos, "release");
                    self.state.by = None;
                }
                self.broadcast(now, None)
            }
            _ => Vec::new(),
        }
    }
}

fn err(code: &str) -> ServerMsg {
    ServerMsg::Error {
        code: code.into(),
        message: String::new(),
    }
}

// ---------------------------------------------------------------------------- Harness

/// Bir simüle istemcinin özellikleri.
#[derive(Debug, Clone)]
pub struct ClientSetup {
    /// `yerel = sunucu + clock_offset_ms`.
    pub clock_offset_ms: i64,
    /// İstemci → sunucu tek yön taban gecikme (ms).
    pub up_ms: f64,
    /// Sunucu → istemci tek yön taban gecikme (ms).
    pub down_ms: f64,
    /// Her mesaja eklenen [0, jitter] ms rastgele gecikme.
    pub jitter_ms: f64,
    pub duration: f64,
    /// Başlangıç indirme hızı (medya-sn / sn).
    pub bandwidth: f64,
    pub cache_cap: f64,
}

impl Default for ClientSetup {
    fn default() -> Self {
        Self {
            clock_offset_ms: 0,
            up_ms: 30.0,
            down_ms: 30.0,
            jitter_ms: 10.0,
            duration: 7200.0,
            bandwidth: 20.0,
            cache_cap: 120.0,
        }
    }
}

/// Bir istemci: motor + oynatıcı + iki yönlü ağ kuyruğu.
pub struct Client {
    pub engine: SyncEngine,
    pub player: SimPlayer,
    pub setup: ClientSetup,
    up: VecDeque<(i64, String)>,
    down: VecDeque<(i64, String)>,
    last_up_at: i64,
    last_down_at: i64,
    next_tick_local: f64,
}

/// Kaydedilen motor olayı.
#[derive(Debug, Clone)]
pub struct LoggedEvent {
    pub t_ms: i64,
    pub client: usize,
    pub event: EngineEvent,
}

/// İstemcinin sunucuya gönderdiği mesaj + gönderim anındaki oynatıcı görüntüsü.
#[derive(Debug, Clone)]
pub struct LoggedSend {
    pub t_ms: i64,
    pub client: usize,
    pub msg: ClientMsg,
    pub player_pos: f64,
    pub cache_ahead: f64,
}

/// Sapma istatistiği (s, mutlak).
#[derive(Debug, Clone, Copy)]
pub struct DriftStats {
    pub n: usize,
    pub median: f64,
    pub p95: f64,
    pub max: f64,
}

/// N istemci + oda; sanal zamanı küçük adımlarla ilerletir.
pub struct Harness {
    pub cfg: SyncConfig,
    pub now_ms: i64,
    pub step_ms: i64,
    pub room: SimRoom,
    pub clients: Vec<Client>,
    pub events: Vec<LoggedEvent>,
    pub sent: Vec<LoggedSend>,
    rng: Lcg,
    drift: BTreeMap<(usize, usize), Vec<f64>>,
    scrub_windows: Vec<(i64, i64)>,
    next_sample_ms: i64,
}

impl Harness {
    /// İstemcileri kurar, ilk istemci oda açar, diğerleri katılır ve ~4 sn ısınma
    /// (ping'ler, saat ofseti) oynatılır. Oda `paused`, konum 0.
    pub fn new(cfg: SyncConfig, setups: Vec<ClientSetup>, seed: u64) -> Self {
        let mut room = SimRoom::new();
        let mut clients = Vec::new();
        for (i, s) in setups.iter().enumerate() {
            let mut player = SimPlayer::new(s.duration, s.bandwidth);
            player.set_cache_cap(s.cache_cap);
            room.set_rtt(i, s.up_ms + s.down_ms + s.jitter_ms);
            clients.push(Client {
                engine: SyncEngine::new(cfg.clone()),
                player,
                setup: s.clone(),
                up: VecDeque::new(),
                down: VecDeque::new(),
                last_up_at: 0,
                last_down_at: 0,
                next_tick_local: 37.0 * i as f64,
            });
        }
        let mut h = Self {
            cfg,
            now_ms: 0,
            step_ms: 10,
            room,
            clients,
            events: Vec::new(),
            sent: Vec::new(),
            rng: Lcg::new(seed),
            drift: BTreeMap::new(),
            scrub_windows: Vec::new(),
            next_sample_ms: 0,
        };
        // Sunucu tarafı RTT ölçümü, üyeler katılınca geçerli olsun diye şimdiden set edilemez
        // (üye yok); join sonrası tekrar set ediyoruz.
        h.send_up(
            0,
            ClientMsg::CreateRoom {
                name: "sim".into(),
                username: "user1".into(),
                policy: None,
                auth: None,
            },
        );
        h.run_ms(300);
        for i in 1..h.clients.len() {
            h.send_up(
                i,
                ClientMsg::Join {
                    invite: "k3F9sim".into(),
                    username: format!("user{}", i + 1),
                    auth: None,
                },
            );
        }
        h.run_ms(200);
        for i in 0..h.clients.len() {
            let s = &h.clients[i].setup;
            let rtt = s.up_ms + s.down_ms + s.jitter_ms;
            h.room.set_rtt(i, rtt);
        }
        h.run_ms(4000);
        h
    }

    /// Basit iki istemcili düzen: ofsetler −5000 / +12345 ms, farklı gecikmeler.
    pub fn two_default(seed: u64) -> Self {
        Self::new(
            SyncConfig::default(),
            vec![
                ClientSetup {
                    clock_offset_ms: -5000,
                    up_ms: 30.0,
                    down_ms: 45.0,
                    jitter_ms: 15.0,
                    ..Default::default()
                },
                ClientSetup {
                    clock_offset_ms: 12345,
                    up_ms: 80.0,
                    down_ms: 120.0,
                    jitter_ms: 30.0,
                    ..Default::default()
                },
            ],
            seed,
        )
    }

    // ----------------------------------------------------------------- zaman

    pub fn server_now(&self) -> i64 {
        EPOCH_MS + self.now_ms
    }
    pub fn local_now(&self, i: usize) -> f64 {
        (self.server_now() + self.clients[i].setup.clock_offset_ms) as f64
    }

    fn delay(&mut self, base: f64, jitter: f64) -> i64 {
        (base + self.rng.next_f64() * jitter).round() as i64
    }

    fn send_up(&mut self, i: usize, msg: ClientMsg) {
        let json = serde_json::to_string(&msg).expect("serialize ClientMsg");
        let (base, jit) = (self.clients[i].setup.up_ms, self.clients[i].setup.jitter_ms);
        let d = self.delay(base, jit);
        let c = &mut self.clients[i];
        let at = (self.now_ms + d).max(c.last_up_at);
        c.last_up_at = at;
        c.up.push_back((at, json));
    }

    fn send_down(&mut self, i: usize, msg: &ServerMsg) {
        let json = serde_json::to_string(msg).expect("serialize ServerMsg");
        let (base, jit) = (
            self.clients[i].setup.down_ms,
            self.clients[i].setup.jitter_ms,
        );
        let d = self.delay(base, jit);
        let c = &mut self.clients[i];
        let at = (self.now_ms + d).max(c.last_down_at);
        c.last_down_at = at;
        c.down.push_back((at, json));
    }

    fn route(&mut self, out: Out) {
        for (conn, msg) in out {
            if conn < self.clients.len() {
                self.send_down(conn, &msg);
            }
        }
    }

    fn process_events(&mut self, i: usize, evs: Vec<EngineEvent>) {
        for e in evs {
            if let EngineEvent::Send(m) = &e {
                self.sent.push(LoggedSend {
                    t_ms: self.now_ms,
                    client: i,
                    msg: m.clone(),
                    player_pos: self.clients[i].player.position(),
                    cache_ahead: self.clients[i].player.cache_ahead(),
                });
                let m = m.clone();
                self.send_up(i, m);
            }
            self.events.push(LoggedEvent {
                t_ms: self.now_ms,
                client: i,
                event: e,
            });
        }
    }

    /// Sanal zamanı bir adım (`step_ms`) ilerlet.
    pub fn step(&mut self) {
        self.now_ms += self.step_ms;
        let sn = self.server_now();

        let out = self.room.poll(sn);
        self.route(out);

        for i in 0..self.clients.len() {
            while self.clients[i]
                .up
                .front()
                .is_some_and(|(at, _)| *at <= self.now_ms)
            {
                let (_, json) = self.clients[i].up.pop_front().unwrap();
                let msg: ClientMsg = serde_json::from_str(&json).expect("parse ClientMsg");
                let out = self.room.handle(sn, i, msg);
                self.route(out);
            }
        }

        for i in 0..self.clients.len() {
            while self.clients[i]
                .down
                .front()
                .is_some_and(|(at, _)| *at <= self.now_ms)
            {
                let (_, json) = self.clients[i].down.pop_front().unwrap();
                let msg: ServerMsg = serde_json::from_str(&json).expect("parse ServerMsg");
                let local = self.local_now(i);
                let c = &mut self.clients[i];
                let evs = c.engine.on_server_msg(&msg, &mut c.player, local);
                self.process_events(i, evs);
            }
        }

        let dt = self.step_ms as f64 / 1000.0;
        for c in &mut self.clients {
            c.player.advance(dt);
        }

        for i in 0..self.clients.len() {
            let local = self.local_now(i);
            if local >= self.clients[i].next_tick_local {
                self.clients[i].next_tick_local =
                    local.max(self.clients[i].next_tick_local) + self.cfg.tick_ms;
                let c = &mut self.clients[i];
                let evs = c.engine.tick(&mut c.player, local);
                self.process_events(i, evs);
            }
        }

        if self.now_ms >= self.next_sample_ms {
            self.next_sample_ms = self.now_ms + 100;
            self.sample_drift();
        }
    }

    fn sample_drift(&mut self) {
        if self.room.mode() != RoomMode::Playing {
            return;
        }
        let ok = |s: LocalState| matches!(s, LocalState::Synced | LocalState::Correcting);
        let n = self.clients.len();
        for a in 0..n {
            for b in (a + 1)..n {
                if ok(self.clients[a].engine.state()) && ok(self.clients[b].engine.state()) {
                    let d = (self.clients[a].player.position() - self.clients[b].player.position())
                        .abs();
                    self.drift.entry((a, b)).or_default().push(d);
                }
            }
        }
    }

    /// `ms` sanal milisaniye çalıştır.
    pub fn run_ms(&mut self, ms: i64) {
        let end = self.now_ms + ms;
        while self.now_ms < end {
            self.step();
        }
    }

    /// `cond` doğru olana dek (en çok `max_ms`) çalıştır. Doğru olduysa `true`.
    pub fn run_until(&mut self, max_ms: i64, mut cond: impl FnMut(&Harness) -> bool) -> bool {
        let end = self.now_ms + max_ms;
        while self.now_ms < end {
            if cond(self) {
                return true;
            }
            self.step();
        }
        cond(self)
    }

    // ----------------------------------------------------------------- kullanıcı eylemleri / müdahaleler

    pub fn user_play(&mut self, i: usize) {
        let c = &mut self.clients[i];
        let msg = c.engine.user_play(&c.player);
        self.send_up(i, msg);
    }
    pub fn user_pause(&mut self, i: usize) {
        let c = &mut self.clients[i];
        let msg = c.engine.user_pause(&c.player);
        self.send_up(i, msg);
    }
    /// Bilinçli scrub: sonraki 5 sn'deki geri seek'ler "scrub" sayılır.
    pub fn user_seek(&mut self, i: usize, pos: f64) {
        self.scrub_windows.push((self.now_ms, self.now_ms + 5000));
        let msg = self.clients[i].engine.user_seek(pos);
        self.send_up(i, msg);
    }
    /// Ağ hızını (medya-sn/sn) değiştir.
    pub fn throttle(&mut self, i: usize, bandwidth: f64) {
        self.clients[i].player.set_bandwidth(bandwidth);
    }
    /// Oynatıcıyı seek kaydı olmadan `delta` sn kaydır (+ ileri).
    pub fn nudge(&mut self, i: usize, delta: f64) {
        let p = self.clients[i].player.position() + delta;
        self.clients[i].player.teleport(p);
    }

    // ----------------------------------------------------------------- gözlem

    pub fn room_mode(&self) -> RoomMode {
        self.room.mode()
    }
    /// Gerçek oda konumu (s) şu an.
    pub fn room_pos(&self) -> f64 {
        self.room.position(self.server_now())
    }
    pub fn pos(&self, i: usize) -> f64 {
        self.clients[i].player.position()
    }
    /// İstemci konumu − oda konumu (+ = önde).
    pub fn drift_vs_room(&self, i: usize) -> f64 {
        self.pos(i) - self.room_pos()
    }
    pub fn state(&self, i: usize) -> LocalState {
        self.clients[i].engine.state()
    }
    pub fn telemetry(&self, i: usize) -> &Telemetry {
        self.clients[i].engine.telemetry()
    }
    pub fn player(&self, i: usize) -> &SimPlayer {
        &self.clients[i].player
    }

    pub fn events_of(&self, i: usize) -> impl Iterator<Item = &LoggedEvent> {
        self.events.iter().filter(move |e| e.client == i)
    }
    /// `since_ms`'ten beri `i` istemcisi `to` durumuna girdi mi.
    pub fn entered_state(&self, i: usize, to: LocalState, since_ms: i64) -> bool {
        self.events_of(i).any(|e| {
            e.t_ms >= since_ms
                && matches!(&e.event, EngineEvent::StateChanged { to: t, .. } if *t == to)
        })
    }
    /// `since_ms`'ten beri `to` durumuna girişlerin sayısı.
    pub fn count_entered(&self, i: usize, to: LocalState, since_ms: i64) -> usize {
        self.events_of(i)
            .filter(|e| {
                e.t_ms >= since_ms
                    && matches!(&e.event, EngineEvent::StateChanged { to: t, .. } if *t == to)
            })
            .count()
    }
    /// `i` istemcisinin gönderdiği raporlar.
    pub fn reports_of(&self, i: usize) -> impl Iterator<Item = (&LoggedSend, &Report)> {
        self.sent
            .iter()
            .filter(move |s| s.client == i)
            .filter_map(|s| match &s.msg {
                ClientMsg::Report(r) => Some((s, r)),
                _ => None,
            })
    }

    /// Scrub penceresi dışındaki geri seek sayısı (düzeltme amaçlı olanlar = hata).
    pub fn non_scrub_backward_seeks(&self, i: usize) -> usize {
        self.clients[i]
            .player
            .seeks()
            .iter()
            .filter(|s| s.to < s.from - 1e-9)
            .filter(|s| {
                !self
                    .scrub_windows
                    .iter()
                    .any(|(a, b)| s.at_ms >= *a && s.at_ms <= *b)
            })
            .count()
    }

    /// `(a, b)` çifti için oynarken biriken |konum farkı| örneklerini sil.
    pub fn clear_drift(&mut self) {
        self.drift.clear();
    }
    /// `(a, b)` (a < b) çiftinin sapma istatistiği.
    pub fn drift_stats(&self, a: usize, b: usize) -> Option<DriftStats> {
        let v = self.drift.get(&(a.min(b), a.max(b)))?;
        if v.is_empty() {
            return None;
        }
        let mut s = v.clone();
        s.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let q = |p: f64| s[((s.len() - 1) as f64 * p).round() as usize];
        Some(DriftStats {
            n: s.len(),
            median: q(0.5),
            p95: q(0.95),
            max: *s.last().unwrap(),
        })
    }

    /// Her senaryoda geçerli genel değişmezler; ihlalde `Err(açıklama)`.
    pub fn check_invariants(&self) -> Result<(), String> {
        for i in 0..self.clients.len() {
            let n = self.non_scrub_backward_seeks(i);
            if n != 0 {
                return Err(format!(
                    "istemci {i}: {n} scrub-dışı geri seek: {:?}",
                    self.player(i).seeks()
                ));
            }
            let t = self.telemetry(i);
            if t.backward_correction_seeks_blocked != 0 {
                return Err(format!(
                    "istemci {i}: backward_correction_seeks_blocked = {}",
                    t.backward_correction_seeks_blocked
                ));
            }
            let (lo, hi) = self.player(i).speed_range();
            if lo < 0.96 - 1e-9 || hi > 1.04 + 1e-9 {
                return Err(format!("istemci {i}: hız aralığı [{lo}, {hi}]"));
            }
        }
        Ok(())
    }
}
