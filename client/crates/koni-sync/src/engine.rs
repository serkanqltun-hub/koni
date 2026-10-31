//! İstemci senkron state makinesi (docs/design/sync.md §4.4–4.7, PROTOCOL p0.1).
//!
//! Motor saf ve deterministiktir: zaman dışarıdan `now_local_ms` olarak verilir,
//! oynatıcıya yalnızca [`Player`] üzerinden dokunulur, ağa gidecek her şey
//! [`EngineEvent::Send`] olarak döner. Kendi thread'i, soketi, saati yoktur.
//!
//! ## Kurallar (sapma = yerelPos − odaPos, + = önde)
//! | Durum | Eylem |
//! |---|---|
//! | \|sapma\| < 50 ms | hız 1.0 |
//! | 50 ms – 3.0 s | hız 0.96–1.04, orantısal ([`SpeedController`]) |
//! | > 3.0 s önde | `AheadHold`: yerel pause, oda konumu yerel konuma gelince yeniden başla. Geri seek YOK |
//! | 3.0 – 30 s geride | `CatchingUp`: geri seek yok; oda HOLD(catch_up) gelirse holdPos'a kadar oynayıp `ready`. Faz 0 sunucusu catch_up yapmıyor → bu arada 1.04 ile oyna, `catching_up` raporla |
//! | > 30 s geride | oda konumuna İLERİ seek, `report.skippedSec`, [`EngineEvent::ForwardSkip`] |
//! | stall | `Stalled` → `report(stalled)`; tampon dolarken `Refilling`; rezerv ≥ 12 s → `ready` |
//!
//! Geri seek yalnızca bilinçli scrub'da (`state.cause == "seek"`) serbesttir.

use crate::buffer::{ReserveGate, StallDetector, StallHistory};
use crate::clock::ClockSync;
use crate::config::SyncConfig;
use crate::controller::SpeedController;
use crate::player::Player;
use crate::protocol::{
    ClientMsg, Cmd, CmdOp, Media, Policy, Report, ReportState, RoomMode, RoomState, ServerMsg,
};

/// İstemcinin yerel durumu (docs/design/sync.md §4.5 "İstemci (yerel)").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalState {
    /// Henüz `joined` alınmadı / oda state'i yok.
    Idle,
    /// Oda `paused`; oynatıcı anchor'da duraklatılmış.
    Paused,
    /// Oda `countdown`; `startAt` bekleniyor (yerel öndeyse başlangıç gecikmeli).
    Countdown,
    /// Oynuyor, |sapma| ölü bantta.
    Synced,
    /// Oynuyor, hız ≠ 1.0.
    Correcting,
    /// > 3.0 s önde, yerel bekleme.
    AheadHold,
    /// 3–30 s geride, yetişmeye çalışıyor.
    CatchingUp,
    /// Oda `hold`; bu istemci beklenen değil, yerinde duruyor.
    HoldWaiting,
    /// Tampon boşaldı.
    Stalled,
    /// Stall sonrası rezerv doluyor (< 12 s).
    Refilling,
    /// Rezerv doldu / yetişti, `ready` gönderildi; oda release bekleniyor.
    Ready,
    /// Oda bu istemciyi beklemeden devam ediyor; "Odayla Senkronla" bekleniyor.
    Detached,
}

/// Motorun dışarıya söylediği şeyler.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineEvent {
    /// Sunucuya gönderilecek mesaj.
    Send(ClientMsg),
    /// Yerel durum değişti (UI rozeti, log).
    StateChanged { from: LocalState, to: LocalState },
    /// > 30 s geride kalındı, ileri atlandı ("42 s ileri atladın").
    ForwardSkip { skipped_sec: f64 },
    /// Yerel sayaç "4 dk'da 3 stall" eşiğine ulaştı (bilgi amaçlı; otorite sunucu).
    ChronicStall { stalls: usize },
}

/// Ölçüm sayaçları (Faz 0 kabul kriterleri: düzeltme amaçlı geri seek = 0).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Telemetry {
    /// Düzeltme amacıyla yapılmaya ÇALIŞILAN geri seek'ler. Seek koruması bunları
    /// engeller; sıfırdan büyükse motorda hata var demektir.
    pub backward_correction_seeks_blocked: u64,
    /// Bilinçli scrub nedeniyle yapılan geri seek'ler (serbest).
    pub scrub_backward_seeks: u64,
    /// İleri seek'ler (countdown hizalama, > 30 s atlama, resync).
    pub forward_seeks: u64,
    /// > 30 s geride atlama sayısı.
    pub forward_skips: u64,
    /// Stall başlangıçları.
    pub stalls: u64,
    /// Son ölçülen sapma (s).
    pub last_drift_sec: Option<f64>,
    /// Oynarken ölçülen en büyük |sapma| (s).
    pub max_abs_drift_sec: f64,
    /// Uygulanan en düşük / en yüksek hız.
    pub min_speed_applied: f64,
    pub max_speed_applied: f64,
}

/// Oynatıcı dururken oda oynuyorsa nasıl hizalanacağı.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Align {
    /// Katılma / snapshot / resync: geride → sessiz ileri seek, önde → `AheadHold`.
    Join,
    /// Stall sonrası `Ready`, oda hiç HOLD'a girmedi: < 3 s oyna (kontrolcü düzeltir),
    /// 3–30 s `CatchingUp`, > 30 s ileri atla.
    Resume,
}

/// İstemci senkron motoru.
pub struct SyncEngine {
    cfg: SyncConfig,
    clock: ClockSync,
    controller: SpeedController,
    stall: StallDetector,
    gate: ReserveGate,
    history: StallHistory,
    state: LocalState,
    me: Option<String>,
    room: Option<RoomState>,
    telemetry: Telemetry,
    /// İlk pong'dan önce `joined.serverTs − now` kaba ofseti.
    rough_offset_ms: Option<f64>,
    last_seq: Option<u64>,
    cid_counter: u64,
    pings_sent: usize,
    next_ping_ms: Option<f64>,
    next_report_ms: Option<f64>,
    last_tick_ms: Option<f64>,
    /// Countdown: yerel başlama anı (öndeysek lead kadar gecikmeli).
    planned_start_ms: Option<f64>,
    /// AheadHold: yerel yeniden başlama anı.
    ahead_resume_ms: Option<f64>,
    /// HOLD(catch_up): bu konuma kadar oyna, sonra `ready`.
    catch_up_target: Option<f64>,
    /// Oynatıcıya en son uygulanan hız.
    speed: f64,
}

