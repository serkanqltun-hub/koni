<script lang="ts">
  // "Mola" düğmesi (T33): kontrol şeridinde; süre seçilince oda durur, herkese mola sahnesi gelir.
  // Süreler şeridin içinde açılır: şeridin üstü video alanıdır ve video orada webview'in
  // üstünde ayrı pencerede çizildiği için yukarı açılan menü videonun altında kalırdı.
  import { t } from "./i18n.svelte";

  let { onBreak }: { onBreak: (min: number) => void } = $props();
  let open = $state(false);
  const MINS = [5, 10, 15, 30];
</script>

<span class="bb">
  {#if open}
    {#each MINS as m (m)}
      <button class="c min" onclick={() => { open = false; onBreak(m); }}>{t("{n} dk", { n: m })}</button>
    {/each}
    <button class="c x" title={t("Kapat")} aria-label={t("Kapat")} onclick={() => (open = false)}>✕</button>
  {:else}
    <button class="c" title={t("Mola ver")} onclick={() => (open = true)}>{t("Mola")}</button>
  {/if}
</span>

<style>
  .bb { display: inline-flex; align-items: center; gap: 4px; }
  /* Şeritteki çiplerle aynı boy (App.svelte .chip2 bu bileşene geçmez). */
  .c { height: 30px; padding: 0 10px; border-radius: 9px; display: inline-flex; align-items: center; font-size: 12px; color: var(--text-2); white-space: nowrap; }
  .min { padding: 0 8px; }
  .x { padding: 0 8px; color: var(--text-3); }
</style>
