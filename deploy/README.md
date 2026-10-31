# KONİ Santral kurulumu (işletici notları)

Kendi sunucunda KONİ kurmak için adım adım rehber: [docs/SELF-HOSTING.md](../docs/SELF-HOSTING.md) (English).
Bu dosya mevcut santralin işletici notlarıdır.

Sunucu `127.0.0.1:8080`'de `koni` kullanıcısıyla systemd servisi olarak çalışır; dışarıya
nginx (TLS) üzerinden açılır. Aynı makinedeki diğer sitelere dokunulmaz; sertifika Let's Encrypt.

- İstemci adresi: `wss://$KONI_DOMAIN/ws`
- Sağlık: `https://$KONI_DOMAIN/healthz` → `ok`

`$KONI_HOST`: sunucunun adresi, `$KONI_DOMAIN`: santralin alan adı (ortam değişkeni olarak ver, ör. `$env:KONI_HOST` / `export KONI_HOST=…`).

## İlk kurulum
1. DNS: alan adı için A kaydı → sunucu IP'si (Cloudflare'de **DNS only**, gri bulut).
   Sertifika Let's Encrypt'ten alınır (önce yalnızca port 80 bloğu etkin olmalı; `install.sh` NGINX=1 bunu sırayla yapar).
2. Repo özel olduğu için kaynak yerelden yüklenir:
   ```
   git archive --format=tar -o koni-src.tar HEAD server deploy
   scp koni-src.tar root@$KONI_HOST:/tmp/
   ssh root@$KONI_HOST 'rm -rf /opt/koni/src && mkdir -p /opt/koni/src && tar -xf /tmp/koni-src.tar -C /opt/koni/src && DOMAIN=$KONI_DOMAIN NGINX=1 /opt/koni/src/deploy/install.sh'
   ```

Firewall değişikliği gerekmez (8080 yalnızca loopback; 443 zaten açık).

## Güncelleme
Yukarıdaki 2. adımı `NGINX=1` olmadan tekrarla (test eder, derler, eski ikiliyi `koni-server.prev` olarak saklar, servisi yeniden başlatır).

### Sunucuya erişimi olan agent için talimat
Amaç: VPS'teki Santral'ı `main` dalının son hâline getirmek. Yalnızca KONİ servisine dokunulur.

1. Yerel klonda sunucu kodu `main`'den, kurulum betikleri bu daldan: `git fetch origin && git checkout --detach origin/main && git checkout origin/deploy/guncelleme -- deploy` (bu dal `main`'e birleştiyse ikinci komut gereksiz).
2. Beklenen protokol sürümü: `grep ProtocolVersion server/hub/messages.go` (ör. `p0.6`). Derleme modülleri ve gerekirse `go.mod`'daki Go sürümünü (1.26) internetten indirir (`GOTOOLCHAIN=auto`, varsayılan).
3. Kaynağı yükle ve kur (**`NGINX=1` verme**, nginx ve sertifika zaten kurulu):
   ```
   tar -cf koni-src.tar server deploy
   scp koni-src.tar root@$KONI_HOST:/tmp/
   ssh root@$KONI_HOST 'rm -rf /opt/koni/src && mkdir -p /opt/koni/src && tar -xf /tmp/koni-src.tar -C /opt/koni/src && /opt/koni/src/deploy/install.sh'
   ```
   `install.sh` önce `go test ./...` koşar; test kırılırsa kurulum durur ve çalışan sürüm değişmez. Bittikten sonra yerel klonu temizle: `git checkout -- deploy && git checkout -`.
4. Doğrula:
   - Çıktının sonunda `ok <- koni healthz` ve `koni-server <sürüm> listening on 127.0.0.1:8080, accounts, db /var/lib/koni/koni.db` satırı; `<sürüm>` 2. adımdakiyle aynı olmalı.
   - Dışarıdan: `curl -fsS https://$KONI_DOMAIN/healthz` → `ok`.
   - Diğer siteler etkilenmedi: `nginx -t` temiz, `systemctl is-active nginx` → `active`.