fn report_state(s: LocalState) -> Option<ReportState> {
    use LocalState as L;
    Some(match s {
        L::Idle => return None,
        L::Paused | L::Countdown | L::HoldWaiting => ReportState::Paused,
        L::Synced | L::Correcting => ReportState::Playing,
        L::AheadHold => ReportState::AheadHold,
        L::CatchingUp => ReportState::CatchingUp,
        L::Stalled => ReportState::Stalled,
        L::Refilling => ReportState::Refilling,
        L::Ready => ReportState::Ready,
        L::Detached => ReportState::Detached,
    })
}

/// Countdown'da `startAt` geçtiyse oda fiilen oynuyor sayılır (`playing` mesajı gecikse bile).
fn countdown_started(st: &RoomState, server_ms: f64) -> bool {
    st.mode == RoomMode::Countdown && st.start_at > 0 && server_ms >= st.start_at as f64
}

fn room_pos_at(st: &RoomState, server_ms: f64) -> f64 {
    if countdown_started(st, server_ms) {
        st.anchor_pos + (server_ms - st.start_at as f64) / 1000.0 * st.rate
    } else {
        st.position_at(server_ms)
    }
}

impl SyncEngine {
    pub fn new(cfg: SyncConfig) -> Self {
        Self {
            clock: ClockSync::default(),
            controller: SpeedController::new(&cfg),
            stall: StallDetector::new(&cfg),
            gate: ReserveGate::new(&cfg),
            history: StallHistory::new(&cfg),
            state: LocalState::Idle,
            me: None,
            room: None,
            telemetry: Telemetry {
                min_speed_applied: 1.0,
                max_speed_applied: 1.0,
                ..Telemetry::default()
            },
            rough_offset_ms: None,
            last_seq: None,
            cid_counter: 0,
            pings_sent: 0,
            next_ping_ms: None,
            next_report_ms: None,
            last_tick_ms: None,
            planned_start_ms: None,
            ahead_resume_ms: None,
            catch_up_target: None,
            speed: 1.0,
            cfg,
        }
    }

    pub fn config(&self) -> &SyncConfig {
        &self.cfg
    }
    /// Müzik (görüntüsüz ses) için geniş ölü bant ve dar hız aralığı
    /// ([`SyncConfig::lounge`]); `false` varsayılana döner. Değişince hız kontrolcüsü
    /// sıfırlanır; diğer eşikler (stall, rezerv) aynı kalır.
    pub fn set_music(&mut self, music: bool) {
        let base = if music {
            SyncConfig::lounge()
        } else {
            SyncConfig::default()
        };
        if self.cfg.deadband_sec == base.deadband_sec && self.cfg.max_speed == base.max_speed {
            return;
        }
        self.cfg.deadband_sec = base.deadband_sec;
        self.cfg.deadband_exit_sec = base.deadband_exit_sec;
        self.cfg.min_speed = base.min_speed;
        self.cfg.max_speed = base.max_speed;
        self.controller = SpeedController::new(&self.cfg);
    }
    pub fn state(&self) -> LocalState {
        self.state
    }
    pub fn me(&self) -> Option<&str> {
        self.me.as_deref()
    }
    pub fn room(&self) -> Option<&RoomState> {
        self.room.as_ref()
    }
    pub fn clock(&self) -> &ClockSync {
        &self.clock
    }
    pub fn telemetry(&self) -> &Telemetry {
        &self.telemetry
    }

    /// Oda konumu (s), yerel `now_local_ms` anında. Oda state'i veya saat ofseti yoksa `None`.
    /// Countdown'da `startAt` geçtiyse konum oradan ilerletilir.
    pub fn room_position(&self, now_local_ms: f64) -> Option<f64> {
        let st = self.room.as_ref()?;
        Some(room_pos_at(st, self.server_now(now_local_ms)?))
    }

    /// Bir sonraki planlı yerel olay (countdown başlangıcı / AheadHold bitişi), yerel ms.
    /// Host bununla `tick()` için hassas bir uyanma zamanlayabilir.
    pub fn next_deadline_ms(&self) -> Option<f64> {
        match self.state {
            LocalState::Countdown => self.planned_start_ms,
            LocalState::AheadHold => self.ahead_resume_ms,
            _ => None,
        }
    }

    /// Sunucudan gelen her mesaj için çağrılır.
    ///
    /// - `pong` → saat örneği (`t1 = now_local_ms`).
    /// - `room_created` / `joined` → `me` ayarlanır; `joined.snapshot.state` uygulanır.
    ///   `joined.serverTs` ofset yokken kaba ilk tahmin olarak kullanılabilir.
    /// - `state` → `seq` ≤ son seq ise yok sayılır; değilse oda moduna göre oynatıcı hizalanır.
    /// - Diğerleri durum değiştirmez.
    pub fn on_server_msg(
        &mut self,
        msg: &ServerMsg,
        player: &mut dyn Player,
        now_local_ms: f64,
    ) -> Vec<EngineEvent> {
        let mut ev = Vec::new();
        match msg {
            ServerMsg::Pong { t0, server_ts } => {
                self.clock
                    .add_sample(*t0 as f64, *server_ts as f64, now_local_ms);
                if self.clock.sample_count() > 0 {
                    self.rough_offset_ms = None;
                }
            }
            ServerMsg::RoomCreated { you, .. } => self.me = Some(you.clone()),
            ServerMsg::Joined {
                you,
                snapshot,
                server_ts,
                ..
            } => {
                self.me = Some(you.clone());
                if self.clock.sample_count() == 0 {
                    self.rough_offset_ms = Some(*server_ts as f64 - now_local_ms);
                }
                self.last_seq = None;
                self.accept_state(&snapshot.state, player, now_local_ms, &mut ev);
            }
            ServerMsg::State(st) => self.accept_state(st, player, now_local_ms, &mut ev),
            ServerMsg::Detached { .. } => ev.extend(self.set_detached(player, now_local_ms)),
            _ => {}
        }
        ev
    }

