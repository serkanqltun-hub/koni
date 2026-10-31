# KONİ — Riskler ve Açık Sorular

> [PLAN.md](../PLAN.md)'nin parçası. Bölüm numaraları korunmuştur; kod ve dokümanlardaki `§4.4` gibi atıflar bu numaralara işaret eder.

## 11. Riskler ve Açık Sorular

### 11.1 Teknik Riskler
| Risk | Olasılık / Etki | Hafifletme |
|---|---|---|
| Child HWND konum/DPI senkronu (modal resize döngüsü, monitör değişimi) | Orta / Orta | Konum Rust'ta pencere prosedüründe (`WM_SIZE`/`WM_WINDOWPOSCHANGED`/`WM_DPICHANGED`), DIP tabanlı ortak layout config, Per-Monitor V2 |
| Hız düzeltmesinin yavaşlığı | Orta / Düşük | 0.04 maksimum hız farkıyla saniyede 40 ms kapanır. 3.0 s sapma en iyi durumda 3.0 / 0.04 = **75 s** sürer (2.5 s ≈ 62.5 s, 1 s ≈ 25 s, 500 ms ≈ 12.5 s). Orantısal kontrolcü hedefe yaklaşırken yavaşladığı için pratikte ~85–100 s [ölçülmeli]. Bu süre boyunca sapma kısmen fark edilebilir. Hafifletme: büyük sapmaların kaynağı olan stall'ları histerezis kapısıyla HOLD'a almak, 3.0 s üstünü freeze/hold ile anında çözmek. Eşik 3.0 s'de sabit; Faz 0 verisi çok kötü çıkarsa yeniden değerlendirilir |
| Müzik modunda pitch correction artefaktları | Orta / Orta | mpv'nin pitch correction'ı (scaletempo2 tabanlı [doğrulanmalı]) sürekli tonlarda/müzikte "titreşim" yapabilir. Lounge'da dar aralık (0.98–1.02), geniş ölü bant, şarkı geçişinde sıfırlama, gerekirse `rubberband` filtresi denemesi |
| Freeze over rewind'ın diğer izleyicileri sık durdurması | Orta / Orta | Histerezis rezervi, kronik stall'da "onsuz devam" seçeneği, Modem Sesi rozetiyle görünürlük |
| Web UI öğelerinin video kutusuna taşması (airspace) | Orta / Düşük | Tasarım kuralı: menüler panele açılır, modal'da video gizlenir |
| macOS gömme (`wid` kırılgan, render API emek ister) | Orta-Yüksek / Orta | Faz 2'ye ertelendi; M1 1 hafta zaman kutusu, sonra M2 |
| yt-dlp kırılmaları (özellikle YouTube) | Yüksek / Orta | Otomatik güncelleme + nightly, güncelle butonu, JS runtime takibi |
| Codec/HDR (AV1 hwdec yok, HDR→SDR, DV profil 5) | Orta / Orta | `hwdec=auto-safe` + yazılım fallback, tone-mapping ayarları, format önerisinde en zayıf istemciyi esas alma |
| Senkron jitter (Wi-Fi, hotspot) | Orta / Düşük | Min-RTT örnekleme, 50 ms ölü bant, speed tabanlı düzeltme |
| Şifre/oturum güvenliği | Düşük / Orta | argon2id, opak hash'lenmiş token, TLS zorunlu, giriş rate limit |
| Tek VPS, SQLite kaybı | Düşük / Orta | Günlük SQLite yedeği (`.backup`), systemd auto-restart |
| Over-engineering | Orta / Orta | RD/proxy en sona alındı, her faz kullanılabilir bir sürümle biter |
| API kotaları | Orta / Düşük | Sunucu tarafı önbellek |
| İmzalama (SmartScreen, notarization) | Orta / Düşük | Özel dağıtımda talimat, ileride sertifika |

NAT sorunu yoktur, çünkü mimari sunucu merkezli ve istemciler yalnızca dışarı bağlantı açar.

### 11.2 Yasal / ToS Riskleri (bilgilendirme)
Bu bölüm hukuki danışmanlık değildir. Amaç riskleri görünür kılıp tasarıma yansıtmaktır.

