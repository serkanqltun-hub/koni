<script lang="ts">
  // Addon listesi (Faz 5). Ayarlar'da ve Film & Dizi panelinde aynı
  // bileşen: anahtar anahtar zincirinde, addon listesi bu bilgisayarda (addons.json).
  import { t, tx } from "./i18n.svelte";
  import {
    addonAdd, addonRemove, addonsList, debridClearKey, debridStatus,
    type Addon, type DebridStatus,
  } from "./player";

  let { onChange }: { onChange?: () => void } = $props();

  let rd = $state<DebridStatus | null>(null);
  let addons = $state<Addon[]>([]);
  let addonDraft = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    debridStatus().then((s) => (rd = s)).catch((e) => (error = String(e)));
    addonsList().then((a) => (addons = a)).catch((e) => (error = String(e)));
  });

  async function act(f: () => Promise<void>) {
    busy = true;
    error = null;
    try {
      await f();
      onChange?.();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
  const clearKey = () => act(async () => {
    await debridClearKey();
    if (rd) rd = { ...rd, configured: false, user: null, error: null };
  });
  /** Birden çok adres (arkadaşın "Hepsini kopyala"sı) yapıştırılabilir: satır ya da boşlukla ayrılmış. */
  const addAddon = () => act(async () => {
    const urls = [...new Set(addonDraft.split(/\s+/).map((u) => u.trim()).filter(Boolean))];
    const have = new Set(addons.map((a) => a.url));
    for (const u of urls) if (!have.has(u)) addons = await addonAdd(u);
    addonDraft = "";
  });

  const removeAddon = (url: string) => act(async () => { addons = await addonRemove(url); });
</script>

<p class="lead">
  {t("Herkes kendi sağlayıcı hesabını kullanır; anahtarın veya bağlantın bu bilgisayardan çıkmaz. Odaya yalnızca izlenen içeriğin bilgisi (bölüm/zaman) gider; KONİ kaynak listelemez, akışı kendi eklediğin eklentiler sağlar.")}
</p>
{#if rd?.configured}
  <!-- Anahtar artık girilmiyor (kaynak addon'un kendi yapılandırmasından gelir); eskiden kaydedilmiş anahtar silinebilsin. -->
  <div class="group glass-card">
    <div class="item">
      <div class="grow"><b>{t("Eski API anahtarı kayıtlı")}</b><small>{t("Artık gerekmiyor; addon'lar kendi yapılandırmalarıyla çalışır.")}</small></div>
      <button class="danger" disabled={busy} onclick={clearKey}>{t("Anahtarı sil")}</button>
    </div>
  </div>
{/if}
<h2>{t("Addon'lar")}</h2>
<div class="group glass-card">
  {#each addons as a (a.url)}
    <div class="item">
      <div class="grow"><b>{a.name}</b><small class="mono url" title={a.url}>{a.url}</small></div>

      <button disabled={busy} onclick={() => removeAddon(a.url)}>{t("Kaldır")}</button>
    </div>
  {/each}
  <div class="item col">
    <div>
      <b>{t("Addon ekle")}</b>
      <small>{t("Stremio addon manifest adresi (…/manifest.json). Addon'u kendi sağlayıcı anahtarınla yapılandır ve oluşan bağlantıyı (anahtar dahil) olduğu gibi buraya ekle. Bağlantı yalnızca bu bilgisayarda durur, odada paylaşılmaz; herkesin KONİ'si aynı bölümü kendi addon'undan bulup senkron açar. Birden çok adresi boşlukla ayırarak tek seferde ekleyebilirsin.")}</small>
    </div>
    <div class="row">
      <input class="mono grow" bind:value={addonDraft} placeholder={t("https://…/manifest.json (birden çok olabilir)")}
        onkeydown={(e) => e.key === "Enter" && addonDraft.trim() && addAddon()} />
      <button class="white" disabled={busy || !addonDraft.trim()} onclick={addAddon}>{t("Ekle")}</button>
    </div>
  </div>
</div>
{#if error}<p class="err">{tx(error)}</p>{/if}

<style>
  .lead { margin: 0 0 18px; color: var(--text-2); font-size: 13px; line-height: 1.5; }
  h2 { margin: 28px 0 12px; font-size: 16px; font-weight: 600; }
  .group { border-radius: 16px; overflow: hidden; }
  .item { display: flex; align-items: center; gap: 14px; padding: 14px 16px; }
  .item + .item { border-top: 1px solid var(--line); }
  .item.col { flex-direction: column; align-items: stretch; gap: 10px; }
  .item b { display: block; font-weight: 550; }
  small { color: var(--text-3); font-size: 12px; }
  .grow { flex: 1; min-width: 0; }
  .row { display: flex; gap: 8px; }
  .url { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .err { color: var(--red, #ff453a); }
  .err { font-size: 13px; margin-top: 12px; }
</style>