    /// Periyodik değerlendirme (`cfg.tick_ms`, ör. 250 ms). Daha sık çağrılabilir.
    /// Ping'leri ve raporları da zamanı gelince üretir.
    pub fn tick(&mut self, player: &mut dyn Player, now_local_ms: f64) -> Vec<EngineEvent> {
        let now = now_local_ms;
        let mut ev = Vec::new();
        let dt = self
            .last_tick_ms
            .map_or(0.0, |t| ((now - t) / 1000.0).max(0.0));
        self.last_tick_ms = Some(now);

        self.tick_ping(now, &mut ev);
        if self.state != LocalState::Detached {
            self.tick_stall(player, now, &mut ev);
            self.tick_timers(player, now, &mut ev);
            self.tick_drift(player, now, dt, &mut ev);
        }
        self.tick_report(player, now, &mut ev);
        ev
    }

    /// Kullanıcı oynat'a bastı → `cmd play`.
    pub fn user_play(&mut self, _player: &dyn Player) -> ClientMsg {
        self.cmd(CmdOp::Play, None)
    }

    /// Kullanıcı durdur'a bastı → `cmd pause` (pos = gerçek time-pos).
    pub fn user_pause(&mut self, player: &dyn Player) -> ClientMsg {
        self.cmd(CmdOp::Pause, Some(player.position()))
    }

    /// Kullanıcı timeline'da bir konuma gitti → `cmd seek`.
    pub fn user_seek(&mut self, pos: f64) -> ClientMsg {
        self.cmd(CmdOp::Seek, Some(pos))
    }

    /// Kullanıcı odaya yeni medya açtı → `cmd media`. Sunucu odayı 0'da duraklatır.
    pub fn user_media(&mut self, media: Media) -> ClientMsg {
        let mut msg = self.cmd(CmdOp::Media, None);
        if let ClientMsg::Cmd(c) = &mut msg {
            c.media = Some(Box::new(media));
        }
        msg
    }

    /// Kullanıcı çalanı kapattı → `cmd close` (oda boşalır).
    pub fn user_close(&mut self) -> ClientMsg {
        self.cmd(CmdOp::Close, None)
    }

    /// Host kronik stall eden üyeyi beklemeden devam etmek istedi → `cmd continue_without`.
    pub fn user_continue_without(&mut self, user: String) -> ClientMsg {
        let mut msg = self.cmd(CmdOp::ContinueWithout, None);
        if let ClientMsg::Cmd(c) = &mut msg {
            c.user = Some(user);
        }
        msg
    }

    /// Host bir üyeyi yardımcı host yapar / bunu geri alır → `cmd set_cohost`.
    pub fn user_set_cohost(&mut self, user: String, on: bool) -> ClientMsg {
        let mut msg = self.cmd(CmdOp::SetCohost, None);
        if let ClientMsg::Cmd(c) = &mut msg {
            c.user = Some(user);
            c.on = Some(on);
        }
        msg
    }

    /// Host oda politikasını değiştirir → `cmd set_policy`.
    pub fn user_set_policy(&mut self, policy: Policy) -> ClientMsg {
        let mut msg = self.cmd(CmdOp::SetPolicy, None);
        if let ClientMsg::Cmd(c) = &mut msg {
            c.policy = Some(policy);
        }
        msg
    }

    /// Sunucu bu istemciyi DETACHED yaptı (`detached` mesajı): oynatıcı durur, oda
    /// takip edilmez; mpv arka planda tamponlamaya devam eder.
    pub fn set_detached(
        &mut self,
        player: &mut dyn Player,
        _now_local_ms: f64,
    ) -> Vec<EngineEvent> {
        let mut ev = Vec::new();
        self.hold_still(player);
        self.clear_plans();
        self.set_state(LocalState::Detached, &mut ev);
        self.push_report(player, None, &mut ev);
        ev
    }

    /// "Odayla Senkronla": DETACHED'dan oda konumuna ileri seek ile dön (sunucuya
    /// `cmd resync`). DETACHED değilken yalnızca yerel yeniden hizalama yapar.
    pub fn user_resync(&mut self, player: &mut dyn Player, now_local_ms: f64) -> Vec<EngineEvent> {
        let mut ev = Vec::new();
        if self.state == LocalState::Detached {
            ev.push(EngineEvent::Send(self.cmd(CmdOp::Resync, None)));
        }
        self.history.clear();
        self.stall.reset();
        // Detached'dayken oynatıcı duraklatılmış: Join hizalaması geride → ileri seek,
        // önde → AheadHold yapar; geri seek asla.
        self.apply_room(player, now_local_ms, &mut ev);
        let reported = ev
            .iter()
            .any(|e| matches!(e, EngineEvent::Send(ClientMsg::Report(_))));
        if !reported {
            self.push_report(player, None, &mut ev);
        }
        ev
    }

    // ------------------------------------------------------------ yardımcılar

    fn offset_ms(&self) -> Option<f64> {
        self.clock.offset_ms().or(self.rough_offset_ms)
    }
    fn server_now(&self, now_local_ms: f64) -> Option<f64> {
        self.offset_ms().map(|o| now_local_ms + o)
    }
    fn to_local(&self, server_ms: f64) -> Option<f64> {
        self.offset_ms().map(|o| server_ms - o)
    }
    fn rate(&self) -> f64 {
        self.room
            .as_ref()
            .map(|r| r.rate)
            .filter(|r| *r > 0.0)
            .unwrap_or(1.0)
    }
    /// Oda fiilen oynuyor mu (`playing` ya da `startAt`'ı geçmiş countdown).
    fn room_running(&self, now: f64) -> bool {
        match (&self.room, self.server_now(now)) {
            (Some(st), Some(s)) => st.mode == RoomMode::Playing || countdown_started(st, s),
            _ => false,
        }
    }
    fn is_waiting_for_me(&self, st: &RoomState) -> bool {
        self.me
            .as_ref()
            .is_some_and(|m| st.waiting_for.iter().any(|w| w == m))
    }

