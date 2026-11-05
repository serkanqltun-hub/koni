//! CI oynatma dumanı: `playback [--lib PATH] [--secs N]`.
//!
//! 1. libmpv'nin kendi kodlayıcısıyla küçük bir test videosu üretir (renk deseni + ton, MP4).
//! 2. Bu dosyayı uygulamadaki gibi render API'yle (CPU tamponuna) N sn (varsayılan 10)
//!    gerçek zamanlı oynatır. Ses çıkışı `null` ve sessiz: hiçbir ses duyulmaz.
//! 3. Konum ilerlemeli, 2 sn'den uzun takılma olmamalı, kareler çizilmeli.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use koni_player::{Player, PlayerEvent, PlayerOptions};

/// Oynatmada izin verilen en uzun takılma (uygulamadaki donma bekçisiyle aynı).
const MAX_STALL: Duration = Duration::from_secs(2);

fn wait_for(player: &Player, want: fn(&PlayerEvent) -> bool, secs: u64) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        match player.wait_event(0.5) {
            Some(PlayerEvent::Shutdown) => return Err("mpv shut down".into()),
            Some(ev) if want(&ev) => return Ok(()),
            Some(PlayerEvent::EndFile) => return Err("EndFile too early".into()),
            _ => {}
        }
    }
    Err("timed out".into())
}

/// Test videosunu üretir; dosya Player bırakılınca tamamlanır.
fn make_video(lib: &Option<PathBuf>, path: &str, secs: u64) -> Result<(), String> {
    let len = secs + 3;
    let src = format!(
        "av://lavfi:testsrc=duration={len}:size=320x240:rate=25[out0];sine=frequency=440:duration={len}[out1]"
    );
    let enc = Player::new(PlayerOptions {
        wid: None,
        lib_path: lib.clone(),
        extra_options: vec![
            ("o".into(), path.into()),
            ("of".into(), "mp4".into()),
            ("ovc".into(), "mpeg4".into()),
            ("ovcopts".into(), "b=400k".into()),
            ("oac".into(), "aac".into()),
            ("keep-open".into(), "no".into()),
        ],
    })
    .map_err(|e| format!("encoder: {e}"))?;
    enc.load(&src).map_err(|e| format!("encode load: {e}"))?;
    wait_for(&enc, |e| matches!(e, PlayerEvent::FileLoaded), 15)
        .map_err(|e| format!("encode start: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if Instant::now() > deadline {
            return Err("encode timed out".into());
        }
        match enc.wait_event(0.5) {
            Some(PlayerEvent::EndFile) | Some(PlayerEvent::Shutdown) => break,
            _ => {}
        }
    }
    drop(enc);
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    println!("test video: {path} ({size} bytes)");
    if size < 1000 {
        return Err("test video is empty".into());
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let mut lib = None;
    let mut secs = 10u64;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--lib" => lib = Some(PathBuf::from(args.next().ok_or("--lib needs a PATH")?)),
            "--secs" => {
                secs = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or("--secs needs a number")?
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }

    let path = std::env::temp_dir().join("koni-ci-test.mp4");
    let path = path.to_string_lossy().into_owned();
    make_video(&lib, &path, secs)?;

    let player = Player::new(PlayerOptions {
        wid: None,
        lib_path: lib,
        extra_options: vec![
            ("vo".into(), "libmpv".into()),
            ("ao".into(), "null".into()),
            ("mute".into(), "yes".into()),
        ],
    })
    .map_err(|e| format!("Player::new: {e}"))?;
    let render = player
        .create_render_sw()
        .map_err(|e| format!("create_render_sw: {e}"))?;
    player.load(&path).map_err(|e| format!("load: {e}"))?;
    wait_for(&player, |e| matches!(e, PlayerEvent::FileLoaded), 10)
        .map_err(|e| format!("FileLoaded: {e}"))?;
    let tracks = player
        .get_property_string("track-list/count")
        .unwrap_or_default();
    println!(
        "file loaded, duration {:?}, tracks {tracks}",
        player.duration()
    );
    player.play().map_err(|e| format!("play: {e}"))?;

    let (w, h) = (320usize, 240usize);
    let mut buf = vec![0u8; w * h * 4];
    let start = Instant::now();
    let mut last_pos = -1.0f64;
    let mut last_move = Instant::now();
    let mut worst = Duration::ZERO;
    let mut frames = 0u32;
    while start.elapsed() < Duration::from_secs(secs) {
        if render.needs_frame() {
            render
                .render_sw(w as i32, h as i32, w * 4, &mut buf)
                .map_err(|e| format!("render_sw: {e}"))?;
            frames += 1;
        }
        while let Some(ev) = player.wait_event(0.0) {
            if matches!(ev, PlayerEvent::Shutdown) {
                return Err("mpv shut down".into());
            }
        }
        if let Some(p) = player.time_pos() {
            if p > last_pos + 0.01 {
                last_pos = p;
                last_move = Instant::now();
            }
        }
        worst = worst.max(last_move.elapsed());
        std::thread::sleep(Duration::from_millis(10));
    }
    let lit = buf.chunks(4).filter(|p| p[..3] != [0, 0, 0]).count();
    println!(
        "played {:.1} s in {:.1} s, {frames} frames drawn, longest stall {} ms, {lit} lit pixels",
        last_pos,
        start.elapsed().as_secs_f64(),
        worst.as_millis()
    );
    if worst > MAX_STALL {
        return Err(format!("playback stalled for {} ms", worst.as_millis()));
    }
    if last_pos < secs as f64 * 0.8 {
        return Err(format!("position only reached {last_pos:.1} s"));
    }
    if frames < secs as u32 * 10 || lit == 0 {
        return Err("too few frames drawn".into());
    }
    let _ = std::fs::remove_file(&path);
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => {
            println!("playback: OK");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("playback: FAILED: {e}");
            ExitCode::FAILURE
        }
    }
}
