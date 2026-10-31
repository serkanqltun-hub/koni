# KONİ — Kullanıcı Modeli ve İstatistikler

> [PLAN.md](../PLAN.md)'nin parçası. Bölüm numaraları korunmuştur; kod ve dokümanlardaki `§4.4` gibi atıflar bu numaralara işaret eder.

## 5. Kullanıcı Modeli ve İstatistikler

### 5.1 Kimlik: Sıfır PII (karar verildi)
- **Model**: Tek kullanımlık **davet token'ı** + kullanıcı adı + **argon2id** şifre, merkezi SQLite (Santral). E-posta, telefon, gerçek isim veya üçüncü taraf kimliği istenmez ve saklanmaz. Kullanıcı adı ve avatar kişinin kendi seçimidir.
- **Akış**: Admin (Serkan) tek kullanımlık davet linki üretir (`koni://invite/<token>` veya kopyala-yapıştır kod, son kullanma tarihli). Kişi kayıt olur, şifre `argon2id` ile saklanır. Girişte opak, rastgele bir **oturum token'ı** (256 bit) verilir. Token sunucuda hash'lenmiş tutulur, istemcide işletim sistemi anahtar zincirine yazılır (`keyring` crate → Windows Credential Manager / macOS Keychain). Şifre sıfırlama admin tarafından yeni davet/sıfırlama token'ıyla yapılır.
- IP adresleri yalnızca rate limit için bellekte tutulur, kalıcı olarak loglanmaz.
- **v1 dışı**: Discord OAuth ve diğer üçüncü taraf girişler.
- **Faz 1 uygulaması (PROTOCOL p0.5)**: Kayıt/giriş ayrı bir HTTP API değil, aynı WebSocket uç noktasında kısa bir el sıkışmadır (`register`/`login`/`logout` → `auth`). Davet kodu şimdilik yalnızca sunucuda `koni-server invite -db …` ile üretilir (admin arayüzü yok); `koni://invite` bağlantısı yok, kod kopyala-yapıştır. Şema `users`, `sessions`, `invites`, `events` ile başladı; diğer tablolar ihtiyaç olunca eklenecek. `display_name`/`avatar` henüz yok. Şifre sıfırlama: admin yeni davet verir, kişi yeni hesap açar (eski hesabın olayları eski kullanıcıda kalır) [onay gerekli].

### 5.2 SQLite Şema Taslağı
| Tablo | Alanlar (özet) |
|---|---|
| `users` | id, username (unique), display_name, pass_hash, avatar, created_at, is_admin |
| `sessions` | token_hash, user_id, created_at, last_seen, device_label |
| `invites` | token_hash, created_by, used_by, expires_at |
| `rooms` | id, name, owner_id, policy, created_at |
| `media` | id, kind (web/local/music/download), url_or_hash, title, duration, tmdb_id? |
| `watch_sessions` | id, room_id, media_id, started_at, ended_at |
| `presence_intervals` | id, user_id, watch_session_id, from_ts, to_ts, mode (video/music) — izleme süresi buradan |
| `events` | id, ts, room_id, user_id, type (pause/play/seek/join/leave/sub_shift/dl_done/stall/hold/detached/resync…), media_id, payload JSON — **append-only** |
| `proposals` | id, room_id, user_id, source (swipe/wheel/queue), media_ref, outcome (picked/rejected/expired), created_at |
| `votes` | proposal_id, user_id, vote (like/dislike), ts |
| `user_stats` | user_id, period (all/2026-10/week), watch_sec, listen_sec, pauses, seeks, proposals_picked, proposals_rejected, … — özet tablo |
| `badges` / `user_badges` | badge_id, title, rule · user_id, badge_id, period, awarded_at |

### 5.3 Event Log → Agregasyon
- Santral'ın oda aktörü her state değişimini zaten işler. Aynı noktada `events` tablosuna **asenkron batch insert** yapılır (kanal → saniyede bir toplu yazım). Senkron yolunu yavaşlatmaz.
- **Uygulanan (p0.9)**: İzleme süresi oda aktöründe sayılır: oda `PLAYING` iken bağlı her üyeye (DETACHED dahil, hayalet hariç) süre eklenir; üye ayrılınca ve 5 dakikada bir `watch` olayı (`ms`, `night` = 00:00–04:00 TSİ payı, `session` = o odadaki kesintisiz toplam) yazılır. `presence_intervals` ve `user_stats` tabloları yok: `stats` isteği (PROTOCOL §3.0a) dönem toplamlarını `events` üzerinde tek `GROUP BY` ile anında hesaplar (`server/store/stats.go`). Ölçek büyürse özet tabloya geçilir.
- **Uygulanan (p0.12)**: Oda medyası Lounge kuyruğundan geldiyse geçen süre `watch` yerine `listen` olayına (`ms`) yazılır. İzleme süresi, gece payı ve oturum uzunluğu bu süreden etkilenmez. `stats` cevabında `listenSec` olarak döner.
- Rozetler aynı istekte bu toplamlardan hesaplanır (dönem lideri; eşitlikte paylaşılır, sıfır değer kazanmaz). Kalıcı `user_badges` yok.
- **Görünürlük (p0.16, Serkan 2026-10-30)**: İstatistikler ve rozetler yalnızca **birbirini arkadaş ekleyen** hesaplar arasında görünür (`friends` tablosu: `user_id → friend_id`, tek yönlü kayıt; iki yön de varsa karşılıklı). Aynı odada bulunmak yetmez; tek yönlü ekleme de yetmez. Rozetler çağıran ile karşılıklı arkadaşları arasında hesaplanır. Arkadaşlıktan çıkarmak paylaşımı iki yönde de bitirir. Eskiden (p0.9–p0.15) metrikler oda arkadaşlarına açıktı. Kullanıcı hesabını ve verilerini tamamen sildirebilir.

### 5.4 Rozet Örnekleri
| Rozet | Kural |
|---|---|
| 🍿 **Cips Molası Şampiyonu** | Dönemde en çok `pause` yapan ✅ |
| ⏩ **Seek Manyağı** | En çok seek yapan (`seek` + `skip`) ✅ |
| 🦉 **Gece Kuşu** | 00:00–04:00 arası en çok izleme süresi ✅ |
| 🎯 **Zevk Diktatörü** | Çarkta önerisi en çok seçilen (`proposal` olayı, `won`) ✅ |
| 🗑️ **Reddedilen Dahi** | Çarkta önerisi en çok elenen ✅ |
| ❤️ **Kolay Beğenen** | Swipe'ta en yüksek "like" oranı, en az 10 oy (`vote` olayı) ✅ |
| 🐢 **Son Dakikacı** | İndirme bariyerinde en sık son hazır olan (üye başına hazır olma olayı henüz yazılmıyor) |
| 📡 **Modem Sesi** | Dönemde en çok stall/HOLD tetikleyen (`hold` olayı) ✅ |
| ⚡ **Fiber Lord** | En hızlı ortalama indirme (indirme hızı henüz yazılmıyor) |
| 🎧 **DJ Koltuğu** | Lounge'da en çok kuyruğa şarkı ekleyen (`queue_add` olayı) ✅ |
| 🏃 **Maratoncu** | Tek oturumda 4+ saat ✅ |

✅ = sunucuda hesaplanıyor, ana ekranın İstatistikler kartında görünür.

Hub/Lobi'deki istatistik kartları: "Bu ay 23 saat birlikte izlediniz", "En çok izlenen tür: bilim kurgu", "Serkan'ın önerileri %62 kabul", kişi başı haftalık izleme çubuğu.
