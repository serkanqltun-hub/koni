# KONİ — Keşif Motoru

> [PLAN.md](../PLAN.md)'nin parçası. Bölüm numaraları korunmuştur; kod ve dokümanlardaki `§4.4` gibi atıflar bu numaralara işaret eder.

## 7. Keşif Motoru ("Ne izlesek?")

### 7.1 Kaynaklar
| Kaynak | Kullanım | Not |
|---|---|---|
| TMDB | discover, keyword, genre, runtime, poster, videos | Ücretsiz anahtar, atıf zorunlu [lisans koşulları doğrulanmalı] |
| OMDb | IMDb puanı, RT eleştirmen puanı, Metascore | Ücretsiz katman ~1000 istek/gün [doğrulanmalı] |
| Trakt | İzleme geçmişi (ops.) | OAuth |
| Letterboxd, RT seyirci | — | Resmi açık API yok/kısıtlı, scraping ToS riski → v1 dışı |

Anahtarlar Santral'da kalır. Sonuçlar 24 saat önbelleklenir.

### 7.2 Mood Eşlemesi
Sunucuda sürümlenebilir bir JSON tablosu tutulur. TMDB keyword ID'leri `/search/keyword` ile bir kez çözülür.

| Mood | TMDB eşlemesi |
|---|---|
| cyberpunk | keyword: cyberpunk, dystopia · genre: Sci-Fi |
| 90'lar | `primary_release_date` 1990–1999 |
| mindfuck | keyword: plot twist, nonlinear timeline · genre: Mystery/Thriller |
| klostrofobik | keyword: claustrophobia, single location, trapped |
| comfort | Comedy/Animation/Family, `vote_average.gte=7`, Horror hariç |
| Tempo / max süre | tür ağırlığı + `with_runtime.lte` |

Kalite tabanı `vote_count.gte=300`. Keyword kapsamı dengesiz olduğu için tablo elle ayarlanır.

### 7.3 Swipe ve Çark
- **Swipe**: Santral 10 kartlık ortak deste üretir. `{ "t":"swipe", "deckId":"k3", "itemId":550, "vote":"like" }`. Tüm aktif üyelerin beğendiği ilk kart → `match` → konfeti → kokpite geçiş ve kaynak arama. Oylar gizlidir, yalnızca "4/5 oyladı" görünür. Eşleşme yoksa ilk 3 kart çarka gider.
- **Çark**: Herkes 1–2 öneri atar. Seçim **sunucuda**, kriptografik RNG ile yapılır ve herkese aynı animasyon parametreleri gider: `{ "t":"wheel_result", "winner":"p4", "spinMs":4200, "turns":5.37 }`.
- Tüm öneri ve oylar `proposals`/`votes` tablolarına yazılır ve rozetleri besler.
- **Hover trailer**: TMDB videos → YouTube anahtarı → `youtube-nocookie` embed (mute, 10 sn, 1 sn hover gecikmesi). Hub'da video kutusu gizli olduğu için bu iframe çakışma yaratmaz.
- **Puan rozeti**: IMDb + RT eleştirmen (OMDb) + TMDB. Eksik kaynak gri gösterilir.

### 7.4 Uygulanan (p0.10)
- Santral: `server/discover` (TMDB discover/search/movie + OMDb, tüm yanıtlar 24 saat önbellekte, en fazla 4000 kayıt), mood tablosu `server/discover/moods.json` (18 mood, `go:embed`; keyword adları `/search/keyword` ile çözülür). Anahtarlar ortam değişkeninden (`KONI_TMDB_KEY`, `KONI_OMDB_KEY`), yoksa keşif kapalı.
- Deste: en popüler 8 sayfadan rastgele başlangıçla, odada daha önce dağıtılmamış 10 film, karıştırılmış; ayrıntı (süre, tür, IMDb id, fragman) paralel çekilir. Fragman seçimi: YouTube Trailer > Teaser, Türkçe öncelikli.
- Swipe/eşleşme/çark kuralları ve mesajlar: [PROTOCOL.md §3.9, §4.12–4.13](../PROTOCOL.md). Eşleşme yalnızca bağlı üyelerle hesaplanır; DETACHED üye de oy verir.
- `proposals`/`votes` tabloları yerine `events` (`vote`, `proposal`) kullanılır; rozetler [accounts.md §5.4](accounts.md)'te.
- İstemci: sağ rayda ✦ "Ne izlesek?" çekmecesi (`DiscoverPanel`, `MovieCard`, `Wheel`). Kart sürüklenerek ya da düğmeyle oylanır; 1 sn üzerinde durunca 10 sn sessiz fragman. Eşleşmede "Kaynak bul" film adını panoya kopyalayıp "Video ekle"yi açar (kaynak araması otomatik değil).
- Doğrulanmadı: TMDB/OMDb kotaları ve atıf koşulları, WebView2'de youtube-nocookie gömmesi.
