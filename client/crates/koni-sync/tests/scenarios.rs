//! Uçtan uca senaryolar: gerçek `SyncEngine` + `SimPlayer` + `SimRoom` + gecikmeli ağ.

use koni_sync::engine::{EngineEvent, LocalState};
use koni_sync::protocol::{ReportState, RoomMode};
use koni_sync::sim::{ClientSetup, Harness};
use koni_sync::{Player, SyncConfig};

const S: i64 = 1000;

fn start_playing(h: &mut Harness) {
    h.user_play(0);
    assert!(
        h.run_until(10 * S, |h| h.room_mode() == RoomMode::Playing),
        "oda PLAYING olmadı"
    );
    // İstemciler senkrona otursun.
    h.run_ms(20 * S);
}

fn invariants(h: &Harness) {
    if let Err(e) = h.check_invariants() {
        panic!("değişmez ihlali: {e}");
    }
}

fn is_synced(s: LocalState) -> bool {
    matches!(s, LocalState::Synced | LocalState::Correcting)
}

#[test]
fn a_steady_state_two_clients_ten_minutes() {
    let mut h = Harness::two_default(1);
    start_playing(&mut h);
    h.run_ms(40 * S);
    h.clear_drift();
    h.run_ms(600 * S);
    invariants(&h);
    let st = h.drift_stats(0, 1).expect("örnek yok");
    println!(
        "[a] n={} median={:.1} ms p95={:.1} ms max={:.1} ms",
        st.n,
        st.median * 1e3,
        st.p95 * 1e3,
        st.max * 1e3
    );
    assert!(st.n > 3000, "az örnek: {}", st.n);
    assert!(st.median < 0.050, "median {}", st.median);
    assert!(st.p95 < 0.100, "p95 {}", st.p95);
}

#[test]
fn b_one_second_behind_converges_by_speed_only() {
    let mut h = Harness::two_default(2);
    start_playing(&mut h);
    let seeks_before = h.player(1).seeks().len();
    h.nudge(1, -1.0);
    assert!(
        h.run_until(180 * S, |h| h.drift_vs_room(1).abs() < 0.060
            && h.state(1) == LocalState::Synced),
        "yakınsamadı: sapma {}",
        h.drift_vs_room(1)
    );
    println!(
        "[b] yakınsadı t={} ms, sapma {:.3} s",
        h.now_ms,
        h.drift_vs_room(1)
    );
    assert_eq!(
        h.player(1).seeks().len(),
        seeks_before,
        "seek yapıldı: {:?}",
        h.player(1).seeks()
    );
    assert!(h.player(1).speed_range().1 > 1.0, "hız hiç artmadı");
    invariants(&h);
}

#[test]
fn c_four_seconds_ahead_ahead_hold() {
    let mut h = Harness::two_default(3);
    start_playing(&mut h);
    let t0 = h.now_ms;
    let seeks_before = h.player(1).seeks().len();
    h.nudge(1, 4.0);
    assert!(
        h.run_until(5 * S, |h| h.state(1) == LocalState::AheadHold),
        "AheadHold'a girmedi"
    );
    assert!(
        h.player(1).is_paused(),
        "AheadHold'da oynatıcı duraklatılmalı"
    );
    assert!(
        h.run_until(15 * S, |h| h.state(1) != LocalState::AheadHold),
        "AheadHold'dan çıkmadı"
    );
    let d = h.drift_vs_room(1);
    println!("[c] AheadHold çıkışında sapma {:.0} ms", d * 1e3);
    assert!(d.abs() < 0.30, "AheadHold çıkışında sapma {d}");
    assert!(!h.player(1).is_paused(), "oynatıcı devam etmedi");
    assert!(h.entered_state(1, LocalState::AheadHold, t0));
    h.run_ms(60 * S);
    assert!(
        h.drift_vs_room(1).abs() < 0.10,
        "sonra sapma {}",
        h.drift_vs_room(1)
    );
    assert_eq!(h.player(1).seeks().len(), seeks_before, "seek yapıldı");
    invariants(&h);
}

