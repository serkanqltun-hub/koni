# ADR 0004: Anti-rubberbanding: düzeltme amaçlı geri seek yok

- Durum: Kabul edildi (v0.3, eşikler v0.3.1)

## Bağlam
Sapmayı geri sararak düzeltmek diyalog ve kare tekrarına yol açar; izleyici bunu "rubberbanding" olarak hisseder.

## Karar
İstemci sapmayı düzeltmek için **asla geri seek yapmaz** (sapma = yerelPos − odaPos):

| Sapma | Eylem |
|---|---|
| < 50 ms | Yok, hız 1.0 |
| 50 ms – 3.0 s | Hız 0.96–1.04, orantısal, `audio-pitch-correction=yes` |
| > 3.0 s önde | Yerel hold: oda konumu yetişince yeniden başla |
| 3–30 s geride | Freeze over rewind: oda HOLD'a girer, geride kalan oynayarak yetişir |
| > 30 s geride | Oda konumuna **ileri** seek |
| Stall | 12 s rezerv dolmadan devam yok; 4 dk'da 3 stall → kronik, "onsuz devam" |

Geri seek yalnızca bilinçli timeline scrub'ında olur ve herkese yeni anchor olarak uygulanır.

## Sonuçlar
- 3.0 s'lik sapmayı hızla kapatmak en iyi ihtimalle ~75 s sürer; büyük sapmaların kaynağı olan stall'lar HOLD ile çözülür.
- Motor (`client/crates/koni-sync`) tek bir seek kapısından geçer; `backward_correction_seeks_blocked` sayacı 0 kalmalıdır (Faz 0/1 kabul kriteri).
- Ayrıntı: [design/sync.md §4.4–4.7](../design/sync.md).
