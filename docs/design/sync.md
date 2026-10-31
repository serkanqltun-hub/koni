# KONİ — Senkron

> [PLAN.md](../PLAN.md)'nin parçası. Bölüm numaraları korunmuştur; kod ve dokümanlardaki `§4.4` gibi atıflar bu numaralara işaret eder.

## 4. Senkron Protokolü (Go Santral)

### 4.1 Syncplay'den Alınanlar
Syncplay'de istemciler periyodik `State` (position, paused, doSeek, setBy) ve `ping` (latencyCalculation, RTT) gönderir. **`ignoringOnTheFly`** sayaçları, zorunlu bir değişikliğin onaylanmasına kadar gelen eski durumları yok sayar. Syncplay büyük sapmada geri sarar, küçük sapmada yavaşlatır [varsayılan eşikler doğrulanmalı]. KONİ bu modeli **sunucu-otoriter** hale getirir: gerçek oda state'i Santral'dadır, istemciler öneri gönderir. Syncplay'den bilinçli olarak ayrıldığı nokta şudur: **KONİ düzeltme amacıyla hiçbir zaman geri sarmaz** (§4.4).

### 4.2 Saat Senkronu
2–5 sn'de bir NTP benzeri ölçüm yapılır: `offset = ts − (t0+t1)/2`, `rtt = t1 − t0`. Son N örnekten en düşük RTT'lilerin medyanı alınır. Tipik ev bağlantısında birkaç ms ile ~10 ms arası hassasiyet gerçekçidir.

### 4.3 Oda Durum Modeli
```
position(t) = anchorPos + (paused ? 0 : (t − anchorServerTime) × rate)
```
Her play/pause/seek yeni bir anchor ve `seq` üretir. İstemciler ~1 sn'de bir gerçek `time-pos`, buffer ve indirme durumunu raporlar. Bu raporlar sağ paneldeki "+12 ms" göstergesini besler. Senkron yalnızca seek edilebilir timeline'larda (VOD, doğrudan dosya, VOD m3u8) yapılır. Canlı akış algılanırsa (`duration` yok veya `seekable=no`) oda "senkron desteklenmiyor" uyarısı verir.