    fn cmd(&mut self, op: CmdOp, pos: Option<f64>) -> ClientMsg {
        self.cid_counter += 1;
        ClientMsg::Cmd(Cmd {
            cid: format!("c-{}", self.cid_counter),
            op,
            pos,
            policy: None,
            media: None,
            user: None,
            on: None,
        })
    }

    fn set_state(&mut self, to: LocalState, ev: &mut Vec<EngineEvent>) {
        if self.state != to {
            ev.push(EngineEvent::StateChanged {
                from: self.state,
                to,
            });
            self.state = to;
        }
    }

    fn clear_plans(&mut self) {
        self.planned_start_ms = None;
        self.ahead_resume_ms = None;
        self.catch_up_target = None;
    }

    fn apply_speed(&mut self, player: &mut dyn Player, v: f64) {
        if (v - self.speed).abs() > 1e-9 {
            player.set_speed(v);
            self.speed = v;
            let t = &mut self.telemetry;
            t.min_speed_applied = t.min_speed_applied.min(v);
            t.max_speed_applied = t.max_speed_applied.max(v);
        }
    }

    fn reset_speed(&mut self, player: &mut dyn Player) {
        self.controller.reset();
        self.apply_speed(player, 1.0);
    }

    /// Tek seek kapısı. Scrub değilse geri seek ENGELLENİR (demir kural).
    fn seek(&mut self, player: &mut dyn Player, target: f64, scrub: bool) -> bool {
        let backward = target < player.position() - 0.001;
        if backward && !scrub {
            self.telemetry.backward_correction_seeks_blocked += 1;
            return false;
        }
        player.seek_exact(target);
        if backward {
            self.telemetry.scrub_backward_seeks += 1;
        } else {
            self.telemetry.forward_seeks += 1;
        }
        self.stall.reset();
        self.reset_speed(player);
        true
    }

    fn resume(&mut self, player: &mut dyn Player) {
        player.set_pause(false);
        // Duraklamadaki konum "ilerleme yok" sanılmasın.
        self.stall.reset();
    }

    fn hold_still(&mut self, player: &mut dyn Player) {
        player.set_pause(true);
        self.reset_speed(player);
    }

    fn report(&self, player: &dyn Player, skipped_sec: Option<f64>) -> Option<EngineEvent> {
        let state = report_state(self.state)?;
        let pos = player.position();
        let cache = player.cache_ahead();
        Some(EngineEvent::Send(ClientMsg::Report(Report {
            pos,
            state,
            buffer_sec: cache,
            buffer_pct: self.gate.percent(pos, cache, player.duration()),
            skipped_sec,
        })))
    }

    fn push_report(&self, player: &dyn Player, skipped: Option<f64>, ev: &mut Vec<EngineEvent>) {
        if self.me.is_some() {
            ev.extend(self.report(player, skipped));
        }
    }

    // ------------------------------------------------------------ oda state'i

    fn accept_state(
        &mut self,
        st: &RoomState,
        player: &mut dyn Player,
        now: f64,
        ev: &mut Vec<EngineEvent>,
    ) {
        if self.last_seq.is_some_and(|l| st.seq <= l) {
            return;
        }
        self.last_seq = Some(st.seq);
        self.room = Some(st.clone());
        // Yeni medya herkesi baştan başlatır (sunucu DETACHED'ı da kaldırır).
        if self.state != LocalState::Detached || st.cause == "media" {
            self.apply_room(player, now, ev);
        }
    }

    fn apply_room(&mut self, player: &mut dyn Player, now: f64, ev: &mut Vec<EngineEvent>) {
        let Some(st) = self.room.clone() else { return };
        // countdown → playing(start): planlı (belki gecikmeli) başlangıç korunur.
        if st.mode == RoomMode::Playing
            && st.cause == "start"
            && self.state == LocalState::Countdown
            && self.planned_start_ms.is_some()
        {
            return;
        }
        self.clear_plans();
        match st.mode {
            RoomMode::Paused => self.apply_paused(&st, player, ev),
            RoomMode::Countdown => self.apply_countdown(&st, player, now, ev),
            RoomMode::Playing => self.apply_playing(&st, player, now, ev),
            RoomMode::Hold => self.apply_hold(&st, player, ev),
        }
    }

    fn apply_paused(&mut self, st: &RoomState, player: &mut dyn Player, ev: &mut Vec<EngineEvent>) {
        self.hold_still(player);
        if st.cause == "seek" {
            self.seek(player, st.anchor_pos, true);
        } else if st.anchor_pos - player.position() > self.cfg.deadband_sec {
            self.seek(player, st.anchor_pos, false);
        } // öndeysek yerinde dur
        self.set_state(LocalState::Paused, ev);
    }

    fn apply_countdown(
        &mut self,
        st: &RoomState,
        player: &mut dyn Player,
        now: f64,
        ev: &mut Vec<EngineEvent>,
    ) {
        self.hold_still(player);
        let pos = player.position();
        let mut lead = 0.0;
        if st.cause == "seek" {
            self.seek(player, st.anchor_pos, true);
        } else if st.anchor_pos - pos > self.cfg.deadband_sec {
            self.seek(player, st.anchor_pos, false);
        } else {
            // Öndeyiz: geri seek yok, başlangıcı lead kadar geciktir (yerel hold).
            lead = (pos - st.anchor_pos).max(0.0);
        }
        if st.start_at > 0 {
            let start = self.to_local(st.start_at as f64).unwrap_or(now);
            self.planned_start_ms = Some(start + lead * 1000.0 / self.rate());
        }
        self.set_state(LocalState::Countdown, ev);
    }

    fn apply_playing(
        &mut self,
        st: &RoomState,
        player: &mut dyn Player,
        now: f64,
        ev: &mut Vec<EngineEvent>,
    ) {
        use LocalState as L;
        if st.cause == "seek" {
            // Bilinçli scrub: her yöne serbest.
            if let Some(rp) = self.room_position(now) {
                self.seek(player, rp, true);
            }
            self.resume(player);
            self.set_state(L::Synced, ev);
            return;
        }
        match self.state {
            L::Synced | L::Correcting | L::CatchingUp if !player.is_paused() => {} // tick düzeltir
            L::Stalled | L::Refilling => {} // rezerv bekleniyor
            L::Ready => self.align(player, now, Align::Resume, ev),
            _ => self.align(player, now, Align::Join, ev),
        }
    }

