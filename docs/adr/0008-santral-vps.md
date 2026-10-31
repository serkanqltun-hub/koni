# ADR 0008: Santral yayını: ana sunucu

- Durum: Kabul edildi (2026-10-29)

## Bağlam
İki farklı ağdan senkron ölçümü için sunucunun internetten `wss://` ile erişilebilir olması gerekiyor. VPS'te başka bir web projesi çalışıyor ve ona dokunulmaması gerekiyor.

## Karar
Santral `ana sunucu` adresinde yayında. Go sunucusu `127.0.0.1:8080`'de systemd servisi (`koni`) olarak çalışır; önünde mevcut nginx'e eklenen **ayrı** bir site dosyası TLS'i sonlandırır (Let's Encrypt, webroot). DNS Cloudflare'de, kayıt "DNS only". Kurulum ve güncelleme: [deploy/README.md](../../deploy/README.md).

## Sonuçlar
- Diğer sitelerin yapılandırmasına dokunulmaz; KONİ site dosyası yalnızca `/ws` ve `/healthz`'i sunucuya yönlendirir.
- Faz 0'da odalar bellekte tutulur; servis yeniden başlarsa açık odalar kapanır.
- İstemcinin varsayılan sunucu adresi budur.
