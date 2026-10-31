# KONİ Santral

Oda/senkron sunucusu; `-db` ile hesaplar ve olay günlüğü (SQLite). Protokol: [docs/PROTOCOL.md](../docs/PROTOCOL.md).

```
go run ./cmd/koni-server -addr :8080                 # açık sunucu: kullanıcı adıyla girilir, hiçbir şey saklanmaz
go run ./cmd/koni-server -addr :8080 -db koni.db     # hesaplı: create_room/join oturum belirteci ister
go run ./cmd/koni-server invite -db koni.db [-ttl 168h]   # tek kullanımlık hesap davet kodu yazdırır
go run ./cmd/koni-server -reports ./reports          # hata bildirimleri bu dizine (varsayılan: -db'nin yanındaki reports/)
go test -race ./...
```
`ws://localhost:8080/ws`, `GET /healthz`. İlk açılan hesap admin olur (`users.is_admin`; henüz bir yetkisi yok). Davet kodu sunucu çalışırken de üretilebilir (SQLite WAL); veritabanı dosyasının sahibi olan kullanıcıyla çalıştır.

- `hub/` — `Room` oda başına tek goroutine aktör (state makinesi), `Hub` oda kayıt defteri, `server.go` WebSocket uç noktası, `auth.go` hesap el sıkışması (`register`/`login`/`logout`) ve IP başına deneme sınırı (10 dk'da 10, yalnızca bellekte).
- `store/` — SQLite (`modernc.org/sqlite`, cgo yok): `users`, `sessions` (belirteç SHA-256 ile saklanır, 90 gün kullanılmayan düşer), `invites` (SHA-256, tek kullanımlık, süreli), `events` (yalnızca ekleme; oda etkinlik satırları aynı `type` ile, sohbetin yalnızca uzunluğu). Şifreler argon2id (RFC 9106: t=3, m=64 MiB, p=4). Olaylar kanal üzerinden saniyede bir toplu yazılır, senkron yolunu bekletmez.
