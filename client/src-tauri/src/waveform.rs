//! Müzikte seek çubuğunun arkasındaki dalga formu (docs/design/media.md §6.3).
//!
//! Ayrı bir ffmpeg yok: ikinci, görüntüsüz bir libmpv sesi 8 kHz mono 16 bit PCM'e
//! çözer (`ao=pcm` zamanlamasızdır, çalma hızını beklemez) ve dosyadan sütun başına
//! tepe değeri çıkarılır. Yalnızca görüntüsüz seste (müzik) ve kısa parçada.

use crate::i18n::trs;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use koni_player::{Player, PlayerEvent, PlayerOptions};

/// Sütun sayısı.
pub const BARS: usize = 160;
/// Bundan uzun sesin dalga formu çıkarılmaz (film sesi değil, şarkı).
pub const MAX_SECS: f64 = 20.0 * 60.0;
const RATE: &str = "8000";
const TIMEOUT: Duration = Duration::from_secs(120);

/// Son hesaplanan (kaynak → sütunlar): aynı parça için yeniden çözme.
static LAST: Mutex<Option<(String, Vec<f32>)>> = Mutex::new(None);

pub fn cached(src: &str) -> Option<Vec<f32>> {
    LAST.lock()
        .unwrap()
        .as_ref()
        .filter(|(s, _)| s == src)
        .map(|(_, v)| v.clone())
}

/// `src`'yi (dosya ya da adres) çözer; geçici PCM dosyası `wav` silinir. Bloklar.
pub fn compute(src: &str, wav: &Path) -> Result<Vec<f32>, String> {
    let o = |k: &str, v: &str| (k.to_owned(), v.to_owned());
    let opts = PlayerOptions {
        wid: None,
        lib_path: crate::libmpv_next_to_exe(),
        extra_options: vec![
            o("vid", "no"),
            o("sid", "no"),
            o("vo", "null"),
            o("hwdec", "no"),
            o("keep-open", "no"),
            o("audio-pitch-correction", "no"),
            o("ao", "pcm"),
            o("ao-pcm-file", &wav.to_string_lossy()),
            o("ao-pcm-waveheader", "no"),
            o("audio-channels", "mono"),
            o("audio-samplerate", RATE),
            o("audio-format", "s16"),
        ],
    };
    let p = Player::new(opts).map_err(|e| e.to_string())?;
    p.load(src).map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        if let Some(PlayerEvent::EndFile | PlayerEvent::Shutdown) = p.wait_event(1.0) {
            break;
        }
        if start.elapsed() > TIMEOUT {
            return Err(trs("dalga formu zaman aşımı", "waveform timed out").into());
        }
    }
    drop(p); // dosya kapansın
    let bytes = std::fs::read(wav).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(wav);
    let v = peaks(&bytes?, BARS);
    if v.is_empty() {
        return Err(trs("ses çözülemedi", "couldn't decode audio").into());
    }
    *LAST.lock().unwrap() = Some((src.to_owned(), v.clone()));
    Ok(v)
}

/// s16le mono örneklerden `bars` sütunun tepe değeri, en yüksek sütun 1 olacak şekilde.
fn peaks(pcm: &[u8], bars: usize) -> Vec<f32> {
    let n = pcm.len() / 2;
    if n < bars || bars == 0 {
        return Vec::new();
    }
    let mut out: Vec<f32> = (0..bars)
        .map(|b| {
            let (from, to) = (b * n / bars, (b + 1) * n / bars);
            (from..to)
                .map(|i| i16::from_le_bytes([pcm[2 * i], pcm[2 * i + 1]]).unsigned_abs())
                .max()
                .unwrap_or(0) as f32
        })
        .collect();
    let top = out.iter().cloned().fold(0.0, f32::max);
    if top > 0.0 {
        out.iter_mut().for_each(|v| *v /= top);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_are_per_column_and_normalized() {
        // 4 sütun: sessiz, yarım, tam (negatif tepe), çeyrek
        let samples: [i16; 8] = [0, 0, 8000, -100, -16000, 50, 4000, 0];
        let pcm: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        assert_eq!(peaks(&pcm, 4), vec![0.0, 0.5, 1.0, 0.25]);
        assert!(peaks(&pcm, 9).is_empty(), "sütundan az örnek");
    }
}
