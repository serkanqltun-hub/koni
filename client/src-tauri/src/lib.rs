//! KONİ istemcisi: Tauri kabuğu + kokpit video yüzeyi + libmpv.
//!
//! Faz 0 spike. Video, ana pencerenin altında native bir child pencerede
//! (Windows'ta HWND) mpv tarafından çizilir; web UI sağ kolon ve alt bardadır.

#[macro_use]
mod i18n;
mod account;
mod adblock;
mod addons;
mod commands;
mod debrid;
mod download;
mod lobby;
mod lounge;
mod media;
mod page;
mod sniffer;
mod spotify;
mod sync;
mod tracks;
mod update;
mod waveform;

use std::sync::{Arc, Mutex};

use koni_player::{Player, PlayerOptions};
use koni_surface::LayoutConfig;
use tauri::Manager;

/// Platforma göre video yüzeyi. Windows dışı platformlarda henüz yok.
#[cfg(windows)]
pub type Surface = koni_surface::Win32ChildSurface;
#[cfg(not(windows))]
pub type Surface = NoSurface;

/// Windows dışı derlemeler için yer tutucu (macOS Faz 2'de gelecek).
#[cfg(not(windows))]
pub struct NoSurface;

#[derive(Default)]
pub struct AppState {
    /// libmpv yüklenemezse `None`; komutlar anlaşılır bir hata döner.
    pub player: Mutex<Option<Arc<Player>>>,
    pub player_error: Mutex<Option<String>>,
    /// Yalnızca ana (UI) thread'den dokunulur: `run_on_main_thread` ile.
    pub surface: Mutex<Option<Surface>>,
    pub layout: Mutex<LayoutConfig>,
    /// Açık oda oturumu (yoksa oynatıcı yerel kontrol edilir).
    pub sync: Mutex<Option<sync::SyncHandle>>,
    /// Açık Sanal Yakalayıcının sayfası; yakalanınca ya da pencere kapanınca boşalır.
    pub sniff_page: Mutex<Option<String>>,
    /// Keşfet modunda tarayıcının oturacağı alan (`sniffer::set_embed`).
    pub sniff_rect: Mutex<Option<sniffer::EmbedRect>>,
    /// Açık yakalayıcı video alanına gömülü mü (çerçevesiz).
    pub sniff_embedded: Mutex<bool>,
}

pub fn run() {
    let builder = tauri::Builder::default();
    // Tek örnek: ikinci açılış (davet linkine tıklama) mevcut pencereyi öne getirir;
    // deep-link özelliği linki ilk örneğe iletir. Diğer eklentilerden önce kurulmalı.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
    }));
    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::default())
        .setup(|app| {
            // Geliştirme modunda (dev.ps1) kurulum yok: koni:// şemasını bu exe'ye kaydet.
            #[cfg(all(debug_assertions, any(windows, target_os = "linux")))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(e) = app.deep_link().register_all() {
                    eprintln!("[koni] koni:// kaydedilemedi: {e}");
                }
            }
            let window = app.get_webview_window("main").ok_or(crate::i18n::trs(
                "ana pencere bulunamadı",
                "main window not found",
            ))?;
            let handle = app.handle().clone();
            window.on_window_event(move |e| {
                if matches!(
                    e,
                    tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)
                ) {
                    sniffer::place(&handle);
                }
            });
            let state = app.state::<AppState>();
            let wid = init_surface(&window, &state);
            init_player(app.handle(), &state, wid);
            if let Ok(dir) = app.path().app_data_dir() {
                media::spawn_ytdlp_update(dir);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::player_load,
            commands::player_play,
            commands::player_pause,
            commands::player_toggle_pause,
            commands::player_seek,
            commands::player_set_speed,
            commands::player_set_volume,
            commands::player_add_subtitle,
            commands::player_sub_delay,
            commands::player_tracks,
            commands::player_music_view,
            commands::player_cover,
            commands::player_set_track,
            commands::player_state,
            commands::set_layout,
            commands::set_fullscreen,
            commands::open_cast_panel,
            commands::sync_connect,
            commands::sync_play,
            commands::sync_pause,
            commands::sync_seek,
            commands::sync_close,
            commands::sync_leave,
            commands::sync_continue_without,
            commands::sync_set_cohost,
            commands::sync_set_policy,
            commands::sync_set_view,
            commands::web_search,
            commands::media_list,
            commands::sync_resync,
            commands::sync_download,
            commands::account_get,
            commands::account_login,
            commands::account_register,
            commands::account_logout,
            commands::stats_get,
            commands::feed_get,
            commands::lists_call,
            commands::friends,
            commands::profile,
            commands::admin,
            commands::avatar_get,
            commands::lobby_start,
            commands::lobby_decline,
            commands::lobby_remote_state,
            commands::bug_report,
            commands::sync_fun,
            commands::queue_add,
            commands::media_waveform,
            commands::media_prefetch,
            commands::subs_hash,
            commands::subs_search_episode,
            commands::subs_apply_url,
            commands::player_add_subtitle_text,
            commands::sniffer_close,
            commands::browse_open,
            commands::browse_embed,
            commands::browse_nav,
            commands::browse_state,
            commands::sync_page,
            commands::page_video,
            commands::debrid_status,
            commands::debrid_set_key,
            commands::debrid_clear_key,
            commands::set_relay,
            commands::addons_list,
            commands::addon_add,
            commands::addon_remove,
            commands::catalog_search,
            commands::catalog_popular,
            commands::catalog_episodes,
            commands::addon_streams,
            commands::debrid_open,
            commands::debrid_check,
            commands::set_lang,
            update::update_check,
            update::update_install,
        ])
        .run(tauri::generate_context!())
        .expect("KONİ başlatılamadı");
}

