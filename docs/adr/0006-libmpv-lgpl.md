# ADR 0006: libmpv: dinamik yükleme, LGPL derleme

- Durum: Kabul edildi (v0.3.1)

## Bağlam
GPL derlenmiş bir libmpv ile dağıtım, uygulamanın tamamını GPL kapsamına sokar. Hazır Windows derlemelerinin (shinchiro, zhongfly, MSYS2) çoğu GPL.

## Karar
libmpv çalışma anında DLL olarak yüklenir (`libloading`, derleme zamanı link yok). Dağıtımda kendi LGPL derlememiz kullanılır: mpv `-Dgpl=false`, FFmpeg `--enable-gpl`/`--enable-nonfree` olmadan (`tools/libmpv`, CI workflow `libmpv (LGPL, Windows x64)`).

## Sonuçlar
- Dağıtımda LGPL metni, atıflar ve kullanılan sürümlerin kaynağı (veya kaynak teklifi) verilir.
- Kullanıcı DLL'i kendi derlediği uyumlu bir sürümle değiştirebilmelidir: statik link ve DLL imza kilidi yok.
- Geliştirme/CI'da indirilen hazır DLL'ler dağıtılmaz.
- Ayrıntı: [design/risks.md §11.3](../design/risks.md) (Açık Sorular → Lisans), [LIBMPV.md](../LIBMPV.md).
