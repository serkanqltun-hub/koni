<script lang="ts">
  // Film/bölüm bitti ekranında puan kartı: 1–5 yıldız + tek kelime + "İzlediklerime ekle".
  import { listsAdd, listsStars, listsWho, type ListEntry } from "./player";
  import { social } from "./social.svelte";
  import { t } from "./i18n.svelte";

  let { entry }: { entry: ListEntry } = $props();

  let stars = $state(0);
  let word = $state("");
  let add = $state(true);
  let wasSeen = $state(false);
  let ready = $state(false);
  let saved = $state(false);
  let busy = $state(false);
  let failed = $state(false);

  $effect(() => {
    const ref = entry.ref;
    ready = saved = failed = false;
    stars = 0; word = ""; add = true; wasSeen = false;
    if (!social.server) return;
    let live = true;
    listsWho(social.server, ref).then((w) => {
      if (!live || !w) return;
      wasSeen = !!w.mine.seen;
      stars = w.mine.stars ?? 0;
      word = w.mine.word ?? "";
      saved = stars > 0;
      ready = true;
    }, () => {});
    return () => { live = false; };
  });

  async function save() {
    if (busy || !stars || !social.server) return;
    busy = true; failed = false;
    try {
      if (!wasSeen) {
        if (!add) return;
        await listsAdd(social.server, "seen", $state.snapshot(entry));
        wasSeen = true;
      }
      await listsStars(social.server, entry.ref, stars, word.trim());
      saved = true;
    } catch { failed = true; } finally { busy = false; }
  }
</script>

{#if social.server && ready}
  <div class="rc">
    <span class="stars" role="radiogroup" aria-label={t("Puanın")}>
      {#each [1, 2, 3, 4, 5] as n (n)}
        <button class:on={n <= stars} role="radio" aria-checked={n === stars} aria-label={`${n}`} onclick={() => { stars = n; saved = false; }}>★</button>
      {/each}
    </span>
    <input maxlength="24" placeholder={t("Tek kelime (isteğe bağlı)")} bind:value={word} oninput={() => (saved = false)} />
    {#if !wasSeen}<label><input type="checkbox" bind:checked={add} /> {t("İzlediklerime ekle")}</label>{/if}
    <button class="sv" disabled={busy || !stars || saved || (!wasSeen && !add)} onclick={save}>{saved ? t("✓ Kaydedildi") : t("Puanla")}</button>
    {#if failed}<small>{t("Kaydedilemedi")}</small>{/if}
  </div>
{/if}

<style>
  .rc { display: flex; flex-wrap: wrap; align-items: center; gap: 10px 14px; margin: 2px 0 14px; font-size: 12.5px; color: var(--text-2); }
  .stars { display: inline-flex; gap: 2px; }
  .stars button { width: 34px; height: 34px; padding: 0; border: 0; background: none; font-size: 24px; line-height: 1; color: var(--text-3); opacity: 0.45; }
  .stars button.on { color: #ffd60a; opacity: 1; }
  .stars button:hover { opacity: 1; }
  input:not([type]) { width: 180px; height: 34px; padding: 0 12px; border-radius: 10px; background: var(--glass-2); border: 1px solid var(--line-2); color: inherit; }
  label { display: inline-flex; align-items: center; gap: 6px; cursor: pointer; }
  .sv { height: 34px; padding: 0 16px; border-radius: 10px; border: 0; font-weight: 800; background: #fff; color: #000; }
  .sv:disabled { opacity: 0.5; }
  small { color: #ff6961; }
</style>
