//! Santral hesabı (PROTOCOL.md §3.0, docs/design/accounts.md §5.1). Oturum belirteci
//! sunucu adresine göre işletim sistemi anahtar zincirinde tutulur (Windows Credential
//! Manager / macOS Keychain); şifre hiçbir yerde saklanmaz.

use crate::i18n::trs;
use futures_util::{SinkExt, StreamExt};
use koni_sync::protocol::{ClientMsg, ServerMsg};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

const SERVICE: &str = "app.koni.desktop";
const TIMEOUT: Duration = Duration::from_secs(10);

/// Kayıtlı oturum.
#[derive(Serialize, Deserialize, Clone)]
pub struct Account {
    pub token: String,
    pub name: String,
    /// p0.18: `Ad#etiket`'in etiketi ve profil fotoğrafı (eski kayıtlarda boş).
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub avatar: String,
}

fn entry(url: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, url).map_err(|e| tr!("anahtar zinciri: {e}", "keychain: {e}"))
}

/// Bu sunucu için kayıtlı oturum.
pub fn get(url: &str) -> Option<Account> {
    let s = entry(url).ok()?.get_password().ok()?;
    serde_json::from_str(&s).ok()
}

pub fn save(url: &str, acc: &Account) -> Result<(), String> {
    let json = serde_json::to_string(acc).map_err(|e| e.to_string())?;
    entry(url)?
        .set_password(&json)
        .map_err(|e| tr!("anahtar zinciri: {e}", "keychain: {e}"))
}

pub fn forget(url: &str) -> Result<(), String> {
    match entry(url)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(tr!("anahtar zinciri: {e}", "keychain: {e}")),
    }
}

/// Oturumlar listesinde görünen cihaz etiketi (bilgisayar adı değil, yalnızca işletim sistemi).
pub fn device() -> String {
    format!("KONİ {}", std::env::consts::OS)
}

/// Tek mesajlık el sıkışma: gönderir, ilk cevabın metnini döndürür (`error` → `Err`).
async fn exchange(url: &str, msg: &ClientMsg) -> Result<String, String> {
    let run = async {
        let (mut ws, _) = tokio_tungstenite::connect_async(url).await.map_err(|e| {
            tr!(
                "{url} bağlantısı kurulamadı: {e}",
                "Couldn't connect to {url}: {e}"
            )
        })?;
        let text = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        ws.send(Message::Text(text.into()))
            .await
            .map_err(|e| e.to_string())?;
        while let Some(m) = ws.next().await {
            let Message::Text(t) = m.map_err(|e| e.to_string())? else {
                continue;
            };
            let _ = ws.close(None).await;
            if let Ok(ServerMsg::Error { code, message }) = serde_json::from_str::<ServerMsg>(&t) {
                return Err(if message.is_empty() { code } else { message });
            }
            return Ok(t.to_string());
        }
        Err(trs(
            "sunucu cevap vermeden kapattı",
            "the server closed without answering",
        )
        .to_string())
    };
    tokio::time::timeout(TIMEOUT, run)
        .await
        .map_err(|_| tr!("{url} cevap vermedi", "{url} didn't respond"))?
}

/// `register`/`login`/`logout` el sıkışması.
pub async fn handshake(url: &str, msg: &ClientMsg) -> Result<Account, String> {
    let t = exchange(url, msg).await?;
    match serde_json::from_str::<ServerMsg>(&t) {
        Ok(ServerMsg::Auth {
            token,
            name,
            tag,
            avatar,
        }) => Ok(Account {
            token,
            name,
            tag,
            avatar,
        }),
        _ => Err(tr!("beklenmeyen cevap: {t}", "unexpected response: {t}")),
    }
}

/// `bug_report` el sıkışması; dönen değer sunucudaki rapor adı.
pub async fn bug_report(url: &str, msg: &ClientMsg) -> Result<String, String> {
    let t = exchange(url, msg).await?;
    let v: serde_json::Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
    if v["t"] != "bug_report_ok" {
        return Err(tr!("beklenmeyen cevap: {t}", "unexpected response: {t}"));
    }
    Ok(v["id"].as_str().unwrap_or_default().to_string())
}

/// `friends` el sıkışması; cevap (`friends` + `requests`) olduğu gibi arayüze gider.
pub async fn friends(
    url: &str,
    auth: String,
    op: String,
    user: String,
) -> Result<serde_json::Value, String> {
    let t = exchange(url, &ClientMsg::Friends { auth, op, user }).await?;
    let v: serde_json::Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
    if v["t"] != "friends" {
        return Err(tr!("beklenmeyen cevap: {t}", "unexpected response: {t}"));
    }
    Ok(v)
}

