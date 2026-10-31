//! Smoke test for the libmpv FFI: `smoke [media] [--lib PATH]`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use koni_player::{Player, PlayerEvent, PlayerOptions};

const DEFAULT_MEDIA: &str = "av://lavfi:testsrc=duration=3:size=320x240:rate=25";

fn run() -> Result<(), String> {
    let mut media = DEFAULT_MEDIA.to_string();
    let mut lib_path = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--lib" {
            lib_path = Some(PathBuf::from(args.next().ok_or("--lib needs a PATH")?));
        } else {
            media = a;
        }
    }

    let player = Player::new(PlayerOptions {
        wid: None,
        lib_path,
        extra_options: vec![("vo".into(), "null".into()), ("ao".into(), "null".into())],
    })
    .map_err(|e| format!("Player::new: {e}"))?;
    println!(
        "mpv-version: {:?}",
        player.get_property_string("mpv-version")
    );

    player.load(&media).map_err(|e| format!("load: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if Instant::now() > deadline {
            return Err("timed out waiting for FileLoaded".into());
        }
        match player.wait_event(0.5) {
            Some(PlayerEvent::FileLoaded) => break,
            Some(PlayerEvent::EndFile) => return Err("EndFile before FileLoaded".into()),
            Some(PlayerEvent::Shutdown) => return Err("mpv shut down".into()),
            Some(ev) => println!("event: {ev:?}"),
            None => {}
        }
    }
    println!("file loaded, duration: {:?}", player.duration());

    player
        .seek_absolute(1.0)
        .map_err(|e| format!("seek: {e}"))?;
    player
        .set_speed(1.04)
        .map_err(|e| format!("set_speed: {e}"))?;
    player.play().map_err(|e| format!("play: {e}"))?;

    let end = Instant::now() + Duration::from_millis(1500);
    let mut prop_events = 0;
    while Instant::now() < end {
        match player.wait_event(0.1) {
            Some(ev @ PlayerEvent::PropertyChanged { .. }) => {
                prop_events += 1;
                println!("event: {ev:?}");
            }
            Some(PlayerEvent::Shutdown) => return Err("mpv shut down".into()),
            Some(ev) => println!("event: {ev:?}"),
            None => {}
        }
    }

    let snap = player.snapshot();
    println!(
        "{}",
        serde_json::to_string_pretty(&snap).map_err(|e| e.to_string())?
    );
    if prop_events == 0 {
        return Err("no PropertyChanged events received".into());
    }
    if (snap.speed - 1.04).abs() > 1e-6 {
        return Err(format!("speed is {}, expected 1.04", snap.speed));
    }
    match snap.time_pos {
        Some(t) if t >= 1.0 || snap.eof_reached => Ok(()),
        other => Err(format!("unexpected time_pos {other:?}")),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => {
            println!("smoke: OK");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("smoke: FAILED: {e}");
            ExitCode::FAILURE
        }
    }
}
