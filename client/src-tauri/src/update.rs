//! Otomatik güncelleme (tauri-plugin-updater): `latest.json` public
//! `Savas07/koni-releases` reposundan okunur, NSIS kurulumu indirilir ve imzası
//! `tauri.conf.json` içindeki açık anahtarla doğrulanır. Windows'ta kurulum
//! sessizce (`passive`) çalışır, uygulama kapanır ve kurulum bitince yeniden açılır.

use crate::i18n::trs;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    version: String,
    notes: Option<String>,
    /// Taşınabilir (zip) kopya: kurulum yapılmaz, kullanıcı elle günceller.
    portable: bool,
}

/// NSIS kurulumu exe'nin yanına `uninstall.exe` koyar; yoksa taşınabilir kopyadır.
fn is_portable() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| !d.join("uninstall.exe").exists()))
        .unwrap_or(true)
}

/// Yeni sürüm varsa bilgisini döner. Geliştirme derlemesinde hiç bakmaz.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    if cfg!(debug_assertions) {
        return Ok(None);
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| {
            tr!(
                "Güncelleme denetlenemedi: {e}",
                "Couldn't check for updates: {e}"
            )
        })?;
    Ok(update.map(|u| UpdateInfo {
        version: u.version,
        notes: u.body,
        portable: is_portable(),
    }))
}

/// Güncellemeyi indirir, doğrular ve kurar. Başarılıysa geri dönmez: Windows'ta
/// kurulum başlar ve süreç kapanır. İlerleme `update://progress` olayıyla
/// `[indirilen, toplam]` bayt olarak gelir.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    if is_portable() {
        return Err(trs(
            "Taşınabilir sürüm kendini güncelleyemez",
            "The portable version can't update itself",
        )
        .into());
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| {
            tr!(
                "Güncelleme denetlenemedi: {e}",
                "Couldn't check for updates: {e}"
            )
        })?
        .ok_or(trs("Yeni sürüm yok", "No new version"))?;
    let mut done = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                done += chunk as u64;
                let _ = app.emit("update://progress", (done, total));
            },
            || {},
        )
        .await
        .map_err(|e| {
            tr!(
                "Güncelleme kurulamadı: {e}",
                "Couldn't install the update: {e}"
            )
        })?;
    // Windows'ta buraya gelinmez (kurulum süreci devralır); diğerlerinde yeniden başlat.
    app.restart()
}
