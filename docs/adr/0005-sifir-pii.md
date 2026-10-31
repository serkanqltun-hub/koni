# ADR 0005: Kimlik: sıfır PII

- Durum: Kabul edildi (v0.3)

## Bağlam
Grup küçük ve özel; e-posta, telefon veya üçüncü taraf kimliği saklamak gereksiz risk ve bağımlılık.

## Karar
Tek kullanımlık davet token'ı + kullanıcı adı + argon2id şifre. Girişte opak, rastgele (256 bit) bir oturum token'ı verilir; sunucuda hash'lenmiş tutulur, istemcide OS anahtar zincirine yazılır. Discord OAuth ve diğer üçüncü taraf girişler v1 dışıdır.

## Sonuçlar
- Şifre sıfırlama admin tarafından yeni davetle yapılır.
- IP adresleri yalnızca rate limit için bellekte tutulur.
- Ayrıntı: [design/accounts.md §5.1](../design/accounts.md).