#[test]
fn d_ten_seconds_behind_catching_up() {
    let mut h = Harness::two_default(4);
    start_playing(&mut h);
    let t0 = h.now_ms;
    let seeks_before = h.player(1).seeks().len();
    h.nudge(1, -10.0);
    assert!(
        h.run_until(5 * S, |h| h.state(1) == LocalState::CatchingUp),
        "CatchingUp'a girmedi"
    );
    assert!(h.entered_state(1, LocalState::CatchingUp, t0));
    let d0 = h.drift_vs_room(1).abs();
    h.run_ms(60 * S);
    let d1 = h.drift_vs_room(1).abs();
    println!(
        "[d] sapma {d0:.2} s -> {d1:.2} s (60 sn sonra), durum {:?}",
        h.state(1)
    );
    assert!(d1 < d0 - 1.5, "yetişme yok: {d0} -> {d1}");
    assert_eq!(h.player(1).seeks().len(), seeks_before, "seek yapıldı");
    assert!(h.player(1).speed() > 1.0);
    invariants(&h);
}

#[test]
fn e_forty_five_seconds_behind_forward_skip() {
    let mut h = Harness::two_default(5);
    start_playing(&mut h);
    h.run_ms(30 * S);
    let t0 = h.now_ms;
    h.nudge(1, -45.0);
    assert!(
        h.run_until(15 * S, |h| h.events_of(1).any(
            |e| e.t_ms >= t0 && matches!(e.event, EngineEvent::ForwardSkip { .. })
        )),
        "ForwardSkip yok"
    );
    let skipped = h
        .events_of(1)
        .find_map(|e| match e.event {
            EngineEvent::ForwardSkip { skipped_sec } if e.t_ms >= t0 => Some(skipped_sec),
            _ => None,
        })
        .unwrap();
    println!("[e] ForwardSkip skipped_sec = {skipped:.2}");
    assert!((skipped - 45.0).abs() < 3.0, "skipped {skipped}");
    h.run_ms(5 * S);
    let rep = h
        .reports_of(1)
        .filter(|(s, _)| s.t_ms >= t0)
        .find_map(|(_, r)| r.skipped_sec)
        .expect("skippedSec'li report yok");
    assert!((rep - 45.0).abs() < 3.0, "report.skippedSec {rep}");
    assert!(h.telemetry(1).forward_skips >= 1);
    h.run_ms(60 * S);
    println!(
        "[e] 60 sn sonra sapma {:.0} ms, durum {:?}",
        h.drift_vs_room(1) * 1e3,
        h.state(1)
    );
    invariants(&h);
}

#[test]
fn f_stall_hold_reserve_release() {
    let mut h = Harness::two_default(6);
    h.clients[1].player.set_cache_cap(20.0);
    start_playing(&mut h);
    h.run_ms(5 * S);
    let t0 = h.now_ms;
    h.throttle(1, 0.3);
    assert!(
        h.run_until(120 * S, |h| h.state(1) == LocalState::Stalled),
        "Stalled olmadı"
    );
    assert!(
        h.run_until(5 * S, |h| h.room_mode() == RoomMode::Hold),
        "oda HOLD olmadı"
    );
    let st = h.room.state().clone();
    assert!(
        st.waiting_for
            .iter()
            .any(|w| Some(w.as_str()) == h.room.member_id(1)),
        "waitingFor: {:?}",
        st.waiting_for
    );
    assert_eq!(st.reason.as_deref(), Some("stall"));
    // Diğer istemci duraklar.
    assert!(
        h.run_until(3 * S, |h| h.player(0).is_paused()),
        "A duraklamadı"
    );
    let hold_pos = st.hold_pos.unwrap();
    println!("[f] HOLD t={} ms, holdPos={hold_pos:.2}", h.now_ms - t0);
    // Ağ geri geliyor, ama yavaşça: rezerv dolana dek ready gitmemeli.
    h.run_ms(5 * S);
    h.throttle(1, 3.0);
    let t_restore = h.now_ms;
    assert!(
        h.run_until(120 * S, |h| h.room_mode() == RoomMode::Countdown),
        "release countdown gelmedi (state {:?}, cache {:.1})",
        h.state(1),
        h.player(1).cache_ahead()
    );
    assert_eq!(h.room.state().cause, "release");
    let ready = h
        .reports_of(1)
        .filter(|(s, _)| s.t_ms >= t0)
        .find(|(_, r)| r.state == ReportState::Ready)
        .expect("ready raporu yok");
    println!(
        "[f] ready anında cache_ahead = {:.2} s, restore'dan {} ms sonra",
        ready.0.cache_ahead,
        ready.0.t_ms - t_restore
    );
    assert!(
        ready.0.cache_ahead >= 12.0 - 1e-6,
        "ready cache {}",
        ready.0.cache_ahead
    );
    // İkisi de oynuyor; ping-pong yok.
    assert!(h.run_until(5 * S, |h| h.room_mode() == RoomMode::Playing));
    let t_play = h.now_ms;
    let stalls_before = h.telemetry(1).stalls;
    h.run_ms(40 * S);
    assert_eq!(h.telemetry(1).stalls, stalls_before, "ikinci stall");
    assert_eq!(h.count_entered(1, LocalState::Stalled, t_play), 0);
    assert_eq!(h.room_mode(), RoomMode::Playing);
    assert!(
        is_synced(h.state(0)) && is_synced(h.state(1)),
        "durumlar {:?} {:?}",
        h.state(0),
        h.state(1)
    );
    assert!(!h.player(0).is_paused() && !h.player(1).is_paused());
    let d = (h.pos(0) - h.pos(1)).abs();
    println!("[f] release sonrası A-B farkı {:.0} ms", d * 1e3);
    assert!(d < 0.25);
    invariants(&h);
}