- **Telif ve korsan kaynaklar**: Lisanssız yayın yapan sitelerden stream çekip grupla izlemek birçok yargı alanında (Türkiye ve AB dahil) telif ihlali riski taşır. Uygulama bu kaynakları öne çıkarır veya kolaylaştırırsa risk geliştiriciye de yansıyabilir. YouTube vb. platformların ToS'u da indirmeyi genellikle kısıtlar.
- **Real-Debrid ToS**: Hesaplar kişiseldir. **Hesap paylaşımı ToS'ta yasaktır** ve RD eşzamanlı IP kullanımını izler. Trafiği tek VPS IP'sinden geçirmek tespit edilemezlik garantisi vermez. Bağlantı sayısı, hacim ve datacenter IP'si [RD politikası doğrulanmalı] başka sinyaller olabilir. Hesabın kapanması ve ödenmiş sürenin kaybı gerçek bir risktir.
- **VPS sağlayıcı (Hetzner)**: Telif şikayetleri müşteriye iletilir. Tekrarlayan veya yanıtsız şikayetlerde sunucu kapatılabilir. İçeriğin diske yazılmaması sorumluluğu ortadan kaldırmaz. v0.2'deki varsayılan mimaride (Boru yok) VPS'ten medya geçmez, bu da bu riski büyük ölçüde azaltır.
- **Dağıtım**: Uygulamayı arkadaş grubunun dışına açmak, özellikle korsan kaynak/RD paylaşım özellikleriyle birlikte, sorumluluk profilini ciddi biçimde değiştirir (kaldırma bildirimleri, aracı sorumluluğu).

**Hafifletici seçenekler:**
1. **Herkes kendi RD anahtarı** (varsayılan). Paylaşımlı Boru yalnızca bilinçli, opsiyonel bir tercih olur.
2. **Çekirdek / eklenti ayrımı**: Çekirdek yalnızca genel amaçlı özellikleri içerir (URL, yerel dosya, senkron, altyazı, keşif). Addon kaynakları kullanıcının kendi eklediği manifest URL'leridir. Uygulama hiçbir korsan kaynak listelemez veya önermez.
3. **Özel kullanım**: Yalnızca davetle kayıt, açık oda yok.
4. **Minimal log**: Upstream URL'leri ve içerik başlıkları gereksiz yere loglanmaz. İstatistikler grup içinde kalır, abuse iletişim kanalı açık tutulur.
5. Keşifte (ileride) yasal izleme seçenekleri gösterilir (TMDB watch providers).

### 11.3 Açık Sorular
- ~~Ciddi geride kalan istemci için varsayılan~~ → **Karar verildi (2026-10-29):** 3–30 s geride freeze over rewind, > 30 s geride otomatik ileri seek ([sync.md §4.4](sync.md)).
- ~~Ciddi desync eşiği 2.5 s mi 3.0 s mi?~~ → **Karar verildi (2026-10-29): 3.0 s.**
- ~~Histerezis rezervi ve kronik kural~~ → **Karar verildi (2026-10-29): 12 s rezerv, "4 dk'da 3 stall".**
- Gruptaki Mac kullanıcı sayısı Faz 2'nin önceliğini değiştirir mi?
- **Lisans (karar, 2026-10-29)**: libmpv dinamik kütüphane (`.dll`/`.dylib`) olarak yükleniyor ve LGPL sınırlarında kalınıyor. KONİ'nin kendi kodu açık da olabilir kapalı da. Bunun geçerli olması için üç koşul var:
  1. **libmpv ve içindeki FFmpeg LGPL modunda derlenmeli.** mpv'nin varsayılan derlemesi GPL'dir, `-Dgpl=false` ile derlenmeli. FFmpeg de `--enable-gpl`/`--enable-nonfree` olmadan derlenmeli. Hazır Windows build'lerinin çoğu GPL derlenmiş olabilir [doğrulanmalı]. GPL bir build ile dağıtılırsa uygulamanın tamamı GPL kapsamına girer.
  2. **Dağıtımda** LGPL lisans metni ve atıflar verilmeli. Kullanılan libmpv/FFmpeg sürümünün kaynak kodu (veya kaynağa erişim teklifi) da sağlanmalı.
  3. **Kullanıcı DLL'i kendi derlediği uyumlu bir sürümle değiştirebilmeli.** Statik bağlama yapılmamalı, DLL imza kontrolüyle kilitleme olmamalı.
  - yt-dlp Unlicense olduğu için sorun yok. Faz 0'da LGPL libmpv build pipeline'ı (CI'da tekrarlanabilir) kurulmalı.
