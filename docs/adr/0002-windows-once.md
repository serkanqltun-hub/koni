# ADR 0002: Platform sırası: önce Windows

- Durum: Kabul edildi (v0.2)

## Bağlam
Grubun çoğu Windows kullanıyor. Native video gömme her platformda ayrı iş ve macOS gömmesi (NSView `wid` / `mpv_render_context`) daha riskli.

## Karar
Faz 0–1 yalnızca Windows'u hedefler. macOS Faz 2'de eklenir, Linux talep olursa sonra gelir.

## Sonuçlar
- CI önce Windows'ta derler.
- macOS işi ayrı bir `VideoSurface` implementasyonu olarak sınırlanır ([0003](0003-videosurface.md)).
- Ayrıntı: [PLAN.md §10](../PLAN.md).