    fn apply_hold(&mut self, st: &RoomState, player: &mut dyn Player, ev: &mut Vec<EngineEvent>) {
        use LocalState as L;
        if !self.is_waiting_for_me(st) {
            self.hold_still(player); // yerinde dur, geri seek yok
            self.set_state(L::HoldWaiting, ev);
            return;
        }
        if matches!(self.state, L::Stalled | L::Refilling) {
            return; // tick rezervi izliyor
        }
        if st.reason.as_deref() == Some("catch_up") {
            self.catch_up_target = Some(st.hold_pos.unwrap_or(st.anchor_pos));
            self.resume(player);
            self.apply_speed(player, self.cfg.max_speed);
            self.set_state(L::CatchingUp, ev);
            self.check_catch_up_done(player, ev);
            return;
        }
        self.hold_still(player);
        let (pos, cache) = (player.position(), player.cache_ahead());
        if self.gate.is_satisfied(pos, cache, player.duration()) {
            self.set_state(L::Ready, ev);
            self.push_report(player, None, ev);
        } else {
            self.set_state(L::Refilling, ev);
        }
    }

    /// Oynatıcı duruyor, oda oynuyor: moda göre hizalan.
    fn align(&mut self, player: &mut dyn Player, now: f64, mode: Align, ev: &mut Vec<EngineEvent>) {
        let Some(rp) = self.room_position(now) else {
            return;
        };
        let c = &self.cfg;
        let (deadband, hard, skip) = (c.deadband_sec, c.hard_desync_sec, c.skip_forward_sec);
        let drift = player.position() - rp;
        let ahead_limit = if mode == Align::Join { deadband } else { hard };
        if drift > ahead_limit {
            self.enter_ahead_hold(player, now, drift, ev);
            return;
        }
        match mode {
            Align::Join if drift < -deadband => {
                self.seek(player, rp, false);
            }
            Align::Resume if drift < -skip => {
                self.forward_skip(player, rp, drift, ev);
                return;
            }
            Align::Resume if drift < -hard => {
                self.resume(player);
                self.enter_catching_up(player, ev);
                return;
            }
            _ => {}
        }
        self.resume(player);
        self.set_state(LocalState::Synced, ev);
    }

    fn enter_ahead_hold(
        &mut self,
        player: &mut dyn Player,
        now: f64,
        drift: f64,
        ev: &mut Vec<EngineEvent>,
    ) {
        self.hold_still(player);
        self.ahead_resume_ms = Some(now + drift * 1000.0 / self.rate());
        if self.state != LocalState::AheadHold {
            self.set_state(LocalState::AheadHold, ev);
            self.push_report(player, None, ev);
        }
    }

    fn enter_catching_up(&mut self, player: &mut dyn Player, ev: &mut Vec<EngineEvent>) {
        self.apply_speed(player, self.cfg.max_speed);
        if self.state != LocalState::CatchingUp {
            self.set_state(LocalState::CatchingUp, ev);
            self.push_report(player, None, ev);
        }
    }

    /// > 30 s geride: oda konumuna İLERİ seek, bildir.
    fn forward_skip(
        &mut self,
        player: &mut dyn Player,
        room_pos: f64,
        drift: f64,
        ev: &mut Vec<EngineEvent>,
    ) {
        let skipped = -drift;
        if !self.seek(player, room_pos, false) {
            return;
        }
        self.telemetry.forward_skips += 1;
        self.resume(player);
        self.set_state(LocalState::Synced, ev);
        ev.push(EngineEvent::ForwardSkip {
            skipped_sec: skipped,
        });
        self.push_report(player, Some(skipped), ev);
    }

    fn check_catch_up_done(&mut self, player: &mut dyn Player, ev: &mut Vec<EngineEvent>) {
        let Some(target) = self.catch_up_target else {
            return;
        };
        if player.position() >= target - self.cfg.deadband_exit_sec {
            self.catch_up_target = None;
            self.hold_still(player);
            self.set_state(LocalState::Ready, ev);
            self.push_report(player, None, ev);
        }
    }

    // ------------------------------------------------------------ tick adımları

    fn tick_ping(&mut self, now: f64, ev: &mut Vec<EngineEvent>) {
        if self.me.is_none() || self.next_ping_ms.is_some_and(|t| now < t) {
            return;
        }
        let rtt = self.clock.rtt_ms().map(|r| r.round() as i64);
        ev.push(EngineEvent::Send(ClientMsg::Ping {
            t0: now as i64,
            rtt,
        }));
        self.pings_sent += 1;
        let iv = if self.pings_sent < self.cfg.warmup_pings {
            self.cfg.warmup_ping_interval_ms
        } else {
            self.cfg.ping_interval_ms
        };
        self.next_ping_ms = Some(now + iv);
    }

    fn tick_report(&mut self, player: &dyn Player, now: f64, ev: &mut Vec<EngineEvent>) {
        if self.me.is_none() || self.next_report_ms.is_some_and(|t| now < t) {
            return;
        }
        ev.extend(self.report(player, None));
        self.next_report_ms = Some(now + self.cfg.report_interval_ms);
    }

    fn tick_stall(&mut self, player: &mut dyn Player, now: f64, ev: &mut Vec<EngineEvent>) {
        use LocalState as L;
        let playing_state = matches!(self.state, L::Synced | L::Correcting | L::CatchingUp);
        let (pos, cache) = (player.position(), player.cache_ahead());
        let stalled = self.stall.update(
            pos,
            cache,
            player.paused_for_cache(),
            player.eof_reached(),
            playing_state,
        );
        if stalled && playing_state {
            self.start_stall(player, now, ev);
        }
        if self.state == L::Stalled && cache > self.cfg.stall_low_water_sec {
            self.set_state(L::Refilling, ev);
        }
        if matches!(self.state, L::Stalled | L::Refilling)
            && self.gate.is_satisfied(pos, cache, player.duration())
        {
            self.set_state(L::Ready, ev);
            self.push_report(player, None, ev);
        }
    }

