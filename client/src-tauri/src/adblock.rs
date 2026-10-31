//! Keşfet tarayıcısı için basit reklam engelleyici: `adblock_hosts.txt`
//! listesindeki alan adlarına (ve alt alan adlarına) giden istekler WebView2'de
//! 403 ile kesilir; reklam pencereye hiç gelmez, ön reklam (preroll) oynamaz.

use std::collections::HashSet;
use std::sync::LazyLock;

static HOSTS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    include_str!("adblock_hosts.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
});

/// `host` ya da üst alan adlarından biri listede mi?
pub fn is_ad_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let mut h = host.as_str();
    loop {
        if HOSTS.contains(h) {
            return true;
        }
        match h.split_once('.') {
            Some((_, rest)) if rest.contains('.') => h = rest,
            _ => return false,
        }
    }
}

/// Açık sayfada engellenen istek sayısı (Keşfet adres çubuğundaki sayaç).
static BLOCKED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn blocked() -> u32 {
    BLOCKED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Yeni sayfa: sayaç sıfırlanır.
pub fn reset_blocked() {
    BLOCKED.store(0, std::sync::atomic::Ordering::Relaxed);
}

pub fn is_ad_url(url: &tauri::Url) -> bool {
    url.host_str().is_some_and(is_ad_host)
}

/// Windows: pencerenin tüm isteklerini (iframe ve worker dahil) süzer.
#[cfg(windows)]
pub fn install(win: &tauri::WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use webview2_com::WebResourceRequestedEventHandler;
    use windows::core::{Interface, HSTRING, PWSTR};

    let _ = win.with_webview(|pw| unsafe {
        let env = pw.environment();
        let Ok(wv) = pw.controller().CoreWebView2() else {
            return;
        };
        let filter = HSTRING::from("*");
        let added = match wv.cast::<ICoreWebView2_22>() {
            Ok(wv22) => wv22.AddWebResourceRequestedFilterWithRequestSourceKinds(
                &filter,
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            ),
            Err(_) => {
                wv.AddWebResourceRequestedFilter(&filter, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)
            }
        };
        if added.is_err() {
            return;
        }
        let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut uri = PWSTR::null();
            args.Request()?.Uri(&mut uri)?;
            let uri = webview2_com::take_pwstr(uri);
            let blocked = tauri::Url::parse(&uri).is_ok_and(|u| is_ad_url(&u));
            if blocked {
                BLOCKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let resp = env.CreateWebResourceResponse(
                    None,
                    403,
                    &HSTRING::from("Blocked"),
                    &HSTRING::new(),
                )?;
                args.SetResponse(&resp)?;
            }
            Ok(())
        }));
        let mut token = 0i64;
        let _ = wv.add_WebResourceRequested(&handler, &mut token);
    });
}

#[cfg(not(windows))]
pub fn install(_win: &tauri::WebviewWindow) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_subdomains_only_on_label_boundary() {
        assert!(is_ad_host("doubleclick.net"));
        assert!(is_ad_host("securepubads.g.doubleclick.net"));
        assert!(is_ad_host("IMASDK.googleapis.com."));
        assert!(!is_ad_host("notdoubleclick.net"));
        assert!(!is_ad_host("googleapis.com"));
        assert!(!is_ad_host("fonts.googleapis.com"));
        assert!(!is_ad_host("net"));
    }
}
