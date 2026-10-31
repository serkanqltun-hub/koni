# KONİ — WebSocket Protokolü (Faz 0)

> Sürüm: **p0.19** (taslak) · Kaynak: [design/sync.md §4](design/sync.md), hesaplar [design/accounts.md §5.1](design/accounts.md), keşif [design/discovery.md §7](design/discovery.md)
> Kapsam: hesap (davet kodu + kullanıcı adı + argon2id şifre, oturum belirteci), oda oluştur/katıl, play/pause/seek, oda medyası (`media`), HOLD/countdown, Host Only + yardımcı host, ping/saat senkronu, yeniden bağlanma (`resume`), kronik stall → "olmadan devam et" / DETACHED, "Herkes İndirsin" bariyeri.
> Henüz yok: admin araçları.
>
> p0.2: `cmd op=media`, `state.media`, `denied.reason=bad_media`, `activity.kind=media`.
> p0.3: `resume`, `joined.resume|resumed|protocol`, `chat`, `leave`, `member_away|member_back|member_detached`, `detach_offer`, `detached`, `cmd op=continue_without|resync`, `members[].away|detached`, `denied.reason=bad_chat`, `error.code=resume_failed`, 20 sn okuma zaman aşımı. Kopan üye artık hemen düşmez (60 sn hayalet). Yalnızca ekleme; eski istemciler yeni mesajları yok sayar.
> p0.4: `media.kind=download` + `media.task`, `dl_progress`, `dl_status`, `snapshot.downloads`, `state.cause=download_ready`, `activity.kind=download`, `member_left.host`. Host düşünce yeni host bağlı üyelerden seçilir.
> p0.5: `register`, `login`, `logout`, `auth`, `create_room.auth`, `join.auth`, `error.code=auth_required|bad_token|bad_code|bad_password|username_taken|bad_login|rate_limited|no_accounts|server_error`. Hesaplı sunucuda (`-db`) `create_room`/`join` oturum belirteci ister ve üye adı hesabın adıdır; açık sunucuda (`-db` yok) her şey p0.4 gibidir.
> p0.6: yardımcı host (co-host): `cmd op=set_cohost` (`user`, `on`), `member_role`, `members[].cohost`, `activity.kind=role`. `host_only`'de yardımcı hostlar da kontrol eder, `continue_without` yapabilir ve `detach_offer` alır; host ayrılınca bağlı yardımcı host önceliklidir.
> p0.7: `media.referer` (yalnızca `kind: "url"`): Sanal Yakalayıcının sayfadan yakaladığı doğrudan akışın geldiği sayfa. Yalnızca ekleme; p0.6 sunucusu alanı düşürür (akış Referer'sız yüklenir).
> p0.9: `media.subs` (yalnızca `kind: "url"` + `referer`): yakalayıcının sayfada gördüğü altyazı dosyaları, `[{url, lang?, title?}]`, en fazla 16. Yalnızca ekleme; p0.8 sunucusu alanı düşürür (altyazı yalnızca yakalayanda görünür).
> p0.8: sohbet kaldırıldı: `chat` (istemci ve sunucu) ve `denied.reason=bad_chat` yok. p0.7 istemcisinin gönderdiği `chat` artık `error` (`bad_message`) alır; bağlantı açık kalır.
> p0.9: `stats` (§3.0a) → `stats` cevabı, `error.code=bad_period`. Yalnızca ekleme.
> p0.10: "Ne izlesek?" (§3.9, §4.12–4.13): `discover`, `swipe`, `propose`, `unpropose`, `spin`, `wheel_clear` → `moods`, `deck_loading`, `deck`, `deck_progress`, `match`, `discover_failed`, `wheel`. Altyazı (§3.10, §4.14): `subs_search`, `subs_pick` → `subs_results`, `subtitle`. Hata satırı `fun_error` (§4.15). Yalnızca ekleme; p0.9 sunucusu bu tiplere `error` (`bad_message`) döner.
> p0.11: Lounge kuyruğu (§3.11, §4.16): `queue_add`, `queue_vote`, `queue_remove`, `queue_next`, `track_end` → `queue`. Kuyruktan geçişte `state.cause: "media"` (by yok) ve `activity.kind: "queue_add"`. Yalnızca ekleme.
> p0.14: hızlı tepki `react` → `reaction` (§3.13, §4.18); `member_status.rttMs`; hata bildirimi `bug_report` → `bug_report_ok` (§3.0b), `error.code=too_large|no_reports`; ilk mesajın okuma sınırı 256 KiB. Bir medyanın ilk başlatılışında geri sayım en az 3 sn (§5); `state`'e alan eklenmedi. Yalnızca ekleme; p0.13 sunucusu `react`/`bug_report`'a `error` (`bad_message`/`not_joined`) döner.
> p0.16: arkadaşlar `friends` (§3.0c) → `friends` cevabı, `error.code=unknown_user|self_friend`. `stats.users` artık oda arkadaşlarını değil yalnızca karşılıklı arkadaşları içerir (§3.0a). p0.15 sunucusu `friends`'e `error` (`not_joined`) döner.
> p0.15: `media.kind=debrid` + `media.file`, `media.idx` (§3.3, [design/debrid.md §8.1](design/debrid.md)). Yalnızca ekleme; p0.14 sunucusu `debrid`'e `denied` (`bad_media`) döner.
> p0.17: "Ne izlesek?" oturumu (§3.9a, §4.12a): `night_start`, `night_ready`, `night_preview`, `night_suggest`, `night_search`, `night_detail`, `night_pick`, `night_unpick`, `night_done`, `night_close` → `night`, `night_previews`, `night_suggestions`, `night_results`, `night_detail`; dizi desteği; kartlara `kind`, `seasons`, `votes`, `director`, `cast`, `countries`. Yalnızca ekleme; p0.16 sunucusu bu tiplere `error` (`bad_message`) döner.
> p0.18: hesap adı artık `Ad#1234` (§3.0d); `auth` cevabına `tag`, `avatar`; `register.avatar`. Profil `profile` (§3.0d), üye davet kodu `ABCD-EFGH` (onunla kaydolan, davet edenle karşılıklı arkadaş olur), profil fotoğrafı `GET /avatars/<id>`. Lobi `hello` → `presence` (§3.0e). Oda içinde `call` → `called` (§3.0e). Hesaplı sunucuda odaya yalnızca çağrılan ya da içerideki birinin karşılıklı arkadaşı girer: `error.code=not_invited`; ayrıca `bad_avatar|too_many_invites|not_friends`. `friends` listesinde `name` artık `Ad#1234`, `requests` nesne listesi.
> p0.19: adminler (§3.0f): `admin` el sıkışması (odalar, üyeler, oda kapatma, ban, davet hakkı), `profile op=upload` (admin görseli), `profile.admin|canInvite|taken`, `hello.admin`, `members[].admin`, arkadaş/presence satırlarında `admin`; `error.code=forbidden|room_closed|banned|no_invites|avatar_taken`. Yalnızca ekleme.
> p0.22: "Açık odalar" (§3.0e): `presence.rooms` (girebileceğin açık odalar: senin çıktığın ve karşılıklı arkadaşının içinde olduğu), `presence.friends[].invite`; bir odada bulunmuş hesap oda açık kaldıkça çağrısız geri girer. Oda içinde `close_room` (yalnızca host) → herkese `error` (`room_ended`); host değilse `denied` (`not_host`). Tek seferlik `feed` (§3.0e) → ana ekran "Etkinlik" sekmesi. `watch` olayına `title`. Yalnızca ekleme.
> p0.23: profil listeleri (§3.0g): tek seferlik `lists` el sıkışması (İzleyeceklerim / İzlediklerim, arkadaşın listeleri, ortak liste, künyede kimler istiyor). Odada `debrid` film (`file` = `movie:tt…|…`) %80'i geçince odadakilerin İzlediklerim'ine kiminle izlendiğiyle girer. `night_pick` artık `itemId` yerine `imdb` (`tt…`) da alır (listeden öneri). `error.code=bad_entry`. Yalnızca ekleme.
>
> p0.13: oda modu: `cmd` `set_view` (§3.3), `view` mesajı (§4.17), `snapshot.view` (§4.1). Yalnızca ekleme; p0.12 sunucusu `set_view`'a `denied` (`bad_op`) döner, istemci `view` yoksa Sinema varsayar.
>
> p0.12: Lounge dosya relay'i (§3.12, HTTP `POST /relay`, `GET /relay/<id>/<ad>`), `stats.users[].listenSec` (§3.0a) ve `listen` olayı. Yalnızca ekleme; p0.11 sunucusunda `/relay` 404 döner.

## 1. Taşıma

- Uç nokta: `GET /ws` (WebSocket upgrade). Üretimde Caddy arkasında `wss://`.
- Her WS metin çerçevesi **tek bir JSON nesnesidir**; ayırt edici alan `t` (mesaj tipi).
- Sağlık kontrolü: `GET /healthz` → `200 ok`.
- Tüm **zamanlar** sunucu duvar saatidir, Unix epoch **milisaniye** (`int64`). Tüm **konumlar** medya süresi, **saniye** (`float64`).
- Bilinmeyen alanlar yok sayılır (ileri uyumluluk). Bilinmeyen `t` için `error` (`bad_message`) döner, bağlantı kapanmaz.
- Çerçeve boyutu sınırı: 8 KiB (yalnızca bağlantının ilk mesajında 256 KiB, `bug_report` için). Aşan bağlantı kapatılır.

## 2. Bağlantı yaşam döngüsü

1. İstemci WS açar ve ilk mesaj olarak `create_room`, `join` veya `resume` gönderir (5 sn içinde, yoksa kapatılır). Hesap işlemleri (`register`, `login`, `logout`, §3.0), `stats` (§3.0a) ve `bug_report` (§3.0b) ayrı ve kısa bir bağlantıdır: tek cevap, sonra sunucu kapatır.
2. Sunucu `joined` ile cevap verir (tam anlık görüntü dahil). Aynı anda diğer üyelere `member_joined` (resume'da `member_back`) gider.
3. Sonrasında `cmd`, `report`, `ping`, `leave` serbesttir.
4. Sunucu her okumada **20 sn** bekler; bu sürede hiç çerçeve gelmezse bağlantıyı koparır (istemci ~1 sn'de bir `report`/`ping` gönderdiği için normalde olmaz).
5. Bağlantı koparsa üye **hayalet** olur: odada kalır, diğerlerine `member_away` gider, HOLD'da bekleniyorsa `waitingFor`'dan düşer. 60 sn içinde `resume` ile dönerse aynı üye (aynı id, host yetkisi, DETACHED durumu) devam eder; dönmezse `member_left` ile düşer.
6. `leave` gönderen üye hemen `member_left` ile düşer ve sunucu bağlantıyı kapatır. Oda boşalınca durdurulur (PAUSED) ve 6 saat açık kalır: aynı davetle ilk giren host olur. 6 saat kimse gelmezse bellekten silinir.

İstemci (src-tauri) davranışı: 8 sn hiçbir mesaj gelmezse bağlantıyı ölü sayar, 0,5→8 sn artan aralıkla 70 sn boyunca `resume` dener; `resume_failed` gelirse davet koduyla yeniden `join` olur (yeni üye). UI'a `sync://link` (`online` | `reconnecting`) yayınlar.

## 3. İstemci → Sunucu

### 3.0 Hesap: `register` / `login` / `logout`
```json
{ "t": "register", "code": "-AzDs2Ykhexelo2r", "username": "Serkan", "password": "…", "device": "KONİ windows" }
{ "t": "login", "username": "serkan", "password": "…", "device": "KONİ windows" }
{ "t": "logout", "auth": "<oturum belirteci>" }
```
Cevap: `{ "t": "auth", "token": "<oturum belirteci>", "name": "Serkan" }` (logout'ta `token` boş) ya da `error`; ardından bağlantı kapanır.
- `code`: tek kullanımlık hesap davet kodu (oda davetinden farklıdır). Sunucuda `koni-server invite -db …` ile üretilir, varsayılan 7 gün geçerlidir. Kullanılmış, süresi dolmuş ya da bilinmeyen kod → `bad_code`. İlk açılan hesap admin olur.
- `username`: 2–24 karakter, sunucuda benzersiz (büyük/küçük harf duyarsız) → `bad_username` | `username_taken`. `password`: 8–128 karakter → `bad_password`. Yanlış ad/şifre → `bad_login` (bilinmeyen adla aynı süre).
- `device`: oturum listesi için serbest etiket (en fazla 64 karakter). İstemci yalnızca işletim sistemini yazar.
- Belirteç 256 bit rastgeledir; sunucu yalnızca SHA-256'sını saklar, 90 gün kullanılmazsa geçersizleşir. İstemci onu işletim sistemi anahtar zincirinde sunucu adresine göre tutar.
- IP başına 10 dakikada 10 `register`/`login` denemesi → `rate_limited`. Hesapsız sunucuda → `no_accounts`.

### 3.0a `stats` — istatistik ve rozetler
```json
{ "t": "stats", "auth": "<oturum belirteci>", "period": "week" }
```
Cevap (ardından bağlantı kapanır):
```json
{ "t": "stats", "period": "week", "you": "Serkan",
  "users": [{ "user": "Serkan", "watchSec": 5400, "nightSec": 1800, "longestSec": 3600, "pauses": 4, "seeks": 2, "holds": 1, "listenSec": 900 }],
  "badges": [{ "id": "pause", "title": "🍿 Cips Molası Şampiyonu", "rule": "En çok durduran", "users": ["Serkan"] }] }
```
- `period`: `week` (pazartesi 00:00), `month` (ayın 1'i) ya da `all`; sınırlar Türkiye saatidir. Başka değer → `bad_period`. Belirteç yok → `auth_required`; hesapsız sunucuda → `no_accounts`.
- `users`: çağıran ve karşılıklı arkadaşları (§3.0c; ikisi de birbirini eklemiş olmalı), yoksa yalnızca çağıran. `badges[].users` boş olabilir; rozet kuralları [design/accounts.md §5.4](design/accounts.md).
- İzleme süresi sunucuda sayılır: oda `PLAYING` iken bağlı (hayalet olmayan) her üyeye; DETACHED üye de sayılır. Ayrılınca ve 5 dakikada bir `watch` olayı (`ms`, `night`, `session`) yazılır. Oda medyası Lounge kuyruğundan geldiyse süre izlemeye değil dinlemeye (`listen` olayı, `ms`) yazılır ve `listenSec` olarak döner.

### 3.0b `bug_report` — hata bildirimi
```json
{ "t": "bug_report", "auth": "<oturum belirteci>", "username": "Serkan", "version": "0.0.9", "text": "<log metni>" }
```
Cevap `{ "t": "bug_report_ok", "id": "20261029-204500-serkan.txt" }` ya da `error`; ardından bağlantı kapanır.
- Hesaplı sunucuda `auth` zorunlu (yoksa `auth_required`, geçersizse `bad_token`); rapor hesabın adıyla yazılır, `username` yok sayılır. Açık sunucuda `username` kullanılır (geçersiz ya da boşsa `anonim`).
- `text` en çok 200 KB (bayt) → `too_large`. `version`: istemci sürümü, tek satıra indirilir, en çok 32 karakter.
- IP başına saatte 5 rapor → `rate_limited` (yalnızca kabul edilen raporlar sayılır).
- Sunucu raporu `-reports` dizinine düz metin dosyası olarak yazar (bayrak boşsa `-db`'nin yanındaki `reports/`; ikisi de yoksa özellik kapalı → `no_reports`). Dosya başında `Kullanıcı`, `Sürüm`, `IP`, `Zaman` (Türkiye saati) satırları, bir boş satır, sonra `text`. Dosya adı Türkiye saatiyle `YYYYAAGG-SSddss-<ad>.txt`; ad küçük harfe çevrilir, Türkçe harfler ASCII'ye iner, `a–z 0–9 - _` dışı atılır. Ad doluysa `-2`, `-3`… eklenir. `id` bu dosya adıdır. Yazılamazsa `server_error`.

### 3.0c `friends` — arkadaş listesi
```json
{ "t": "friends", "auth": "<oturum belirteci>", "op": "add", "user": "Berk" }
```
Cevap (ardından bağlantı kapanır):
```json
{ "t": "friends", "friends": [{ "name": "Berk", "mutual": true }], "requests": ["Cem"] }
```
- `op`: `list` (varsayılan), `add`, `remove` ya da `decline` (p0.18: sana gelen isteği sil); başka değer → `bad_message`. `user`: hesap adı (büyük/küçük harf duyarsız). Böyle hesap yok → `unknown_user`, kendin → `self_friend`. Tekrar eklemek ya da eklenmemiş birini çıkarmak hata değildir.
- `friends`: eklediklerin; `mutual: true` ise o da seni eklemiş ve istatistikleriniz birbirinize görünür (§3.0a). `requests`: seni ekleyen ama senin eklemediğin hesaplar.
- Belirteç yok → `auth_required`, geçersiz → `bad_token`; hesapsız sunucuda → `no_accounts`.

### 3.0d `profile` — profil, fotoğraf, davet kodu (p0.18)
Hesap adı kayıtta seçilir, değişmez; sunucu sonuna 4 haneli bir etiket ekler (`Serkan#3124`). Aynı ad birden çok kişide olabilir; ad+etiket benzersizdir. Etiketsiz ad yalnızca o ad tekse bulunur (giriş ve `friends.user` için).
```json
{ "t": "profile", "auth": "<oturum belirteci>", "op": "invite" }
```
Cevap (ardından bağlantı kapanır):
```json
{ "t": "profile", "name": "Serkan", "tag": "3124", "avatar": "av01", "avatars": ["av01", "av02"],
  "invites": [{ "expires": 0, "usedBy": "Ayşe#0412", "avatar": "av02" }], "daily": [0, 3600, 0, 0, 1200, 0, 5400], "code": "ABCD-EFGH" }
```
- `op`: `get` (varsayılan), `avatar` (+ `avatar`: listedeki bir kimlik, yoksa `bad_avatar`) ya da `invite` (yeni kod `code` alanında; açık kod en fazla 5, 7 gün geçerli, fazlası → `too_many_invites`).
- Davet kodu büyük/küçük harf ve tire olmadan da yazılabilir. Üyenin koduyla kaydolan, davet edenle karşılıklı arkadaş olur.
- `invites`: son 10 kod (kodun kendisi saklanmaz, yalnızca bitiş ve kimin kullandığı).
- `daily`: son 7 günün izleme saniyesi, bugün en sonda.
- Fotoğraflar yalnızca sunucunun klasöründen seçilir (`-avatars`, varsayılan `-db` yanında `avatars/`). Dosya: `GET /avatars/<id>` + `Authorization: Bearer <oturum belirteci>`; belirteç yoksa 401.
- `register` isteğe bağlı `avatar` alır; `auth` cevabı `name`, `tag`, `avatar` içerir.

### 3.0e `hello` — lobi, çevrimiçi durum, çağrılar (p0.18)
Uygulama açıkken bir bağlantı açık kalır:
```json
{ "t": "hello", "auth": "<oturum belirteci>" }
```
Sunucu önce `{ "t": "hello", "protocol": "p0.18", "name", "tag", "avatar" }`, sonra her değişiklikte:
```json
{ "t": "presence",
  "friends": [{ "name": "Ayşe#0412", "avatar": "av02", "mutual": true, "online": true, "room": "Cuma", "title": "Dune" }],
  "requests": [{ "name": "Cem#7781", "avatar": "" }],
  "calls": [{ "from": "Serkan#3124", "avatar": "av01", "room": "Cuma", "invite": "k3F9…", "at": 0 }] }
```
- `online`/`room`/`title` yalnızca karşılıklı arkadaşlar için dolu. İstemci 20 sn içinde bir şey yazmalı (ör. `{ "t": "ping" }`); `{ "t": "decline", "invite": "…" }` çağrıyı siler. Odaya girince çağrı kendiliğinden düşer.
- Oda içinden çağrı: `{ "t": "call", "user": "Ayşe#0412" }` → `{ "t": "called", "user": "Ayşe#0412" }`; karşılıklı arkadaş değilse `error` (`not_friends`), bağlantı açık kalır.
- Hesaplı sunucuda `join` yalnızca şu durumlarda kabul edilir: çağrılmışsın, içeride karşılıklı bir arkadaşın var, zaten odadasın, daha önce bu odadaydın (p0.22) ya da oda boş. Yoksa `error` (`not_invited`).
- p0.22 `presence.rooms`: ana ekrandaki "Açık odalar". Önce senin bulunduğun odalar (`mine: true`, `left`: çıktığın an), sonra karşılıklı arkadaşının içinde olduğu odalar; en fazla 6:
```json
{ "invite": "k3F9…", "room": "Cuma", "title": "Prisoners", "view": "watch", "night": false,
  "playing": false, "pos": 4350, "dur": 9180, "members": ["Ayşe", "Mert"], "mine": true, "left": 0 }
```
  `pos` 30 sn'ye yuvarlanır (presence yalnızca değişince gider). Karşılıklı arkadaş satırında `invite`: arkadaşının odasına ana ekrandan katılmak için.
- p0.22 tek seferlik `{ "t": "feed", "auth": "…" }` → son 7 gün, en yeni önce, en fazla 30 satır (sen + karşılıklı arkadaşların):
```json
{ "t": "feed", "you": "Serkan#3124", "items": [
  { "kind": "watch", "at": 0, "users": ["Ayşe#0412", "Serkan#3124"], "title": "Prisoners", "room": "Cuma", "sec": 2400 },
  { "kind": "joined", "at": 0, "users": ["Deniz#0815"], "by": "Mert#1180" } ] }
```
  `watch`: aynı odada aynı başlığı izleyenler bir satırda (en çok izleyen önce, `sec` en uzun süre); 5 dk altı düşer.
- p0.22 oda içinde `{ "t": "close_room" }`: host odayı herkes için kapatır; diğerlerine `error` (`room_ended`), oda listeden hemen düşer.

### 3.0g `lists` — İzleyeceklerim / İzlediklerim (p0.23)
Tek seferlik (cevap, sonra bağlantı kapanır). Başlık anahtarı `ref`: IMDb kimliği (`tt…`), yoksa `tmdb:<kind>:<id>`.
```json
{ "t": "lists", "auth": "…" }                                  // kendi listelerin
{ "t": "lists", "auth": "…", "user": "Ayşe#0412" }             // karşılıklı arkadaşın (değilse not_friends)
{ "t": "lists", "auth": "…", "op": "add", "list": "want", "entry": { "ref": "tt2543164", "kind": "movie", "title": "Arrival", "year": 2016, "poster": "https://…", "source": "night" } }
{ "t": "lists", "auth": "…", "op": "remove", "list": "seen", "ref": "tt2543164" }
{ "t": "lists", "auth": "…", "op": "rate", "ref": "tt2543164", "rating": 1 }   // 1 ❤️, -1 👎, 0 yok
{ "t": "lists", "auth": "…", "op": "who", "ref": "tt2543164" }
```
Cevap: `{ "t": "lists", "user", "avatar", "want": [entry], "seen": [entry] }`. Kendi `want` satırlarında `mates` (aynı başlığı isteyen karşılıklı arkadaşlar); arkadaşın görünümünde `common` (ikinizin de istediği `ref`'ler) ve `together` (birlikte izlenen sayısı). `seen` satırında `rating`, `with` (birlikte izlenenler). `add/remove/rate` kendi listelerini döner. `who`: `{ "op": "who", "ref", "want": [{name, avatar}], "seen": [{name, avatar, rating}], "mine": { "want"?: true, "seen"?: true, "rating"? } }`. `seen`'e eklenen başlık `want`'tan düşer.

### 3.0f `admin` — yönetim (p0.19)
Admin sunucuda verilir: `koni-server admin -db koni.db Ad#1234` (`-off` geri alır, `-list` hesapları listeler).
```json
{ "t": "admin", "auth": "<oturum belirteci>", "op": "ban", "user": "Kaan#7730" }
```
Cevap (ardından bağlantı kapanır):
```json
{ "t": "admin", "now": 0,
  "rooms": [{ "id": "r-…", "name": "Cuma", "view": "watch", "title": "Prisoners", "since": 0, "members": [{ "name": "Serkan", "avatar": "av25", "host": true, "admin": true }] }],
  "members": [{ "name": "Ayşe#4471", "avatar": "av26", "admin": false, "canInvite": true, "invitedBy": "Serkan#3124", "created": 0, "friends": 6, "online": true, "room": "Cuma" }] }
```
- `op`: `list` (varsayılan), `close` (+ `room`: oda kimliği; içeridekiler `error` `room_closed` alıp çıkar), `ban` (+ `user`), `invite_right` (+ `user`, `on`). Admin değilse → `forbidden`; admini banlamak da `forbidden`.
- Ban: hesap, oturumları, arkadaşlıkları, davetleri ve istatistikleri silinir; odadaysa `error` (`banned`) alıp çıkar. Onu davet eden admin değilse davet hakkını kaybeder (`profile op=invite` → `no_invites`) ve kullanılmamış kodları silinir.
- `profile op=upload` (yalnızca admin): `image` base64 JPEG/PNG/WebP, en fazla 180 KiB. Görsel `ad-…` kimliğiyle ortak seçiciye eklenir ve adminin fotoğrafı olur.
- Bir admin hangi fotoğrafı kullanıyorsa (hazır ya da yüklenmiş) başkası onu seçemez: `profile.taken` listesi, seçerse `error` (`avatar_taken`). Admin değiştirince fotoğraf herkese açılır. Daha önce seçmiş olan üye fotoğrafını korur.

### 3.1 `create_room`
```json
{ "t": "create_room", "name": "Cuma Gecesi", "username": "Serkan", "policy": "free", "auth": "<oturum belirteci>" }
```
`policy`: `free` (varsayılan) | `host_only`. Oluşturan kişi **host** olur.
`auth` (`join`'de de): hesaplı sunucuda zorunlu (yoksa `auth_required`, geçersizse `bad_token`); üye adı hesabın adıdır, `username` yok sayılır. Açık sunucuda `auth` yok sayılır.

### 3.2 `join`
```json
{ "t": "join", "invite": "k3F9…", "username": "Coni" }
```
`invite`: `joined`/`room_created` cevabında gelen davet token'ı. Kullanıcı adı odada benzersiz olmalıdır (büyük/küçük harf duyarsız), 1–24 karakter.

### 3.2a `resume` — kopan bağlantıyı sürdürme
```json
{ "t": "resume", "room": "r-7Qx", "member": "u2", "token": "…" }
```
`token`: `joined.resume` değeri (üye başına gizli; sunucu sabit zamanlı karşılaştırır). Üye hâlâ odadaysa (hayalet ya da bağlantısı henüz düşmemiş) sürdürülür; eski bağlantı varsa kapatılır. Aksi halde `error` (`resume_failed`) ve bağlantı kapanır.

### 3.3 `cmd` — oda state'ini değiştirme önerisi
```json
{ "t": "cmd", "cid": "c-81", "op": "pause", "pos": 5050.120 }
```
| `op` | Alanlar | Etki |
|---|---|---|
| `play` | `pos` (ops.) | Odayı `COUNTDOWN`'a sokar; `startAt`'ta `PLAYING` olur |
| `pause` | `pos` | Odayı hemen `PAUSED` yapar |
| `seek` | `pos` | Yeni anchor; odayı `COUNTDOWN`'a sokar (herkes exact seek yapar, `startAt`'ta devam) |
| `set_policy` | `policy` | Yalnızca host. `free` ↔ `host_only` |
| `set_view` | `view` | Odanın ortak modu: `browse` (Keşfet) · `watch` (Sinema) · `listen` (Müzik). `host_only`'de yalnızca hostlar. Herkese `view` (§4.17) |
| `media` | `media` | Odanın medyasını değiştirir; oda `PAUSED`, `anchorPos = 0`, bekleyenler temizlenir, tüm DETACHED üyeler odaya döner |
| `continue_without` | `user` | Host ya da yardımcı host (host'u hedefleyemez). `user` üyesini DETACHED yapar: oda onu beklemez (HOLD'daysa `waitingFor`'dan düşer; boşalırsa `release`) |
| `resync` | — | Gönderen DETACHED ise odaya döner (`member_detached` `detached:false`); her durumda gönderene `ack`'li `state` (`cause: "snapshot"`) gider |
| `set_cohost` | `user`, `on` | Yalnızca host. `on: true` üyeyi yardımcı host yapar, `false` geri alır; herkese `member_role`. Değişiklik yoksa sessiz |

- `cid`: istemcinin ürettiği, bağlantı içinde benzersiz kimlik. Sunucu `state`/`denied` içinde `ack` olarak geri verir; istemci kendi yankısını böyle tanır.
- `pos` yoksa sunucu o anki oda konumunu kullanır (`position(now)`, bkz. §6).
- `pause` sırasında `pos` istemcinin gerçek `time-pos` değeridir; sunucu bunu anchor yapar (kullanıcı neyi görüp durdurduysa o konum).
- Rate limit: üye başına **3 cmd/sn** (kayan pencere). Aşım → `denied` (`rate_limited`).
- `host_only` politikasında host ve yardımcı hostlar dışındaki `play|pause|seek|media` → `denied` (`host_only`). Host/yardımcı host olmayanın `continue_without`'u ve host olmayanın `set_policy`/`set_cohost`'u → `denied` (`not_host`).

`media` nesnesi:
```json
{ "t": "cmd", "cid": "c-82", "op": "media", "media": { "kind": "url", "src": "https://youtu.be/dQw4w9WgXcQ", "title": "", "duration": 0 } }
```
- `kind: "url"`: `src` http(s) adresi. Her istemci adresi **kendisi** çözer (yt-dlp stream adresleri çözen istemcinin IP'sine bağlıdır) ve yükler. `referer` (ops.) varsa `src` yt-dlp'ye gitmeyen doğrudan bir akıştır (istemcinin Sanal Yakalayıcısı sayfadan yakaladı); her istemci onu bu HTTP Referer'la (ve tarayıcı user-agent'ıyla) yükler. Geçersiz `referer` (http(s) değil, 2048 bayttan uzun) reddedilmez, düşürülür. `subs` (ops., p0.9): yakalanan akışın altyazıları `[{url, lang, title}]`; her istemci oynatıcıya ekler. Geçersiz girdiler düşürülür; `referer` yoksa alan yok sayılır.
- `kind: "file"`: `src` yalnızca dosya adıdır (`/` veya `\` içeremez, yerel yol sunucuya gitmez). Her üye kendi kopyasını açar.
- `kind: "download"` ("Herkes İndirsin"): `src` http(s) adresi; her üye sayfayı kendi diskine indirir (§3.8). Sunucu yeni bir görev açar ve `media.task` kimliğini (`d1`, `d2`, …) **kendisi** atar (istemcinin gönderdiği yok sayılır). Aynı adres yeniden gönderilirse yeni görev açılır.
- `kind: "debrid"` (p0.15, Real-Debrid): `src` torrentin infoHash'i (40 hex; sunucu küçük harfe çevirir). `file` (ops.) torrent içindeki dosyanın adı; sunucu yalnızca son yol parçasını tutar, 255 karakteri aşarsa düşürür. `idx` (ops.) Stremio `fileIdx`'i (0 tabanlı, 0–100000; dışındaysa düşürülür). Her üye torrenti **kendi** Real-Debrid hesabıyla çözer; anahtar ve addon adresleri sunucuya gitmez. `file`/`idx` başka türlerde yok sayılır.
- `title` (ops., en fazla 200 karakter; boşsa `src`), `duration` (ops., saniye). `src` en fazla 2048 bayt.
- Geçersiz nesne → `denied` (`bad_media`).

### 3.4 `report` — periyodik durum raporu (~1 sn)
```json
{ "t": "report", "pos": 5050.118, "state": "playing", "bufferSec": 34.2, "bufferPct": 100 }
```
- `state`: `playing` | `paused` | `stalled` | `refilling` | `ready` | `ahead_hold` | `catching_up` | `detached`.
- `stalled` odayı `HOLD(stall)` yapar (§5). `refilling` rezerv dolarken, `ready` rezerv ≥ 12 s dolunca (veya yetişince) gönderilir. `ready` gelen üye `waitingFor`'dan düşer.
- Kronik stall: bir üye son **4 dk** içinde **3** kez `stalled` bildirirse host'a ve bağlı yardımcı hostlara bir kez `detach_offer` gider (§4.9). Host'un stall'u sayılmaz.
- DETACHED üyenin `stalled`/`ready` raporları odayı etkilemez.
- Diğer değerler (`playing`, `paused`, `ahead_hold`, `catching_up`, `detached`) yalnızca bilgi amaçlıdır; sunucu `member_status` ile yayar.
- `skippedSec` (ops.): istemci > 30 s geride kalıp ileri seek yaptıysa atladığı süre. Sunucu `activity` (`kind: "skip"`, "Furkan 42 s ileri atladı") yayınlar. **> 30 s geride için sunucu odayı HOLD'a almaz**, istemci kendi başına ileri seek yapar.
- `report` yanıt üretmez; sunucu bunu diğer üyelere `member_status` ile (en çok 1 Hz) yayar. Sapma gösterimi (`driftMs`) sunucuda hesaplanır.

### 3.5 `ping`
```json
{ "t": "ping", "t0": 1730130000123, "rtt": 38 }
```
`t0`: istemcinin kendi yerel saati (ms). Sunucu aynen geri yansıtır. `rtt` (ops.): istemcinin ölçtüğü son RTT (ms); sunucu countdown payı (`startAt`) için kullanır ve `member_status.rttMs` ile yayar, üst sınır 2000.

### 3.6 ~~`chat`~~
p0.8'de kaldırıldı (sohbet yok).

### 3.7 `leave`
```json
{ "t": "leave" }
```
Odadan bilerek çıkış: hayalet beklemesi olmadan `member_left`.

### 3.8 `dl_progress` — "Herkes İndirsin" ilerlemesi
```json
{ "t": "dl_progress", "taskId": "d1", "state": "downloading", "pct": 42.5, "speed": 3500000, "eta": 31, "error": "" }
```
- `state`: `queued` | `downloading` | `ready` | `failed`. `pct` 0–100, `speed` bayt/sn, `eta` saniye, `error` yalnızca `failed`'da.
- `taskId` odanın geçerli görevi değilse yok sayılır. Sunucu durumu `dl_status` (§4.11) ile herkese yayınlar.
- **Bariyer:** bağlı ve DETACHED olmayan her üyenin durumu `ready` ya da `failed` olunca (en az biri `ready`) sunucu kendiliğinden `state` (`mode: "countdown"`, `cause: "download_ready"`) yayınlar. Bundan önce gelen herhangi bir `play`/`seek` "hazır olanlarla başla" demektir; bariyer kalkar. İndirmesi bitmemiş üyelerin `report`'undaki stall HOLD tetiklemez. İstemci yeniden bağlanınca son durumu tekrar gönderir.

### 3.9 "Ne izlesek?": `discover` / `swipe` / `propose` / `unpropose` / `spin` / `wheel_clear`
```json
{ "t": "discover", "mood": "cyberpunk", "maxRuntime": 120 }
{ "t": "swipe", "deckId": "k3", "itemId": 550, "vote": "like" }
{ "t": "propose", "query": "matrix" }
{ "t": "propose", "itemId": 550 }
{ "t": "unpropose", "proposalId": "p4" }
{ "t": "spin" }
{ "t": "wheel_clear" }
```
Sunucuda TMDB anahtarı yoksa (`KONI_TMDB_KEY`) hepsi `fun_error` döner ve `moods` hiç gelmez.
- `discover`: `mood` sunucunun mood tablosundan (`moods`, §4.12). `maxRuntime` dakika (0 = fark etmez). Sunucu TMDB'den popüler sayfalardan, bu odada daha önce dağıtılmamış 10 film seçer (24 saat önbellek). Politika `host_only` ise yalnızca host/yardımcı host. Yükleme sürerken gelen ikinci istek yok sayılır.
- `swipe`: `vote` `like` | `pass`. Oylar kesindir ve gizlidir (yalnızca kaç kişinin bitirdiği yayınlanır). Bağlı üyelerin hepsinin beğendiği **destedeki ilk kart** eşleşmedir (`match`). Bağlantısı kopan üye sayılmaz; o kopunca eşleşme oluşabilir. Herkes desteyi eşleşmesiz bitirirse en çok beğenilen 3 kart (eşitlikte deste sırası) "Deste" önerisi olarak çarka eklenir.
- `propose`: `query` (1–80 karakter; sunucu TMDB'de arar, bulamazsa düz başlık) ya da geçerli destedeki `itemId`. Üye başına en fazla 2, çarkta en fazla 12 öneri; aynı film iki kez eklenmez. Sonuçlanmış bir çarka yapılan öneri yeni tur başlatır.
- `unpropose`: kendi önerisini (host/yardımcı host herkesinkini) çevirmeden önce çıkarır.
- `spin`: en az 2 öneri gerekir; `host_only`'de yalnızca hostlar. Kazananı sunucu `crypto/rand` ile seçer (§4.13). `wheel_clear`: hostlar çarkı boşaltır.
- Kayıt: her oy `vote` (`deck`, `tmdb`, `like`), çevrilen çarktaki her üye önerisi `proposal` (`title`, `tmdb`, `won`) olayı olur; rozetleri besler.

### 3.9a "Ne izlesek?" oturumu (p0.17): `night_*`
```json
{ "t": "night_start", "kind": "movie", "perUser": 3 }
{ "t": "night_ready", "ready": true, "mood": "Gerim gerim" }
{ "t": "night_preview", "filters": [{ "tag": "gerim", "filter": { "genres": [53, 9648], "voteMin": 7 } }] }
{ "t": "night_suggest", "filter": { "genres": [53], "without": [27], "yearFrom": 1995, "yearTo": 2024, "runtimeMin": 90, "runtimeMax": 150,
                                     "voteMin": 7, "voteCountMin": 1000, "languages": ["en", "ko"], "keywords": ["heist"], "hideSeen": true } }
{ "t": "night_search", "query": "memories of murder" }
{ "t": "night_detail", "itemId": 11423 }
{ "t": "night_pick", "itemId": 11423 }
{ "t": "night_unpick", "itemId": 11423 }
{ "t": "night_done", "ready": true }
{ "t": "night_close" }
```
Akış: **setup** (herkes kendi havasını/filtresini seçer) → **pick** (herkes en fazla `perUser` başlık önerir) → **vote** (havuz desteye döner, §3.9 `swipe`) → **done**.
- `night_start`: host/yardımcı host (politika `free` ise herkes). `kind` `movie` | `tv` herkese kilitlenir; `perUser` 1–5. Açık bir oturumu baştan başlatır.
- `night_ready`: yalnızca setup'ta. `mood` diğerlerine görünen etiket (≤ 40 karakter). Bağlı üyelerin hepsi hazır olunca sunucu pick'e geçer.
- `night_preview` (≤ 8 filtre), `night_suggest`, `night_search`, `night_detail`: yalnızca soranın kendisine cevap gelir (§4.12a). Filtre alanlarının hepsi isteğe bağlı; tür id'leri TMDB film türleridir, dizide en yakın dizi türüne çevrilir (karşılığı olmayan düşer). `hideSeen`: bu odada daha önce dağıtılanlar gelmez.
- `night_pick`: yalnızca pick'te ve `night_done` göndermeden; sunucu künyeyi çeker, sonra ekler. Sınır `perUser`. `night_unpick` geri alır.
- `night_done`: önerilerim hazır (`ready: false` geri alır). Herkes hazır olunca öneriler birleştirilir (aynı başlık bir kez), karıştırılır ve `deck` olarak yayınlanır; kimin neyi önerdiği gönderilmez. Havuz boşsa `fun_error` gelir ve herkes yeniden seçer.
- Eşleşme ve eşleşmesiz bitiş §3.9 ile aynı (`match`, ya da en çok beğenilen 3 kart çarka). İkisinde de oturum done olur.
- `night_close`: hostlar oturumu kapatır.

### 3.10 Altyazı: `subs_search` / `subs_pick`
```json
{ "t": "subs_search", "lang": "tr", "hash": "8e245d9679d31e12", "query": "the matrix 1999" }
{ "t": "subs_pick", "fileId": 1952296, "lang": "tr" }
```
- `subs_search`: `hash` yerel dosyanın OpenSubtitles `moviehash`'i (boyut + ilk/son 64 KiB'ın 64 bit toplamı, 16 hane hex); yoksa `imdb` ya da `query` (boşsa oda medyasının başlığı). `lang` `tr`, `en` ya da `tr,en`. Cevap yalnızca soranadır (`subs_results`, §4.14).
- `subs_pick`: sunucu dosyayı indirir (UTF-8'e çevirir, en fazla 1 MiB) ve odaya `subtitle` yayınlar. `host_only`'de yalnızca hostlar. Sunucuda `KONI_OS_KEY` yoksa ikisi de hata döner.

### 3.11 Lounge kuyruğu: `queue_add` / `queue_vote` / `queue_remove` / `queue_next` / `track_end`
```json
{ "t": "queue_add", "media": { "kind": "url", "src": "https://youtu.be/…", "title": "Sanatçı – Şarkı" } }
{ "t": "queue_vote", "queueId": "q3" }
{ "t": "queue_remove", "queueId": "q3" }
{ "t": "queue_next", "src": "https://youtu.be/…" }
{ "t": "track_end", "src": "https://youtu.be/…" }
{ "t": "track_end", "src": "https://youtu.be/…", "failed": true }
```
- `queue_add`: yalnızca `kind: "url"` (yakalanmış akış yok, yerel dosya yok). Herkes ekleyebilir; üye başına 10, toplam 50; aynı `src` iki kez girmez. İstemci adresi önce kendisi çözer (başlık için). Oda medyası yoksa ya da çalan parça bitmiş ve kuyruk boşken bekliyorsa eklenen hemen açılır. `activity` (`kind: "queue_add"`) olayı DJ rozetini besler.
- `queue_vote`: "bunu sıradaki yap" oyunu açar/kapar. Sıra: en çok oy, eşitlikte ilk eklenen.
- `queue_remove`: kendi parçası; host/yardımcı host herkesinkini.
- `queue_next`: kuyruğun başına hemen geç; `host_only`'de yalnızca hostlar. `src` (isteğe bağlı) üyenin çalıyor gördüğü şarkıdır; oda medyası artık o değilse istek yok sayılır (iki kişi aynı anda ya da çift tıkla basınca tek şarkı geçilir).
- `track_end`: istemcinin oynatıcısı `src`'nin sonuna geldi (mpv `eof-reached`). Oda medyası `src` ise ve bu medya için ilk bildirimse kuyruğun başı oda medyası olur (`state`, `cause: "media"`), kısa bir beklemeden sonra sunucu kendiliğinden geri sayım başlatır (`cause: "play"`; aynı şarkı yeniden sıradaysa oynatıcılar onu sonunda tuttuğu için `cause: "seek"`). Kuyruk boşsa oda sonda duraklar (`state`, `cause: "pause"`, `by` yok); o zaman gelen `play` (konumsuz) şarkıyı baştan çalar (`cause: "seek"`), eklenen şarkı hemen açılır. Aynı sonun sonraki bildirimleri ve odadan ayrı (DETACHED) üyenin bildirimleri yok sayılır.
- `track_end` + `failed: true`: oda medyası bu üyede açılamadı. Şarkıyı kuyruğa ekleyen ya da odadaki üyelerin yarısından fazlası açamıyorsa herkes için geçilir (`activity` "Açılamadı, geçildi"); yoksa bu üye şarkı bitene dek DETACHED olur (`detached`, `by` yok) ve oda onu beklemez.
- Kuyruk eklemesi hemen açılır: oda medyası yoksa, bitmişse ya da kuyruktan gelmeyen ve duran bir medyaysa (Sinema'dan kalmış duraklatılmış film).
- Medyanın ilk 2 saniyesindeki takılmalar (herkes yeni şarkıyı açarken) etkinliğe "bekleniyor" diye yazılmaz ve "olmadan devam et" sayacına girmez.

### 3.12 Dosya relay'i (kaldırıldı)
p0.12–p0.15'te Lounge'a yerel ses dosyası eklemek için `POST /relay` vardı. Kaldırıldı: sunucu hiçbir dosya yüklemesi kabul etmez, `/relay` her zaman `404` döner. Kuyruğa yalnızca link girer.

### 3.13 `react` — hızlı tepki
```json
{ "t": "react", "emoji": "😂" }
```
- İzinli emojiler: 😂 😱 😍 👏 😭 🔥. Başka değer sessizce yok sayılır.
- Üye başına en çok 1 tepki / 700 ms; fazlası sessizce düşer (`denied` yok).
- Sunucu herkese (gönderen dahil) `reaction` yayınlar (§4.18). Etkinlik akışına yazılmaz, olay günlüğüne kaydedilmez.

## 4. Sunucu → İstemci

### 4.1 `room_created` / `joined`
```json
{ "t": "room_created", "room": "r-7Qx", "name": "Cuma Gecesi", "invite": "k3F9…", "you": "u1" }
```
```json
{ "t": "joined", "you": "u2", "invite": "k3F9…", "resume": "…", "resumed": false, "protocol": "p0.3",
  "snapshot": { …bkz. §4.2… }, "serverTs": 1730130000180 }
```
`room_created`'u hemen bir `joined` izler (host da bir üyedir). `resume`: bu üyenin sürdürme token'ı (§3.2a), istemci saklar. `resumed: true` ise bağlantı `resume` ile sürdürüldü.

`snapshot`:
```json
{ "room": "r-7Qx", "name": "Cuma Gecesi", "policy": "free", "host": "u1",
  "view": "browse",
  "members": [ { "id": "u1", "name": "Serkan", "host": true, "away": false, "detached": false } ],
  "state": { …§4.2 ile aynı alanlar, "cause": "snapshot"… },
  "downloads": [ { "user": "u1", "name": "Serkan", "state": "ready", "pct": 100, "speed": 0, "eta": 0, "error": "" } ] }
```
`view`: odanın modu (§4.17); yeni oda `browse` ile başlar.

`downloads`: geçerli "Herkes İndirsin" görevinde (`state.media.task`) durum bildirmiş üyeler; görev yoksa boş.

### 4.2 `state` — oda state'i (anchor)
```json
{ "t": "state", "seq": 43, "mode": "paused", "anchorPos": 5050.120, "anchorTs": 1730130000180,
  "startAt": 0, "rate": 1.0,
  "by": { "id": "u1", "name": "Serkan" }, "cause": "pause", "ack": "c-81", "serverTs": 1730130000180,
  "media": { "kind": "url", "src": "https://youtu.be/dQw4w9WgXcQ", "title": "https://youtu.be/dQw4w9WgXcQ" } }
```
- `mode`: `paused` | `countdown` | `playing` | `hold`
- `countdown`'da `startAt` > 0: bu anda `anchorPos` konumundan oynat. İstemci `seek exact` + pause yapıp hazır bekler.
- `playing`: `position(t) = anchorPos + (t − anchorTs)/1000 × rate`. Sunucu `startAt`'ta ayrıca bir `state` (`mode: "playing"`, `cause: "start"`) yayınlar.
- `hold`: `holdPos` + `waitingFor` (üye id'leri) + `reason` (`stall`) alanları da bulunur. İstemciler oynatmayı durdurup `anchorPos`'ta bekler.
- `by`: eylemi yapan üye (UI: "Serkan durdurdu: 01:24:10"). Sistem kaynaklı geçişlerde (`start`, `stall`, `release`) yoktur.
- `cause`: `play` | `pause` | `seek` | `start` | `stall` | `release` | `snapshot` | `media` | `download_ready`
- `media` (ops.): odanın o anki medyası (§3.3). Her `state` ve `snapshot`'ta bulunur; henüz medya açılmadıysa yoktur. İstemci `src` değişince yeni medyayı yükler.
- `ack`: komutun `cid` değeri. Yalnızca komutu gönderen üyeye giden kopyada bulunur, diğer üyelerdeki kopyada yoktur.
- `seq`: oda başına tek yönlü artan sayı. İstemci `seq` ≤ bildiği son `seq` ise mesajı yok sayar.

### 4.3 `activity` — etkinlik akışı
```json
{ "t": "activity", "kind": "pause", "user": "u1", "text": "Serkan durdurdu: 01:24:10", "serverTs": 1730130000180 }
```
`kind`: `join` | `leave` | `away` | `play` | `pause` | `seek` | `hold` | `release` | `policy` | `role` | `skip` | `media` | `detach` | `resync` | `download`. `text` Türkçe, doğrudan gösterilebilir.

### 4.4 `member_joined` / `member_left` / `member_status` / `member_away` / `member_back` / `member_detached` / `member_role`
```json
{ "t": "member_joined", "member": { "id": "u2", "name": "Coni", "host": false } }
{ "t": "member_left", "id": "u2", "host": "u3" }
{ "t": "member_role", "id": "u3", "cohost": true, "ack": "c-14" }
{ "t": "member_status", "id": "u2", "pos": 5050.118, "driftMs": 12, "state": "playing", "bufferSec": 34.2, "bufferPct": 100, "rttMs": 38 }
{ "t": "member_away", "id": "u2" }
{ "t": "member_back", "id": "u2" }
{ "t": "member_detached", "id": "u2", "detached": true }
```
`driftMs = (yerelPos − odaPos(now)) × 1000`; pozitif = önde. `bufferPct = min(100, bufferSec / 12 × 100)`. `rttMs` (ops.): üyenin `ping.rtt` ile bildirdiği son RTT (ms, üst sınır 2000); henüz bildirmediyse alan yoktur. `member_away`: bağlantı koptu, üye hayalet (§2); `member_back`: `resume` ile döndü. `member_left.host` (ops.): ayrılan host idiyse yeni host (öncelik: bağlı yardımcı host, sonra bağlı üye; yeni host'un yardımcılığı düşer). Üye nesnelerinde `cohost: true` yardımcı hostu gösterir.

### 4.5 HOLD ve geri sayım
HOLD bir `state` mesajıdır (`mode: "hold"`), ayrıca bekleme göstergesi için:
```json
{ "t": "buffer_status", "user": "u3", "name": "Furkan", "bufferPct": 65, "targetSec": 12 }
```
Hazır olunca sunucu `state` (`mode: "countdown"`, `cause: "release"`, `startAt`) yayınlar.

**"Hazır mısınız?"**: medya henüz hiç `PLAYING`'e geçmediyse geri sayım en az 3 sn'dir (§5). İstemci `mode == "countdown"` ve `startAt − serverTs ≥ 2500` ise 3-2-1 gösterir; `state`'te ayrı bir alan yoktur.

### 4.6 `pong`
```json
{ "t": "pong", "t0": 1730130000123, "serverTs": 1730130000150 }
```
İstemci `t1` = pong'u aldığı yerel saat: `rtt = t1 − t0`, `offset = serverTs − (t0 + t1)/2`. En düşük RTT'li örneklerin medyanı kullanılır (design/sync.md §4.2).

### 4.7 `policy`
```json
{ "t": "policy", "policy": "host_only", "ack": "c-12" }
```
`set_policy` sonrası herkese gider.

### 4.8 `denied` / `error`
```json
{ "t": "denied", "ack": "c-90", "reason": "host_only" }
{ "t": "error", "code": "bad_invite", "message": "Davet geçersiz" }
```
- `denied.reason`: `host_only` | `rate_limited` | `not_host` | `bad_op` | `bad_media`
- `error.code`: `bad_message` | `bad_invite` | `bad_username` | `name_taken` | `not_joined` | `room_full` | `resume_failed` | hesap kodları (§3.0, §3.1) | `too_large` | `no_reports` (§3.0b)
- `error` ile ilgili durumlarda bağlantı `create_room`/`join` aşamasında kapatılır, sonrasında açık kalır.

### 4.9 `detach_offer` / `detached`
```json
{ "t": "detach_offer", "user": "u3", "name": "Furkan", "stalls": 3 }
{ "t": "detached", "by": "u1" }
```
- `detach_offer`: host'a ve bağlı yardımcı hostlara; kronik stall eden üye için "Furkan olmadan devam et" önerisi (§3.4). Biri `cmd continue_without` ile kabul eder; öneri o üye için bir sonraki DETACHED/`media`'ya kadar tekrarlanmaz.
- `detached`: yalnızca DETACHED yapılan üyeye. İstemci oda komutlarını uygulamayı bırakır (yalnızca `media` değişimini izler) ve kullanıcı "Odayla Senkronla" diyene kadar kendi hızında oynar; sonra `cmd resync`.

### 4.10 ~~`chat`~~
p0.8'de kaldırıldı.

### 4.11 `dl_status`
```json
{ "t": "dl_status", "taskId": "d1", "user": "u2", "name": "Coni", "state": "downloading", "pct": 42.5, "speed": 3500000, "eta": 31, "error": "" }
```
Bir üyenin indirme durumu (§3.8). Durum `ready`/`failed` olunca ayrıca `activity` (`kind: "download"`) gider.

### 4.12 Keşif: `moods` / `deck_loading` / `deck` / `deck_progress` / `match` / `discover_failed`
```json
{ "t": "moods", "moods": [{ "id": "cyberpunk", "label": "Cyberpunk", "emoji": "🌃" }] }
{ "t": "deck_loading", "mood": "🌃 Cyberpunk", "by": "Serkan" }
{ "t": "deck", "deckId": "k3", "mood": "🌃 Cyberpunk", "by": "Serkan", "over": false, "progress": { "done": 0, "of": 3 },
  "items": [{ "id": 603, "title": "Matrix", "original": "The Matrix", "year": 1999, "overview": "…", "poster": "https://image.tmdb.org/t/p/w342/….jpg",
              "runtime": 136, "genres": ["Aksiyon", "Bilim-Kurgu"], "imdb": "tt0133093", "trailer": "vKQi3bBA1y8",
              "ratings": { "tmdb": 8.2, "imdb": "8.7", "rt": "83%", "meta": "73" } }],
  "match": 603, "mine": { "603": true } }
{ "t": "deck_progress", "deckId": "k3", "done": 2, "of": 3, "over": false }
{ "t": "match", "deckId": "k3", "item": { "id": 603, "title": "Matrix", "…": "…" } }
{ "t": "discover_failed", "message": "TMDB: HTTP 503" }
```
- `moods`: katılınca (ve `resume`'da) gelir; yalnızca keşif açık sunucuda.
- `deck`: yeni deste herkese; katılan/dönen üyeye ayrıca geçerli deste, `mine` (kendi oyları) ile. `match` varsa eşleşen kartın id'si. `ratings` alanları eksik olabilir (OMDb anahtarı yoksa yalnızca `tmdb`). `trailer` YouTube video id'si.
- `deck_progress`: her oyda ve üye değişiminde. `over: true` = herkes bitirdi, eşleşme yok (ardından `wheel`).

### 4.12a Ne izlesek? oturumu: `night` / `night_previews` / `night_suggestions` / `night_results` / `night_detail`
```json
{ "t": "night", "id": "n2", "kind": "movie", "perUser": 3, "by": "Serkan", "phase": "pick", "deckId": "k4",
  "members": [{ "id": "u1", "name": "Serkan", "ready": true, "mood": "Gerim gerim", "picks": 2, "done": false }],
  "pool": 7, "mine": [{ "id": 146233, "title": "Prisoners", "…": "…" }], "pending": 0 }
{ "t": "night", "closed": true }
{ "t": "night_previews", "items": [{ "tag": "gerim", "total": 412, "posters": ["https://image.tmdb.org/t/p/w342/….jpg"] }] }
{ "t": "night_suggestions", "total": 412, "items": [{ "id": 146233, "title": "Prisoners", "year": 2013, "poster": "…", "votes": 12000, "ratings": { "tmdb": 8.1 } }] }
{ "t": "night_results", "query": "memories", "items": [] }
{ "t": "night_detail", "item": { "id": 11423, "kind": "", "runtime": 132, "seasons": 0, "director": "Bong Joon-ho", "cast": ["Song Kang-ho"], "countries": ["Güney Kore"], "trailer": "…", "…": "…" } }
```
- `night`: her değişimde herkese (kendi `mine` listesiyle); katılan/dönen üyeye de. `members` yalnızca bağlı üyeler; `picks` sayıdır, başlıklar gizlidir. `pending`: sunucunun hâlâ künyesini çektiği kendi önerilerin. `deckId` vote'tan itibaren.
- `night_suggestions` ve `night_results` öğeleri künyesizdir (afiş, yıl, puan); künye `night_detail` ile gelir. Dizilerde `kind: "tv"`, `runtime` bölüm süresi, `seasons` sezon sayısı, `director` yaratıcıdır. Hata olursa `error` alanı dolu, `items` boş.

### 4.13 `wheel` — çark
```json
{ "t": "wheel", "proposals": [{ "id": "p4", "by": "u2", "byName": "Coni", "item": { "id": 603, "title": "Matrix", "…": "…" } }],
  "result": { "winner": "p4", "index": 0, "spinMs": 4630, "turns": 6.71, "at": 1793300000000 } }
```
Her değişimde tam durum. `by` yoksa öneri desteden gelmiştir (`byName: "Deste"`). `item.id` 0 ise TMDB'de bulunamayan düz başlıktır. `result` yalnızca çevrildikten sonra: istemci çarkı saat yönünde `turns` tur, `spinMs` sürede (yavaşlayarak) döndürür. Dilim `i`, tepedeki işaretten saat yönünde `[i/n, (i+1)/n)` turunu kaplar; `turns`'ün kesir kısmı `1 − (index + u)/n` (u ∈ [0.15, 0.85]) olduğundan işaret `index` diliminde durur. `at` sunucu zamanı: `at + spinMs` geçmişse (geç katılan) animasyon atlanır.

### 4.14 Altyazı: `subs_results` / `subtitle`
```json
{ "t": "subs_results", "lang": "tr", "items": [{ "fileId": 1952296, "name": "The.Matrix.1999.srt", "lang": "tr", "downloads": 51234, "hashMatch": true, "hi": false, "machine": false }] }
{ "t": "subs_results", "lang": "tr", "error": "altyazı bulunamadı" }
{ "t": "subtitle", "name": "The.Matrix.1999.srt", "lang": "tr", "text": "1\n00:00:01,000 --> …", "by": "Serkan" }
```
`items` sırası: dosyaya özel (`hashMatch`) önce, insan çevirisi makineden önce, sonra indirme sayısı. `subtitle` herkese gider ve istemci onu yükleyip seçer; aynı medya sürerken katılana da gönderilir, medya değişince geçersizdir.

### 4.15 `fun_error`
```json
{ "t": "fun_error", "message": "Çark için en az 2 öneri gerekli" }
```
Keşif/çark/altyazı isteğinin yalnızca gönderene giden Türkçe hata satırı. Bağlantı açık kalır.

### 4.16 `queue`
```json
{ "t": "queue", "items": [{ "id": "q3", "media": { "kind": "url", "src": "https://youtu.be/…", "title": "Sanatçı – Şarkı" }, "by": "u2", "byName": "Coni", "voters": ["u1"] }] }
```
Kuyruğun tam hâli, sırasıyla; her değişimde herkese, katılan/dönen üyeye kuyruk boş değilse.

### 4.17 `view` — oda modu (p0.13)
```json
{ "t": "view", "view": "watch", "by": "Coni", "ack": "view-1730…" }
```
Herkes aynı ekranı görür: `browse` (arama, "Ne izlesek?", gömülü tarayıcı; video gizli), `watch` (sinema), `listen` (Lounge). `by` yoksa mod sunucu tarafından değişti: oda `browse`'dayken bir üye medya açarsa (`cmd media`) oda kendiliğinden `watch`'a geçer.

### 4.18 `reaction`
```json
{ "t": "reaction", "user": "u2", "name": "Coni", "emoji": "😂" }
```
Bir üyenin hızlı tepkisi (§3.13); gönderen dahil herkese. Saklanmaz, snapshot'ta yoktur.

## 5. Oda state makinesi (Faz 0)

```
PAUSED ──play──▶ COUNTDOWN ──startAt──▶ PLAYING
PLAYING ──pause──▶ PAUSED
PLAYING / PAUSED ──seek──▶ COUNTDOWN
PLAYING / COUNTDOWN ──report(stalled)──▶ HOLD(waitingFor += u)
HOLD ──tüm bekleyenler `ready`──▶ COUNTDOWN
HOLD ──bekleyen ayrıldı / koptu / DETACHED oldu──▶ (waitingFor'dan düşer; boşsa COUNTDOWN)
HOLD ──pause──▶ PAUSED
HOLD ──seek──▶ COUNTDOWN
herhangi ──media──▶ PAUSED (anchorPos = 0)
```
- `startAt = now + max(250, rttMax) ms`. Faz 0'da `rttP95`, üyelerin `ping.rtt` ile bildirdiği RTT'lerin en büyüğüdür (ölçüm yoksa 250 ms).
- **İlk başlatılış:** oda medyası değiştikten sonra henüz hiç `PLAYING`'e geçilmediyse (`play`, `seek`, HOLD sonrası `release`, `download_ready`) pay en az **3 sn**'dir. Medya ilk `COUNTDOWN → PLAYING` geçişinde başlamış sayılır; yeni medyada sıfırlanır. Böylece biri yüklemeyi bitirmeden stall → HOLD olursa `release` de 3 sn olur. Kuyruk geçişinde (§3.11) 3 sn bekleme zaten yapıldığından oradaki otomatik geri sayım normal paydır. Medya yokken `play` normal paydır.
- COUNTDOWN'da gelen `pause` odayı `PAUSED` yapar. COUNTDOWN'da `stalled` ise o üye `waitingFor`'a eklenir (HOLD).
- Boş odada `play` çalışır; medya yüklü mü kontrolü Faz 0'da yok.

## 6. Konum

```
position(t) = anchorPos + (mode == "playing" ? (t − anchorTs)/1000 × rate : 0)
```
`countdown` ve `hold` boyunca oda konumu `anchorPos`'ta sabit sayılır.

## 7. Sürüm ve değişiklik

Bu doküman istemci senkron motoruyla ortak sözleşmedir. Mesaj alanı eklemek geriye uyumludur. Mevcut alanın anlamını değiştirmek veya silmek `p0.x` sürümünü artırır ve istemci thread'ine haber verilmelidir. Sunucu sürümü `joined.protocol` ve açılış logunda (`koni-server p0.14 listening …`) görünür.
