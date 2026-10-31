# ADR 0001: Kokpit modeli: video ve web UI yan yana

- Durum: Kabul edildi (v0.2)

## Bağlam
Saydam webview'ı native video üstüne bindirmek her platformda kırılgan: WebView2 hosting modları, WKWebView saydamlığı ve compositor farkları.

## Karar
Video ve web UI **çakışmayan dikdörtgenlerdir**. Video sol/üst kutuda native bir child pencerede (Windows: HWND) mpv tarafından çizilir; Svelte UI yalnızca sağ kolon ve alt bardadır. Saydam overlay yoktur.

## Sonuçlar
- z-order/airspace çakışması yoktur. Kural: dropdown, tooltip ve modal'lar sağ kolona veya yukarı doğru alt bara açılır; tam pencere modal gerekiyorsa video geçici olarak gizlenir.
- Tam ekranda bildirimler mpv OSD'si (`show-text`) ile gösterilir.
- Ayrıntı: [design/client.md §3.1–3.2](../design/client.md).
