<script lang="ts">
  // "Mola" düğmesi (T33): kontrol şeridinde; süre seçilince oda durur, herkese mola sahnesi gelir.
  import { t } from "./i18n.svelte";

  let { onBreak }: { onBreak: (min: number) => void } = $props();
  let open = $state(false);
  const MINS = [5, 10, 15, 30];
</script>

<span class="bb">
  <button class="chip2 info" class:on={open} title={t("Mola ver")} onclick={() => (open = !open)}>{t("Mola")}</button>
  {#if open}
    <span class="menu" role="menu">
      {#each MINS as m (m)}
        <button role="menuitem" onclick={() => { open = false; onBreak(m); }}>{t("{n} dk", { n: m })}</button>
      {/each}
    </span>
  {/if}
</span>

<style>
  .bb { position: relative; display: inline-flex; }
  .menu { position: absolute; bottom: calc(100% + 8px); right: 0; display: flex; flex-direction: column; min-width: 84px; padding: 4px; border-radius: 12px; background: var(--glass-2); border: 1px solid var(--line-2); backdrop-filter: blur(14px); z-index: 5; }
  .menu button { background: none; border: 0; color: inherit; padding: 7px 12px; border-radius: 8px; text-align: left; font-size: 13px; }
  .menu button:hover { background: rgba(255, 255, 255, 0.08); }
</style>