    fn start_stall(&mut self, player: &mut dyn Player, now: f64, ev: &mut Vec<EngineEvent>) {
        // Histerezis: ilk pakette devam etme; rezerv dolana kadar duraklat.
        self.hold_still(player);
        self.clear_plans();
        self.set_state(LocalState::Stalled, ev);
        self.telemetry.stalls += 1;
        let now_sec = now / 1000.0;
        if self.history.record(now_sec) {
            ev.push(EngineEvent::ChronicStall {
                stalls: self.history.count_in_window(now_sec),
            });
        }
        self.push_report(player, None, ev);
    }

    fn tick_timers(&mut self, player: &mut dyn Player, now: f64, ev: &mut Vec<EngineEvent>) {
        match self.state {
            LocalState::Countdown => {
                if self.planned_start_ms.is_some_and(|t| now >= t) {
                    self.planned_start_ms = None;
                    self.reset_speed(player);
                    self.resume(player);
                    self.set_state(LocalState::Synced, ev);
                }
            }
            LocalState::AheadHold => self.tick_ahead_hold(player, now, ev),
            LocalState::Ready => {
                // Sunucu hiç HOLD'a almadıysa (oda oynuyor) kendimiz dönelim.
                if self
                    .room
                    .as_ref()
                    .is_some_and(|r| r.mode == RoomMode::Playing)
                {
                    self.align(player, now, Align::Resume, ev);
                }
            }
            LocalState::CatchingUp => self.check_catch_up_done(player, ev),
            _ => {}
        }
    }

    fn tick_ahead_hold(&mut self, player: &mut dyn Player, now: f64, ev: &mut Vec<EngineEvent>) {
        if !self.room_running(now) {
            return;
        }
        // Saat ofseti iyileştikçe bitiş anını tazele.
        if let Some(rp) = self.room_position(now) {
            let drift = player.position() - rp;
            self.ahead_resume_ms = Some(now + drift * 1000.0 / self.rate());
        }
        if self.ahead_resume_ms.is_none_or(|t| now >= t - 1.0) {
            self.ahead_resume_ms = None;
            self.reset_speed(player);
            self.resume(player);
            self.set_state(LocalState::Synced, ev);
        }
    }

