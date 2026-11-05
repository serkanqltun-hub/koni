//! Donma bekçisi: ana (UI) thread'i ayrı bir thread'den yoklar.
//!
//! Her yarım saniyede ana thread'e boş bir iş gönderilir; 2 sn içinde yanıt
//! gelmezse takılma `app_log_dir/freeze.log`'a yazılır. Ana thread yeniden
//! yanıt verince süre loga eklenir ve arayüze `app://freeze` (ms) olayı gider.
//! Mac'te video da ana thread'de çizildiği için takılma orada en çok görünür.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{Emitter, Manager};

const TICK: Duration = Duration::from_millis(500);
const LIMIT: Duration = Duration::from_secs(2);
pub const LOG_FILE: &str = "freeze.log";

pub fn spawn(app: tauri::AppHandle) {
    let log = app.path().app_log_dir().ok().map(|d| d.join(LOG_FILE));
    std::thread::Builder::new()
        .name("koni-watchdog".into())
        .spawn(move || {
            let done = Arc::new(AtomicBool::new(true));
            loop {
                std::thread::sleep(TICK);
                done.store(false, Ordering::SeqCst);
                let flag = done.clone();
                let sent = Instant::now();
                if app
                    .run_on_main_thread(move || flag.store(true, Ordering::SeqCst))
                    .is_err()
                {
                    return; // olay döngüsü kapandı
                }
                let mut reported = false;
                while !done.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(50));
                    if !reported && sent.elapsed() >= LIMIT {
                        reported = true;
                        write(&log, "ana thread 2 sn'dir yanıt vermiyor");
                    }
                }
                if reported {
                    let ms = sent.elapsed().as_millis() as u64;
                    write(
                        &log,
                        &format!("ana thread yeniden yanıt verdi, takılma {ms} ms"),
                    );
                    let _ = app.emit("app://freeze", ms);
                }
            }
        })
        .expect("bekçi thread'i başlatılamadı");
}

fn write(path: &Option<std::path::PathBuf>, msg: &str) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    eprintln!("[koni] donma: {msg}");
    let Some(path) = path else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Dosya büyümesin: 256 KB'ı geçince baştan başla.
    if std::fs::metadata(path)
        .map(|m| m.len() > 256 * 1024)
        .unwrap_or(false)
    {
        let _ = std::fs::remove_file(path);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{secs} {msg}");
    }
}

/// Sorun bildirimi için logun son `lines` satırı.
pub fn tail(dir: &std::path::Path, lines: usize) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(LOG_FILE)).ok()?;
    let all: Vec<&str> = text.lines().collect();
    Some(all[all.len().saturating_sub(lines)..].join("\n") + "\n")
}