5. p0.5'ten itibaren sunucu hesaplıdır: bir hesap davet kodu üret (aşağıda, "Hesap davetleri") ve kodu, sürüm ve healthz ile birlikte bu dosyanın bulunduğu PR'a ya da kullanıcıya bildir.
6. p0.10'dan itibaren keşif/altyazı anahtarları `/etc/koni/env`'den okunur ("Keşif ve altyazı anahtarları"). Anahtarları kullanıcı verir; dosyayı yalnızca kullanıcı isterse oluştur, değerleri hiçbir yere yazma ya da loglama.
7. Dosya relay'i kaldırıldı; nginx'te `/relay` bloğu artık yok ve `client_max_body_size 1k` ile hiçbir yükleme kabul edilmez. Bu sürüme ilk geçişte 3. adımı bir kez `NGINX=1 /opt/koni/src/deploy/install.sh` ile koş (alan adı kurulu site dosyasından okunur; yalnızca koni site dosyasını yeniler, `nginx -t && systemctl reload nginx` yapar). Doğrulama: `/relay/x/y` adresi nginx'in 404'ünü döner.

Yapma: nginx/certbot/firewall ayarlarını değiştirme, `/etc/nginx/sites-*` altında `koni` dışındaki dosyalara dokunma, başka servisleri yeniden başlatma.

Not: servis yeniden başlayınca açık odalar kapanır; kullanıcılar yeniden oda kurar.

### Geri alma
```
ssh root@$KONI_HOST 'cp -f /opt/koni/koni-server.prev /opt/koni/koni-server && systemctl restart koni && sleep 1 && curl -fsS http://127.0.0.1:8080/healthz'
```
`.prev` p0.4 ya da öncesiyse `-db` bayrağını tanımaz ve başlamaz; önce birimden çıkar: `sed -i 's| -db /var/lib/koni/koni.db||' /etc/systemd/system/koni.service && systemctl daemon-reload` (veritabanı yerinde kalır).

## Hesap davetleri
Sunucu `-db /var/lib/koni/koni.db` ile hesaplı çalışır: odaya girmek için hesap gerekir, hesap açmak için tek kullanımlık davet kodu. İlk açılan hesap admin olur.
```
ssh root@$KONI_HOST 'runuser -u koni -- /opt/koni/koni-server invite -db /var/lib/koni/koni.db'          # 7 gün geçerli
ssh root@$KONI_HOST 'runuser -u koni -- /opt/koni/koni-server invite -db /var/lib/koni/koni.db -ttl 720h' # 30 gün
```
Komutu mutlaka `koni` kullanıcısıyla çalıştır (root ile açılan SQLite dosyaları servisin yazmasını bozar). Servis çalışırken de olur. Kod istemcide "Hesapla giriş yap → Davet kodum var" alanına yazılır. p0.4 ve öncesi istemciler hesaplı sunucuya giremez (`auth_required`).

Yedek: `/var/lib/koni/koni.db` (+ `-wal`, `-shm`). Hesapları sıfırlamak için servisi durdurup bu dosyaları sil.

## Keşif ve altyazı anahtarları (p0.10)
"Ne izlesek?" TMDB ister (ücretsiz: themoviedb.org → Ayarlar → API; v3 anahtarı ya da v4 okuma belirteci). IMDb/Rotten Tomatoes puanları için OMDb (ücretsiz, omdbapi.com/apikey.aspx), altyazı araması için OpenSubtitles (opensubtitles.com → API consumers; hesap bilgisi verilirse günlük indirme kotası artar). Anahtarlar istemciye hiç gitmez:
```
ssh root@$KONI_HOST 'install -d -m 750 -o root -g koni /etc/koni && install -m 640 -o root -g koni /dev/null /etc/koni/env'
ssh root@$KONI_HOST 'cat >> /etc/koni/env' <<'EOF'
KONI_TMDB_KEY=...
KONI_OMDB_KEY=...
KONI_OS_KEY=...
KONI_OS_USER=...
KONI_OS_PASS=...
EOF
ssh root@$KONI_HOST 'systemctl restart koni && journalctl -u koni -n 3 --no-pager'
```
Açılış satırı açık servisleri sayar: `… accounts, db /var/lib/koni/koni.db, discover+omdb, subtitles`. Boş bırakılan anahtar o özelliği kapalı bırakır; istemci "Bu sunucuda keşif kapalı" der.

## İşletim
`systemctl status koni` · `journalctl -u koni -f`

Not: odalar bellektedir; servis yeniden başlayınca tüm odalar düşer. Hesaplar ve olay günlüğü kalır.
