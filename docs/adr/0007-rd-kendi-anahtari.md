# ADR 0007: Real-Debrid: varsayılan "herkes kendi anahtarı"

- Durum: Kabul edildi (v0.2)

## Bağlam
RD hesapları kişiseldir, hesap paylaşımı ToS'ta yasak. Tek bir VPS üzerinden içerik akıtmak hem trafik hem telif şikayeti riski taşır.

## Karar
Opsiyonel RD/addon modunda her istemci kendi RD hesabıyla doğrudan çözer; Santral yalnızca "hangi kaynak" bilgisini senkronlar. Paylaşımlı range proxy ("Boru") yalnızca bilinçli, ayrı bir alt faz olur.

## Sonuçlar
- VPS'ten medya geçmez; hesap paylaşımı yok.
- Ayrıntı: [design/debrid.md §8](../design/debrid.md), [design/risks.md §11.2](../design/risks.md).
