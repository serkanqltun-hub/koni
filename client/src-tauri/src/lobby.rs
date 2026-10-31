//! Lobi bağlantısı (PROTOCOL.md §3.0e, p0.18): uygulama açıkken santrale açık kalır;
//! arkadaşların çevrimiçi durumu ve odaya çağrılar `lobby://msg` ile arayüze ham gider.
//! Bağlantı koparsa artan aralıklarla yeniden denenir; oturum geçersizse durur.

use futures_util::{SinkExt, StreamExt};
use koni_sync::protocol::ClientMsg;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// Sunucu 20 sn sessiz bağlantıyı kapatır.
const PING: Duration = Duration::from_secs(8);

enum Cmd {
    Send(String),
    Stop,
}

static LOBBY: Mutex<Option<mpsc::UnboundedSender<Cmd>>> = Mutex::new(None);

/// Lobiyi (yeniden) açar; açık olan kapanır.
pub fn start(app: AppHandle, url: String, auth: String) {
    let (tx, rx) = mpsc::unbounded_channel();
    if let Some(old) = LOBBY.lock().unwrap().replace(tx) {
        let _ = old.send(Cmd::Stop);
    }
    tauri::async_runtime::spawn(run(app, url, auth, rx));
}

pub fn stop() {
    if let Some(old) = LOBBY.lock().unwrap().take() {
        let _ = old.send(Cmd::Stop);
    }
}

/// Lobiye ham mesaj (ör. `decline`); lobi kapalıysa düşer.
pub fn send(text: String) {
    if let Some(tx) = LOBBY.lock().unwrap().as_ref() {
        let _ = tx.send(Cmd::Send(text));
    }
}

enum End {
    Stopped,
    /// Oturum geçersiz ya da sunucu lobiyi tanımıyor: yeniden denemek boşuna.
    Fatal,
    Dropped,
}

async fn run(app: AppHandle, url: String, auth: String, mut rx: mpsc::UnboundedReceiver<Cmd>) {
    let mut wait = Duration::from_secs(2);
    loop {
        match session(&app, &url, &auth, &mut rx).await {
            End::Stopped | End::Fatal => return,
            End::Dropped => {}
        }
        let _ = app.emit("lobby://msg", serde_json::json!({ "t": "offline" }));
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            c = rx.recv() => if matches!(c, None | Some(Cmd::Stop)) { return },
        }
        wait = (wait * 2).min(Duration::from_secs(30));
    }
}

async fn session(
    app: &AppHandle,
    url: &str,
    auth: &str,
    rx: &mut mpsc::UnboundedReceiver<Cmd>,
) -> End {
    let Ok(Ok((mut ws, _))) = tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::connect_async(url),
    )
    .await
    else {
        return End::Dropped;
    };
    let hello = serde_json::to_string(&ClientMsg::Hello {
        auth: auth.to_string(),
    })
    .unwrap();
    if ws.send(Message::Text(hello.into())).await.is_err() {
        return End::Dropped;
    }
    let mut tick = tokio::time::interval(PING);
    loop {
        tokio::select! {
            m = ws.next() => {
                let Some(Ok(m)) = m else { return End::Dropped };
                let Message::Text(t) = m else { continue };
                let Ok(v) = serde_json::from_str::<serde_json::Value>(t.as_str()) else { continue };
                let fatal = v["t"] == "error";
                let _ = app.emit("lobby://msg", &v);
                if fatal {
                    return End::Fatal;
                }
            }
            _ = tick.tick() => {
                if ws.send(Message::Text(r#"{"t":"ping"}"#.into())).await.is_err() {
                    return End::Dropped;
                }
            }
            c = rx.recv() => match c {
                Some(Cmd::Send(t)) => {
                    if ws.send(Message::Text(t.into())).await.is_err() {
                        return End::Dropped;
                    }
                }
                None | Some(Cmd::Stop) => {
                    let _ = ws.close(None).await;
                    return End::Stopped;
                }
            },
        }
    }
}
