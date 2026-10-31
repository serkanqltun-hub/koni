# ADR 0003: `VideoSurface` platform soyutlaması

- Durum: Kabul edildi (v0.3)

## Bağlam
macOS'a geçerken UI, IPC ve WebSocket sözleşmelerinin değişmemesi gerekiyor. Video yüzeyinin yaşam döngüsü (oluşturma, resize, tam ekran, yok etme) platforma özgü.

## Karar
Rust çekirdeğinde platformdan bağımsız `VideoSurface` trait'i vardır. Windows implementasyonu `Win32ChildSurface` (`client/crates/koni-surface`); video kutusunun konumu JS roundtrip'i olmadan Rust'ta `WM_SIZE`/`WM_WINDOWPOSCHANGED`/`WM_DPICHANGED` mesajlarından hesaplanır. Svelte yalnızca seyrek bir layout config gönderir.

## Sonuçlar
- macOS için yalnızca yeni bir implementasyon (`MacSurface`) yazılır; Svelte, Tauri IPC ve WS protokolünde değişiklik olmaz (Faz 2 kabul kriteri).
- Pencere kenarından sürükleyerek boyutlandırmada (modal size-move döngüsü) video takılmadan pencereyi izler.
- Ayrıntı: [design/client.md §3.2–3.3](../design/client.md).