    fn tick_drift(
        &mut self,
        player: &mut dyn Player,
        now: f64,
        dt: f64,
        ev: &mut Vec<EngineEvent>,
    ) {
        use LocalState as L;
        if !matches!(self.state, L::Synced | L::Correcting | L::CatchingUp)
            || self.catch_up_target.is_some()
            || player.eof_reached()
            || !self.room_running(now)
        {
            return;
        }
        let Some(rp) = self.room_position(now) else {
            return;
        };
        let drift = player.position() - rp;
        self.telemetry.last_drift_sec = Some(drift);
        self.telemetry.max_abs_drift_sec = self.telemetry.max_abs_drift_sec.max(drift.abs());

        let c = &self.cfg;
        if drift > c.hard_desync_sec {
            self.enter_ahead_hold(player, now, drift, ev);
        } else if drift < -c.skip_forward_sec {
            self.forward_skip(player, rp, drift, ev);
        } else if drift < -c.hard_desync_sec {
            // Kontrolcüyü ısıt: 3 s içine girince hız 1.04'ten yumuşakça iner.
            self.controller.update(drift, dt);
            self.enter_catching_up(player, ev);
        } else {
            let v = self.controller.update(drift, dt);
            self.apply_speed(player, v);
            let next = if self.controller.is_correcting() {
                L::Correcting
            } else {
                L::Synced
            };
            self.set_state(next, ev);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Policy, Snapshot};

    /// Sunucu saati = yerel + OFF.
    const OFF: f64 = 1.7e12;

    /// Minimal oynatıcı: duraklatılmamışsa ve `frozen` değilse `speed` ile ilerler.
    struct FakePlayer {
        pos: f64,
        paused: bool,
        speed: f64,
        cache: f64,
        frozen: bool,
        seeks: Vec<f64>,
    }

    impl FakePlayer {
        fn new(pos: f64) -> Self {
            Self {
                pos,
                paused: true,
                speed: 1.0,
                cache: 60.0,
                frozen: false,
                seeks: vec![],
            }
        }
        fn advance(&mut self, ms: f64) {
            if !self.paused && !self.frozen {
                self.pos += ms / 1000.0 * self.speed;
            }
        }
    }

    impl Player for FakePlayer {
        fn position(&self) -> f64 {
            self.pos
        }
        fn is_paused(&self) -> bool {
            self.paused
        }
        fn cache_ahead(&self) -> f64 {
            self.cache
        }
        fn paused_for_cache(&self) -> bool {
            false
        }
        fn duration(&self) -> Option<f64> {
            Some(7200.0)
        }
        fn eof_reached(&self) -> bool {
            false
        }
        fn set_pause(&mut self, paused: bool) {
            self.paused = paused;
        }
        fn set_speed(&mut self, speed: f64) {
            self.speed = speed;
        }
        fn seek_exact(&mut self, pos: f64) {
            self.pos = pos;
            self.seeks.push(pos);
        }
    }

    fn srv(local_ms: f64) -> i64 {
        (local_ms + OFF) as i64
    }

    fn room(
        seq: u64,
        mode: RoomMode,
        anchor_pos: f64,
        anchor_local_ms: f64,
        cause: &str,
    ) -> RoomState {
        RoomState {
            seq,
            mode,
            anchor_pos,
            anchor_ts: srv(anchor_local_ms),
            start_at: 0,
            rate: 1.0,
            by: None,
            cause: cause.into(),
            ack: None,
            server_ts: None,
            hold_pos: None,
            waiting_for: vec![],
            reason: None,
            media: None,
        }
    }

    struct H {
        e: SyncEngine,
        p: FakePlayer,
        now: f64,
        ev: Vec<EngineEvent>,
    }

    impl H {
        /// `joined` (snapshot = `state`) + kesin ofsetli bir pong, t = 0.
        fn new(pos: f64, state: RoomState) -> Self {
            let mut h = H {
                e: SyncEngine::new(SyncConfig::default()),
                p: FakePlayer::new(pos),
                now: 0.0,
                ev: vec![],
            };
            let joined = ServerMsg::Joined {
                you: "u2".into(),
                invite: "inv".into(),
                snapshot: Box::new(Snapshot {
                    room: "r".into(),
                    name: "n".into(),
                    policy: Policy::Free,
                    view: String::new(),
                    host: "u1".into(),
                    members: vec![],
                    state,
                    downloads: vec![],
                }),
                server_ts: srv(0.0),
                resume: None,
                resumed: false,
            };
            h.recv(joined);
            h.recv(ServerMsg::Pong {
                t0: 0,
                server_ts: srv(0.0),
            });
            assert_eq!(h.e.clock().offset_ms(), Some(OFF));
            h
        }
        fn recv(&mut self, m: ServerMsg) {
            let ev = self.e.on_server_msg(&m, &mut self.p, self.now);
            self.ev.extend(ev);
        }
        fn state(&mut self, st: RoomState) {
            self.recv(ServerMsg::State(st));
        }
        fn run(&mut self, ms: f64) {
            let end = self.now + ms;
            while self.now < end - 1e-9 {
                self.now += 50.0;
                self.p.advance(50.0);
                let ev = self.e.tick(&mut self.p, self.now);
                self.ev.extend(ev);
            }
        }
        fn take(&mut self) -> Vec<EngineEvent> {
            std::mem::take(&mut self.ev)
        }
        fn drift(&self) -> f64 {
            self.p.pos - self.e.room_position(self.now).unwrap()
        }
        fn assert_clean(&self) {
            assert_eq!(self.e.telemetry().backward_correction_seeks_blocked, 0);
        }
        /// Odaya katıl: oda 100 s'de oynuyor, oynatıcı 0'da duruyor.
        fn joined_playing() -> Self {
            let mut h = H::new(0.0, room(1, RoomMode::Playing, 100.0, 0.0, "snapshot"));
            assert_eq!(h.e.state(), LocalState::Synced);
            assert_eq!(h.p.seeks, vec![100.0]);
            assert!(!h.p.paused);
            let ev = h.take();
            assert!(!ev
                .iter()
                .any(|e| matches!(e, EngineEvent::ForwardSkip { .. })));
            h.run(1000.0);
            assert!(h.drift().abs() < 1e-6);
            h.take();
            h
        }
    }

    fn reports(ev: &[EngineEvent]) -> Vec<&Report> {
        ev.iter()
            .filter_map(|e| match e {
                EngineEvent::Send(ClientMsg::Report(r)) => Some(r),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn seq_guard_ignores_stale_state() {
        let mut h = H::new(10.0, room(5, RoomMode::Paused, 10.0, 0.0, "snapshot"));
        assert_eq!(h.e.state(), LocalState::Paused);
        h.state(room(5, RoomMode::Paused, 50.0, 0.0, "seek"));
        h.state(room(4, RoomMode::Paused, 60.0, 0.0, "seek"));
        assert!(h.p.seeks.is_empty());
        h.state(room(6, RoomMode::Paused, 50.0, 0.0, "seek"));
        assert_eq!(h.p.seeks, vec![50.0]);
        h.assert_clean();
    }

    #[test]
    fn scrub_allows_backward_seek() {
        let mut h = H::new(100.0, room(1, RoomMode::Paused, 100.0, 0.0, "snapshot"));
        h.state(room(2, RoomMode::Paused, 20.0, 0.0, "seek"));
        assert_eq!(h.p.pos, 20.0);
        let mut cd = room(3, RoomMode::Countdown, 5.0, 0.0, "seek");
        cd.start_at = srv(500.0);
        h.state(cd);
        assert_eq!(h.p.pos, 5.0);
        assert_eq!(h.e.telemetry().scrub_backward_seeks, 2);
        assert_eq!(h.e.next_deadline_ms(), Some(500.0));
        h.assert_clean();
    }

    #[test]
    fn paused_ahead_stays_in_place() {
        let h = H::new(105.0, room(1, RoomMode::Paused, 100.0, 0.0, "pause"));
        assert_eq!(h.e.state(), LocalState::Paused);
        assert!(h.p.paused);
        assert_eq!(h.p.pos, 105.0);
        assert!(h.p.seeks.is_empty());
        h.assert_clean();
    }

    #[test]
    fn countdown_ahead_delays_start_by_lead() {
        let mut h = H::new(100.5, room(1, RoomMode::Paused, 100.0, 0.0, "pause"));
        let mut cd = room(2, RoomMode::Countdown, 100.0, 0.0, "play");
        cd.start_at = srv(1000.0);
        h.state(cd);
        assert_eq!(h.e.state(), LocalState::Countdown);
        assert_eq!(h.e.next_deadline_ms(), Some(1500.0));
        h.run(1000.0);
        h.state(room(3, RoomMode::Playing, 100.0, 1000.0, "start"));
        assert_eq!(h.e.next_deadline_ms(), Some(1500.0), "plan korunmalı");
        h.run(450.0);
        assert_eq!(h.e.state(), LocalState::Countdown);
        assert!(h.p.paused);
        h.run(50.0);
        assert_eq!(h.e.state(), LocalState::Synced);
        assert!(!h.p.paused);
        h.run(2000.0);
        assert!(h.drift().abs() < 0.01, "drift {}", h.drift());
        assert!(h.p.seeks.is_empty());
        h.assert_clean();
    }

    #[test]
    fn far_ahead_holds_then_resumes_near_room() {
        let mut h = H::joined_playing();
        h.p.pos += 5.0;
        h.run(250.0);
        assert_eq!(h.e.state(), LocalState::AheadHold);
        assert!(h.p.paused);
        let ev = h.take();
        assert!(reports(&ev)
            .iter()
            .any(|r| r.state == ReportState::AheadHold));
        assert!(h.e.next_deadline_ms().is_some());
        let mut guard = 0;
        while h.e.state() == LocalState::AheadHold && guard < 400 {
            h.run(50.0);
            guard += 1;
        }
        assert_eq!(h.e.state(), LocalState::Synced);
        assert!(h.drift().abs() < 0.06, "drift {}", h.drift());
        assert_eq!(h.p.seeks.len(), 1);
        h.assert_clean();
    }

    #[test]
    fn far_behind_skips_forward() {
        let mut h = H::joined_playing();
        h.p.pos -= 40.0;
        h.run(250.0);
        let ev = h.take();
        let skipped = ev
            .iter()
            .find_map(|e| match e {
                EngineEvent::ForwardSkip { skipped_sec } => Some(*skipped_sec),
                _ => None,
            })
            .expect("ForwardSkip");
        assert!((skipped - 40.0).abs() < 0.01);
        assert!(reports(&ev)
            .iter()
            .any(|r| r.skipped_sec.is_some_and(|s| (s - 40.0).abs() < 0.01)));
        assert_eq!(h.e.telemetry().forward_skips, 1);
        assert!(h.drift().abs() < 1e-6);
        assert_eq!(h.e.state(), LocalState::Synced);
        h.assert_clean();
    }

    #[test]
    fn moderately_behind_catches_up_without_seek() {
        let mut h = H::joined_playing();
        h.p.pos -= 10.0;
        h.run(250.0);
        assert_eq!(h.e.state(), LocalState::CatchingUp);
        assert_eq!(h.p.speed, 1.04);
        let ev = h.take();
        assert!(reports(&ev)
            .iter()
            .any(|r| r.state == ReportState::CatchingUp));
        h.run(10_000.0);
        assert_eq!(h.e.state(), LocalState::CatchingUp);
        assert!((h.drift() + 9.6).abs() < 0.05, "drift {}", h.drift());
        assert_eq!(h.p.seeks.len(), 1);
        assert_eq!(h.e.telemetry().max_speed_applied, 1.04);
        h.assert_clean();
    }

    #[test]
    fn small_drift_uses_speed_controller() {
        let mut h = H::joined_playing();
        h.p.pos -= 1.0;
        h.run(250.0);
        assert_eq!(h.e.state(), LocalState::Correcting);
        assert!(h.p.speed > 1.0);
        h.run(120_000.0);
        assert_eq!(h.e.state(), LocalState::Synced);
        assert!(h.drift().abs() < 0.05);
        assert_eq!(h.p.speed, 1.0);
        assert_eq!(h.p.seeks.len(), 1);
        h.assert_clean();
    }

    #[test]
    fn stall_refills_to_reserve_before_ready() {
        let mut h = H::joined_playing();
        h.p.cache = 0.2;
        h.p.frozen = true;
        h.run(500.0);
        assert_eq!(h.e.state(), LocalState::Stalled);
        assert!(h.p.paused);
        assert_eq!(h.e.telemetry().stalls, 1);
        let ev = h.take();
        assert!(reports(&ev).iter().any(|r| r.state == ReportState::Stalled));

        h.p.cache = 5.0;
        h.run(250.0);
        assert_eq!(h.e.state(), LocalState::Refilling);
        h.p.cache = 11.9;
        h.run(250.0);
        assert_eq!(h.e.state(), LocalState::Refilling);
        assert!(h.p.paused, "ilk paketle devam yok");
        h.take();

        h.p.frozen = false;
        h.p.cache = 12.0;
        h.run(50.0);
        let ev = h.take();
        assert!(reports(&ev)
            .iter()
            .any(|r| r.state == ReportState::Ready && r.buffer_pct == 100));
        // Oda HOLD'a girmedi → geride < 3 s: oyna, kontrolcü düzeltsin (seek yok).
        assert_eq!(h.e.state(), LocalState::Correcting);
        assert!(h.p.speed > 1.0);
        assert!(!h.p.paused);
        assert_eq!(h.p.seeks.len(), 1);
        h.assert_clean();
    }

    #[test]
    fn hold_waiting_for_me_reports_ready_when_gate_ok() {
        let mut h = H::joined_playing();
        let mut hold = room(2, RoomMode::Hold, h.p.pos, h.now, "stall");
        hold.waiting_for = vec!["u2".into()];
        hold.reason = Some("stall".into());
        h.state(hold);
        assert_eq!(h.e.state(), LocalState::Ready);
        assert!(h.p.paused);
        let ev = h.take();
        assert!(reports(&ev).iter().any(|r| r.state == ReportState::Ready));

        let mut other = room(3, RoomMode::Hold, h.p.pos, h.now, "stall");
        other.waiting_for = vec!["u3".into()];
        h.state(other);
        assert_eq!(h.e.state(), LocalState::HoldWaiting);
        h.assert_clean();
    }

    #[test]
    fn ping_schedule_warmup_then_steady() {
        let mut h = H::new(0.0, room(1, RoomMode::Paused, 0.0, 0.0, "snapshot"));
        h.run(8000.0);
        let pings: Vec<i64> = h
            .take()
            .iter()
            .filter_map(|e| match e {
                EngineEvent::Send(ClientMsg::Ping { t0, .. }) => Some(*t0),
                _ => None,
            })
            .collect();
        assert_eq!(pings, vec![50, 350, 650, 950, 1250, 4250, 7250]);
        h.assert_clean();
    }

    #[test]
    fn detach_and_resync_forward() {
        let mut h = H::joined_playing();
        h.e.set_detached(&mut h.p, h.now);
        assert_eq!(h.e.state(), LocalState::Detached);
        h.run(20_000.0);
        assert!(h.p.paused);
        assert!(h.drift() < -19.0);
        let ev = h.e.user_resync(&mut h.p, h.now);
        assert_eq!(h.e.state(), LocalState::Synced);
        assert!(h.drift().abs() < 1e-6);
        assert!(!reports(&ev).is_empty());
        h.assert_clean();
    }

    #[test]
    fn user_cmds_increment_cid() {
        let mut e = SyncEngine::new(SyncConfig::default());
        let p = FakePlayer::new(12.5);
        let ClientMsg::Cmd(a) = e.user_play(&p) else {
            panic!()
        };
        let ClientMsg::Cmd(b) = e.user_pause(&p) else {
            panic!()
        };
        let ClientMsg::Cmd(c) = e.user_seek(30.0) else {
            panic!()
        };
        assert_eq!((a.cid.as_str(), a.op, a.pos), ("c-1", CmdOp::Play, None));
        assert_eq!(
            (b.cid.as_str(), b.op, b.pos),
            ("c-2", CmdOp::Pause, Some(12.5))
        );
        assert_eq!(
            (c.cid.as_str(), c.op, c.pos),
            ("c-3", CmdOp::Seek, Some(30.0))
        );
    }
}