#[test]
fn g_three_stalls_in_four_minutes_chronic() {
    let mut h = Harness::two_default(7);
    h.clients[1].player.set_cache_cap(20.0);
    start_playing(&mut h);
    let t0 = h.now_ms;
    for round in 0..3 {
        h.throttle(1, 0.0);
        assert!(
            h.run_until(120 * S, |h| h.state(1) == LocalState::Stalled),
            "tur {round}: stall yok"
        );
        h.run_ms(2 * S);
        h.throttle(1, 5.0);
        assert!(
            h.run_until(60 * S, |h| h.room_mode() == RoomMode::Playing
                && is_synced(h.state(1))),
            "tur {round}: devam yok (durum {:?}, oda {:?})",
            h.state(1),
            h.room_mode()
        );
        println!("[g] tur {round} bitti t={} sn", (h.now_ms - t0) / S);
        // Cache yeniden dolsun.
        h.run_ms(10 * S);
    }
    assert!(
        h.now_ms - t0 < 240 * S,
        "senaryo 4 dk'yı aştı: {} sn",
        (h.now_ms - t0) / S
    );
    assert!(
        h.events_of(1)
            .any(|e| matches!(e.event, EngineEvent::ChronicStall { .. })),
        "ChronicStall olayı yok (stalls={})",
        h.telemetry(1).stalls
    );
    assert_eq!(h.telemetry(1).stalls, 3);
    invariants(&h);
}

#[test]
fn h_deliberate_scrub_backward_allowed() {
    let mut h = Harness::new(
        SyncConfig::default(),
        vec![
            ClientSetup {
                clock_offset_ms: -5000,
                ..Default::default()
            },
            ClientSetup {
                clock_offset_ms: 12345,
                up_ms: 60.0,
                down_ms: 90.0,
                ..Default::default()
            },
        ],
        8,
    );
    start_playing(&mut h);
    h.run_ms(30 * S);
    assert!(h.pos(0) > 40.0);
    h.user_seek(1, 5.0);
    assert!(h.run_until(10 * S, |h| h.room_mode() == RoomMode::Playing
        && h.room.state().cause == "start"
        && h.room.state().anchor_pos < 10.0));
    h.run_ms(10 * S);
    let total_back: usize = (0..2).map(|i| h.player(i).backward_seeks()).sum();
    let scrub: u64 = (0..2).map(|i| h.telemetry(i).scrub_backward_seeks).sum();
    println!(
        "[h] geri seek toplam={total_back} telemetry.scrub={scrub}, pozisyonlar {:.1} {:.1}",
        h.pos(0),
        h.pos(1)
    );
    assert!(total_back >= 1, "geri seek yapılmadı");
    assert!(scrub >= 1, "scrub sayılmadı");
    assert!(h.pos(0) < 30.0 && h.pos(1) < 30.0);
    assert!((h.pos(0) - h.pos(1)).abs() < 0.3);
    invariants(&h);
}