/// Child video penceresini oluşturur ve mpv'ye verilecek `wid`'i döner.
/// `setup` ana thread'de çalışır, Win32 çağrıları için doğru yer.
#[cfg(windows)]
fn init_surface(window: &tauri::WebviewWindow, state: &AppState) -> Option<i64> {
    use koni_surface::{MpvTarget, VideoSurface};
    use raw_window_handle::HasWindowHandle;

    let layout = state.layout.lock().unwrap().clone();
    let handle = match window.window_handle() {
        Ok(h) => h.as_raw(),
        Err(e) => {
            eprintln!("[koni] pencere handle alınamadı: {e}");
            return None;
        }
    };
    match Surface::create(handle, &layout) {
        Ok(mut surface) => {
            if let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) {
                surface.on_parent_resized(size.width as i32, size.height as i32, scale);
            }
            let MpvTarget::Wid(wid) = surface.mpv_target();
            *state.surface.lock().unwrap() = Some(surface);
            Some(wid)
        }
        Err(e) => {
            eprintln!("[koni] video yüzeyi oluşturulamadı: {e}");
            None
        }
    }
}

#[cfg(not(windows))]
fn init_surface(_window: &tauri::WebviewWindow, _state: &AppState) -> Option<i64> {
    eprintln!("[koni] bu platformda gömülü video yüzeyi yok; mpv kendi penceresini açar");
    None
}

fn init_player(app: &tauri::AppHandle, state: &AppState, wid: Option<i64>) {
    let opts = PlayerOptions {
        wid,
        lib_path: libmpv_next_to_exe(),
        ..Default::default()
    };
    match Player::new(opts) {
        Ok(player) => {
            let player = Arc::new(player);
            spawn_event_pump(app.clone(), player.clone());
            *state.player.lock().unwrap() = Some(player);
        }
        Err(e) => {
            let msg = tr!("libmpv yüklenemedi: {e}", "couldn't load libmpv: {e}");
            eprintln!("[koni] {msg}");
            *state.player_error.lock().unwrap() = Some(msg);
        }
    }
}

/// Exe'nin yanındaki libmpv DLL'i varsa onu kullan (LGPL: kullanıcı değiştirebilir).
pub(crate) fn libmpv_next_to_exe() -> Option<std::path::PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    ["libmpv-2.dll", "mpv-2.dll"]
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.exists())
}

/// mpv olay kuyruğunu boşaltır (`wait_event` tek thread'den çağrılmalı).
/// Odadayken yeni yüklenen dosya oda konumuna hizalanır.
fn spawn_event_pump(app: tauri::AppHandle, player: Arc<Player>) {
    std::thread::Builder::new()
        .name("mpv-events".into())
        .spawn(move || loop {
            match player.wait_event(1.0) {
                Some(koni_player::PlayerEvent::Shutdown) => break,
                Some(koni_player::PlayerEvent::FileLoaded) => {
                    eprintln!("[koni] dosya yüklendi");
                    tracks::on_file_loaded(&player);
                    let music = lounge::on_file_loaded(&player);
                    let state = app.state::<AppState>();
                    let tx = state.sync.lock().unwrap().as_ref().map(|h| h.tx.clone());
                    if let Some(tx) = tx {
                        let _ = tx.send(sync::UiCmd::Music(music));
                        let _ = tx.send(sync::UiCmd::Realign);
                    }
                }
                Some(koni_player::PlayerEvent::EndFile) => eprintln!("[koni] dosya bitti"),
                _ => {}
            }
        })
        .expect("mpv olay thread'i başlatılamadı");
}

#[cfg(test)]
mod acl_tests {
    /// Kayıtlı her komut build.rs listesinde ve default.json'da olmalı; yoksa
    /// arayüzden çağrı "not allowed by ACL" ile sessizce düşer.
    #[test]
    fn every_command_is_allowed() {
        let lib = include_str!("lib.rs");
        let build = include_str!("../build.rs");
        let caps = include_str!("../capabilities/default.json");
        let mut n = 0;
        for part in lib.split("commands::").skip(1) {
            let name: String = part
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '_')
                .collect();
            if name.is_empty() || name == "page_video" {
                continue;
            }
            n += 1;
            assert!(build.contains(&format!("\"{name}\"")), "build.rs: {name}");
            let perm = format!("\"allow-{}\"", name.replace('_', "-"));
            assert!(caps.contains(&perm), "default.json: {perm}");
        }
        assert!(n > 10);
    }
}
