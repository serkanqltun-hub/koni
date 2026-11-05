//! "TV'de oynat": oynatılan medyayı aynı ağdaki bir TV'ye gönderir.
//!
//! Yerel dosya ve altyazı `serve` ile ev ağına açılır; doğrudan `http(s)` akışlar
//! TV'ye olduğu gibi verilir. Cihaz türleri (Chromecast, DLNA) kendi dosyalarında
//! `devices` ve `play` içine eklenir.

pub mod dlna;
pub mod serve;

use crate::i18n::trs;
use koni_player::Player;
use serde::{Deserialize, Serialize};

/// Ağda bulunan bir TV. `kind` hangi gönderim yolunun kullanılacağını söyler.
#[derive(Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: String,
}

/// TV'nin açacağı adresler.
#[derive(Clone, Serialize)]
pub struct Media {
    pub url: String,
    /// Seçili altyazı (VTT); yerel dosya değilse ya da gömülüyse `None`.
    pub sub_url: Option<String>,
    pub mime: String,
    pub title: String,
}

/// TV'ye giden kontrol komutu. JS'den `{ action: "seek", seconds: 30 }` biçiminde gelir.
#[derive(Clone, Copy, Deserialize)]
#[serde(tag = "action", content = "seconds", rename_all = "lowercase")]
pub enum Control {
    Play,
    Pause,
    Stop,
    Seek(f64),
}

/// TV'deki oynatma konumu (saniye); TV bilmiyorsa süre 0.
#[derive(Clone, Copy, Serialize)]
pub struct Position {
    pub position: f64,
    pub duration: f64,
    pub playing: bool,
}

/// Aynı ağdaki TV'ler. Her tür kendi bulucusunu buraya ekler.
pub fn devices() -> Vec<Device> {
    let mut out = Vec::new();
    out.extend(dlna::devices());
    out
}

fn unsupported(device: &Device) -> String {
    format!(
        "{}: {}",
        trs("bu cihaza gönderilemiyor", "can't send to this device"),
        device.name
    )
}

/// `media`yı cihazda oynatır.
pub fn play(device: &Device, media: &Media) -> Result<(), String> {
    match device.kind.as_str() {
        "dlna" => dlna::play(device, media),
        _ => Err(unsupported(device)),
    }
}

/// Oynat / duraklat / dur / sar.
pub fn control(device: &Device, c: Control) -> Result<(), String> {
    match device.kind.as_str() {
        "dlna" => dlna::control(device, c),
        _ => Err(unsupported(device)),
    }
}

/// TV'deki konum.
pub fn position(device: &Device) -> Result<Position, String> {
    match device.kind.as_str() {
        "dlna" => dlna::position(device),
        _ => Err(unsupported(device)),
    }
}

/// mpv'de açık olanı TV'ye hazırlar: yerel dosyayı ağa açar, altyazıyı VTT'ye çevirir.
pub fn prepare(mpv: &Player) -> Result<Media, String> {
    let Some(path) = mpv.get_property_string("path").filter(|p| !p.is_empty()) else {
        return Err(trs("açık video yok", "nothing is playing").into());
    };
    let title = mpv
        .get_property_string("media-title")
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| crate::media::file_name(&path));
    let sub = mpv
        .get_property_string("current-tracks/sub/external-filename")
        .filter(|f| std::path::Path::new(f).is_file())
        .and_then(|f| std::fs::read(f).ok())
        .and_then(|b| to_vtt(&crate::addons::decode_sub_bytes(&b)));
    if path.starts_with("https://") || path.starts_with("http://") {
        let sub_url = sub.map(serve::publish_sub).transpose()?;
        return Ok(Media {
            mime: serve::mime(&path).into(),
            url: path,
            sub_url,
            title,
        });
    }
    let file = path.strip_prefix("file://").unwrap_or(&path);
    if !std::path::Path::new(file).is_file() {
        return Err(trs(
            "bu yayın TV'ye gönderilemiyor",
            "this stream can't be sent to a TV",
        )
        .into());
    }
    let (url, sub_url) = serve::publish(file, sub)?;
    Ok(Media {
        url,
        sub_url,
        mime: serve::mime(file).into(),
        title,
    })
}

/// SRT ya da VTT metnini WebVTT'ye çevirir; ASS/SSA çevrilmez (`None`).
pub fn to_vtt(text: &str) -> Option<String> {
    let text = text.replace("\r\n", "\n");
    if text.trim_start().starts_with("WEBVTT") {
        return Some(text);
    }
    if text.contains("[Script Info]") {
        return None;
    }
    let mut out = String::from("WEBVTT\n\n");
    for line in text.lines() {
        if line.contains("-->") {
            out.push_str(&line.replace(',', "."));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn srt_to_vtt() {
        let v = super::to_vtt("1\r\n00:00:01,000 --> 00:00:02,500\r\nMerhaba, dünya\r\n").unwrap();
        assert!(v.starts_with("WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.500\nMerhaba, dünya"));
        assert!(super::to_vtt("[Script Info]\n").is_none());
    }
}