### 4.4 Drift Düzeltme ve Anti-Rubberbanding
- Hedef: **medyan < 50 ms, p95 < 100 ms**. İnternet üzerinden mutlak kare-kare senkron garanti edilemez.
- **Temel kural**: İstemci drift'i düzeltmek için **asla geri seek yapmaz**. Diyalog veya kare tekrarı yasak. Geri seek yalnızca bir kullanıcının bilinçli timeline scrub'ında (ve o da herkese uygulanan yeni anchor olarak) olur.
- **Planlı başlangıç**: Santral `startAt = now + RTT_p95 + 250 ms` yayınlar. İstemciler `seek exact` + pause yapar, buffer kapısını geçince `ready` gönderir ve sunucu saatiyle `startAt` anında oynatır.
- **Düzeltme tablosu** (`sapma = yerelPos − odaPos`, 250–500 ms'de bir değerlendirilir):
  | Sapma | Eylem |
  |---|---|
  | \|sapma\| < 50 ms | Yok (`speed = 1.0`) |
  | 50 ms – 3.0 s **geride** | `speed` 1.0 → en fazla **1.04**, orantısal, `audio-pitch-correction=yes` |
  | 50 ms – 3.0 s **önde** | `speed` 1.0 → en az **0.96**, aynı şekilde |
  | > 3.0 s **önde** (sabit eşik, v0.3.1) | **Yerel hold**: istemci geri seek etmez. Kendi pause/hold'unda bekler, oda konumu kendi konumuna ulaşınca (± 50 ms) sunucu saatiyle yeniden başlar. Yalnızca o istemci etkilenir |
  | > 3.0 s **geride** | **3–30 s: Freeze over rewind.** Kimse geri sarılmaz. Santral odayı `hold` durumuna alır (diğer peer'ler kendi konumlarında durur), geride kalan istemci oynayarak yetişir, sonra senkron geri sayımla herkes devam eder. **> 30 s: ileri seek.** Oda durmaz, geride kalan istemci oda konumuna **ileri seek** ile atlar (aradaki kısmı kaçırır, geri sarma yasağına uyar). Etkinlik akışında "Furkan 42 s ileri atladı" bildirimi çıkar. *(Karar: 2026-10-29, Serkan.)* |
  | Buffer boş (stall) | §4.7 histerezis kapısı |
- Hızın 1.0'a dönüşü yumuşak bir rampayla yapılır (orantısal kontrol + küçük sapmada sönümleme). Böylece hızdaki gidip gelme (osilasyon) kulakla duyulmaz.

### 4.5 Senkron State Makinesi
**Oda (Santral)**:
```
PAUSED ──play──▶ COUNTDOWN ──startAt──▶ PLAYING
PLAYING ──pause──▶ PAUSED
PLAYING ──scrub(seek)──▶ COUNTDOWN            (herkes yeni anchor'a exact seek)
PLAYING ──stall / lag>3.0s(geride)──▶ HOLD(reason, waitingFor[])
HOLD ──bekleneni hazır (rezerv ≥ 12 s / yetişti)──▶ COUNTDOWN
HOLD ──kronik stall + host "onsuz devam"──▶ COUNTDOWN (bekleneni DETACHED yapar)
```
**İstemci (yerel)**: `SYNCED` ↔ `CORRECTING` (hız ≠ 1.0) · `AHEAD_HOLD` (> 3.0 s önde, yerel bekleme) · `CATCHING_UP` (oda HOLD'da, bu istemci oynuyor) · `STALLED` → `REFILLING` (rezerv doluyor) · `DETACHED` (arka planda tamponluyor, odayı takip etmiyor) → kullanıcı "Odayla Senkronla" der → ileri seek ile `SYNCED`.
### 4.6 Mesaj Şeması (JSON / WebSocket)
Kesin sözleşme [PROTOCOL.md](../PROTOCOL.md)'dir (şu an `p0.3`); bu bölüm yalnızca tasarım notlarını tutar.
- HOLD ve geri sayım ayrı mesaj tipleri değil, `state` mesajının `mode` alanıdır (`hold`, `countdown`; `holdPos`, `waitingFor`, `reason`, `startAt` alanlarıyla). Bekleme göstergesi için `buffer_status` vardır.
- Faz 1'de eklenenler: `cmd op=media` + `state.media` (p0.2); `detach_offer` / `detached`, `cmd op=continue_without | resync`, `resume` + hayalet üye, `chat` (p0.8'de kaldırıldı), `leave` (p0.3).
- `cid` → `ack` eşleşmesi, istemcinin kendi yankısını tanımasını sağlar (ignoringOnTheFly'ın karşılığı). İstemciler süre raporlar; uyuşmazlıkta "farklı dosya/sürüm" uyarısı çıkar. `AHEAD_HOLD` yalnızca yerel bir durumdur, odayı etkilemez; `report.state = "ahead_hold"` ile görünür olur.

### 4.7 Buffering ve Stall: Histerezis Kapısı
- **Stall algılama**: `paused-for-cache=yes` veya `demuxer-cache-duration < 0.5 s` iken oynatma ilerlemiyorsa istemci `stalled` raporlar. Santral odayı `HOLD(stall)` durumuna alır.
- **Histerezis**: İlk paket gelir gelmez devam **edilmez**. Stall eden istemci **12 s güvenlik rezervi** (v0.3.1'de sabitlendi) tamponlayana kadar geri sayım başlamaz. Alt eşik (~0.5 s) ile üst eşik (12 s) arasındaki fark, "oynat-dur-oynat" salınımını önler. Dosyanın sonuna yakınsa rezerv yerine `cache-end ≥ duration` koşulu yeterlidir.
- **Buffer yüzdesi**: `bufferPct = min(100, demuxer-cache-duration / targetSec × 100)`. mpv'nin `cache-buffering-state` (paused-for-cache sırasında 0–100) değeri yardımcı göstergedir. Ayrıntı için `demuxer-cache-state` alt alanları (`cache-end`, `fw-bytes`) kullanılır [property semantiği doğrulanmalı].
- **UI**: Sağ panelde kişinin satırında durum göstergesi (⏳ ve bar), alt barda/ekranda **"Furkan bekleniyor... [Buffer: %65]"** rozeti, tam ekranda OSD.
- **Kronik kopma fallback'i**: Aynı istemci kısa bir pencerede tekrar tekrar stall ederse (varsayılan **4 dk içinde 3 stall**), host'a sağ panelde satır içi **[Furkan olmadan devam et]** düğmesi çıkar. Seçilirse:
  - Furkan `DETACHED` olur. Oda onu beklemeden devam eder.
  - Furkan'ın istemcisi arka planda tamponlamaya devam eder, ekranında "Odadan geride: −00:42 · Buffer %100 · [Odayla Senkronla]" görünür.
  - Furkan stabil olunca **"Odayla Senkronla"**'ya manuel basar. Bu bir **ileri seek**'tir: Furkan geride olduğu için geri sarma kuralı çiğnenmez, o istemci odanın güncel konumuna atlar ve `SYNCED` olur. Kendiliğinden yeniden bağlanmaz, çünkü yine stall edip odayı durdurma riski vardır.

### 4.8 Oda, Roller, Yeniden Bağlanma
- Oda: kalıcı isimli odalar (ör. "Cuma Gecesi"), üyelik listesi profillere bağlı. Roller: Host, Co-host, Üye. Politika: `free` / `host_only`. `host_only`'de üyeler "durdurma iste" butonunu kullanır.
- Rate limit: kullanıcı başına ≤ 3 kontrol komutu/sn.
- Yeniden bağlanma: üstel geri çekilme (0.5→8 sn), `resume {lastSeq}`. Santral tam snapshot gönderir. İstemci §4.4 tablosuna göre hizalanır (geride ise ≤ 30 s freeze, > 30 s ileri seek, önde ise hold, geri seek asla). Kullanıcı 60 sn "hayalet" kalır, host yetkisi hemen düşmez.

### 4.9 Neden Go
Oda başına tek goroutine aktör modeli kurulur, kilit karmaşası olmaz. Tek statik binary ve düşük RAM. `coder/websocket` + `modernc.org/sqlite` (CGO'suz) veya `mattn/go-sqlite3`. Python prototip için hızlıdır ama Go dağıtımı ve eşzamanlılığı daha basittir.
