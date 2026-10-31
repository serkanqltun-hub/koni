# koni-sync

KONİ istemci senkron motoru. Saf Rust, platformdan bağımsız; mpv'ye `Player` trait'i üzerinden bağlanır.
Kurallar docs/design/sync.md §4.4–4.7 ve ADR 0004'ten, mesajlar docs/PROTOCOL.md (p0.1) sözleşmesinden gelir.

| Sapma (yerel − oda) | Eylem |
|---|---|
| < 50 ms | hız 1.0 |
| 50 ms – 3.0 s | hız 0.96–1.04, orantısal + yumuşak rampa |
| > 3.0 s önde | yerel bekleme (AheadHold), geri seek yok |
| 3–30 s geride | CatchingUp, geri seek yok |
| > 30 s geride | ileri seek + `skippedSec` |
| stall | 12 s rezerv dolmadan `ready` yok; 4 dk'da 3 stall → kronik |

Geri seek yalnızca bilinçli scrub'da (`cause: "seek"`) yapılır; her seek tek bir korumadan geçer ve
`Telemetry::backward_correction_seeks_blocked` sayacı 0 kalmalıdır.

```
cargo test     # birim testler + tests/scenarios.rs (simüle ağ, saat, oynatıcı, sunucu)
```