/// `stats` el sıkışması; cevap (kullanıcılar + rozetler) olduğu gibi arayüze gider.
pub async fn stats(url: &str, auth: String, period: String) -> Result<serde_json::Value, String> {
    let t = exchange(url, &ClientMsg::Stats { auth, period }).await?;
    let v: serde_json::Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
    if v["t"] != "stats" {
        return Err(tr!("beklenmeyen cevap: {t}", "unexpected response: {t}"));
    }
    Ok(v)
}

/// `feed` el sıkışması (p0.22): ana ekran "Etkinlik" sekmesi.
pub async fn feed(url: &str, auth: String) -> Result<serde_json::Value, String> {
    let t = exchange(url, &ClientMsg::Feed { auth }).await?;
    let v: serde_json::Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
    if v["t"] != "feed" {
        return Err(tr!("beklenmeyen cevap: {t}", "unexpected response: {t}"));
    }
    Ok(v)
}

/// `lists` el sıkışması (p0.23); cevap olduğu gibi arayüze gider.
pub async fn lists(url: &str, msg: ClientMsg) -> Result<serde_json::Value, String> {
    let t = exchange(url, &msg).await?;
    let v: serde_json::Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
    if v["t"] == "error" {
        return Err(v["message"]
            .as_str()
            .unwrap_or(trs("liste hatası", "list error"))
            .to_string());
    }
    if v["t"] != "lists" {
        return Err(tr!("beklenmeyen cevap: {t}", "unexpected response: {t}"));
    }
    Ok(v)
}

/// `profile` el sıkışması (p0.18); cevap olduğu gibi arayüze gider.
pub async fn profile(
    url: &str,
    auth: String,
    op: String,
    avatar: String,
    image: String,
) -> Result<serde_json::Value, String> {
    let msg = ClientMsg::Profile {
        auth,
        op,
        avatar,
        image,
    };
    let t = exchange(url, &msg).await?;
    let v: serde_json::Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
    if v["t"] != "profile" {
        return Err(tr!("beklenmeyen cevap: {t}", "unexpected response: {t}"));
    }
    Ok(v)
}

/// `wss://host/ws` → `https://host`: santralin HTTP uçları aynı adreste.
pub fn http_base(url: &str) -> String {
    let base = url
        .replacen("wss://", "https://", 1)
        .replacen("ws://", "http://", 1);
    match base
        .find("://")
        .and_then(|i| base[i + 3..].find('/').map(|j| i + 3 + j))
    {
        Some(cut) => base[..cut].to_string(),
        None => base,
    }
}

/// Profil fotoğrafı `data:` adresi olarak; aynı fotoğraf bir kez indirilir.
pub async fn avatar(url: &str, auth: &str, id: &str) -> Result<String, String> {
    use base64::Engine;
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};
    static CACHE: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        || id.is_empty()
    {
        return Err(trs("geçersiz fotoğraf", "invalid photo").into());
    }
    let key = format!("{url}|{id}");
    if let Some(v) = CACHE.lock().unwrap().get(&key) {
        return Ok(v.clone());
    }
    let res = crate::addons::http()
        .get(format!("{}/avatars/{id}", http_base(url)))
        .bearer_auth(auth)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(tr!(
            "fotoğraf alınamadı ({})",
            "couldn't fetch photo ({})",
            res.status()
        ));
    }
    let mime = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();
    let body = res.bytes().await.map_err(|e| e.to_string())?;
    let v = format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&body)
    );
    CACHE.lock().unwrap().insert(key, v.clone());
    Ok(v)
}

/// `admin` el sıkışması (p0.19); cevap (odalar + üyeler) olduğu gibi arayüze gider.
pub async fn admin(url: &str, msg: &ClientMsg) -> Result<serde_json::Value, String> {
    let t = exchange(url, msg).await?;
    let v: serde_json::Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
    if v["t"] != "admin" {
        return Err(tr!("beklenmeyen cevap: {t}", "unexpected response: {t}"));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    #[test]
    fn http_base() {
        assert_eq!(super::http_base("wss://a.example/ws"), "https://a.example");
        assert_eq!(
            super::http_base("ws://127.0.0.1:8080/ws"),
            "http://127.0.0.1:8080"
        );
        assert_eq!(super::http_base("wss://a.example"), "https://a.example");
    }
}
