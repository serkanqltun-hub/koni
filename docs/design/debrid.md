# KONİ — Real-Debrid / Addon (opsiyonel)

> [PLAN.md](../PLAN.md)'nin parçası. Bölüm numaraları korunmuştur; kod ve dokümanlardaki `§4.4` gibi atıflar bu numaralara işaret eder.

## 8. Opsiyonel Faz: Real-Debrid / Addon Modu

Bu mod çekirdek akış oturana kadar ertelenir. Kısa tasarım notları:
- **Önerilen varsayılan: "herkes kendi RD anahtarı"**. İstemci, Stremio addon protokolüyle (`/stream/movie/tt….json`) önbellekli kaynakları listeler ve kendi RD hesabıyla **doğrudan** çözer. Santral yalnızca "hangi kaynak" bilgisini senkronlar. Proxy yok, VPS'ten içerik geçmez, hesap paylaşımı yok.
- **Paylaşımlı Boru (ileri, bilinçli tercih)**: Tek akışı herkese kopyalamak (fan-out) gerçekçi değil, çünkü mpv dosyanın farklı aralıklarını okur ve seek'ler bağımsızdır. Doğru model şudur: her izleyiciye **bağımsız range proxy** + upstream'de **singleflight chunk cache** (4 MiB chunk, oda başına 64–256 MiB RAM). 5–10 MB tampon 70 Mbps'te yalnızca ~1 sn eder. İmzalı, kısa ömürlü URL'ler kullanılır (HMAC: `streamId|userId|exp`). "Hiç diske yazmama" için swap kapatılır.
- **Bant genişliği**: 4K remux ~50–80 Mbps. 2 saatlik film kişi başı ~63 GB, 5 kişide ~315 GB/gece. Hetzner AB cloud planlarında ~20 TB/ay dahil trafik [güncel kota ve port hızı doğrulanmalı].

### 8.1 Uygulama: "herkes kendi RD anahtarı" (p0.15)

**Akış.** Keşfet → *Film & Dizi* (`client/src/lib/TitlesPane.svelte`):
1. Arama Cinemeta'ya gider (Stremio'nun resmî meta addon'u, anahtarsız): `catalog/{movie|series}/top/search=….json`, IMDb kimliği döner. Adı sorguyla eşleşen başlıklar öne alınır.
2. Dizide bölümler `meta/series/{id}.json`'dan gelir; kaynak isteği `tt…:sezon:bölüm`.
3. Kaynaklar **yalnızca kullanıcının eklediği** addon'lardan istenir (`/stream/{tür}/{id}.json`, paralel, 20 sn). KONİ hiçbir addon önermez ya da listelemez ([risks.md](risks.md)).
4. Seçilen kaynak açılır (`debrid_open`):
   - Odada yalnızca `media{kind:"debrid", src:infoHash, file, idx, title}` odaya gider.
   - Her üye (açan dahil) kendi anahtarıyla çözer: `addMagnet` → `selectFiles` → `downloaded` mi → `unrestrict/link` → mpv.
   - Tek başına izlerken aynı çözüm yerelde yapılır.

**Paylaşılabilir kaynak.** Yalnızca `infoHash` taşıyan akışlar odaya açılabilir.
- Addon'un kullanıcının kendi debrid hesabıyla çözdüğü `url` akışları da listelenir, ama soluk gösterilir ve açılamaz.
- Sebep: o adres kişiye özeldir; başkasının hesabını odaya taşır.
- Arayüz bu durumda addon'u debrid ayarı olmadan eklemeyi söyler.

**Dosya seçimi** (`debrid.rs` `pick_file`): addon'un verdiği dosya adı (`behaviorHints.filename`) → Stremio `fileIdx` (0 tabanlı; RD dosya kimliği 1 tabanlı) → en büyük video.

**Önbellek.** RD `instantAvailability` 2024'te kapatıldı; önbellek denetimi açılışta yapılır.
- `selectFiles` sonrası 6 sn içinde `downloaded` olmayan torrent silinir.
- Kullanıcıya "önbellekte değil, başka kaynak seç" denir.
- Kullanıcının hesabında aynı torrent zaten indirilmişse yeniden eklenmez (`/torrents?limit=100`).

**Gizlilik.**
- RD anahtarı işletim sisteminin anahtar zincirinde durur (`app.koni.desktop` / `real-debrid`).
- Addon adresleri (içinde yapılandırma olabilir) uygulama yapılandırma klasöründeki `addons.json`'da durur.
- İkisi de Santral'a gitmez.
- Sunucu `src`'yi 40 haneli hex olarak doğrular, `file`'ı dosya adına indirger (≤255 karakter), `idx`'i 0–100000 aralığında tutar.
