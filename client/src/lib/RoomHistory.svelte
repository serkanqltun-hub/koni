<script lang="ts">
  // Odanın ortak geçmişi: kim neyi açtı, nerede kaldık. Karta basınca aynı medya odaya yeniden açılır.
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import { roomHistory, reopen } from "./historyStore.svelte";
  import type { HistoryItem } from "./player";

  let { limit = 4, onError }: { limit?: number; onError?: (msg: string) => void } = $props();

  const name = (h: HistoryItem) => h.media.title || h.media.file || h.media.src;
  function where(pos: number): string {
    const m = Math.floor(pos / 60);
    if (m < 1) return t("Baştan");
    return m >= 60 ? t("{h} sa {m} dk'da kaldı", { h: Math.floor(m / 60), m: m % 60 }) : t("{m} dk'da kaldı", { m });
  }
  async function open(h: HistoryItem) {
    try { await reopen(h); } catch (e) { onError?.(String(e)); }
  }
</script>

{#if roomHistory.items.length}
  <div class="rh">
    <span class="k">{t("Odada açılanlar")}</span>
    <div class="grid" style="--n:{limit}">
      {#each roomHistory.items.slice(0, limit) as h (h.media.kind + h.media.src + (h.media.file ?? ""))}
        <button class="card" title={name(h)} onclick={() => open(h)}>
          <span class="th"><Icon name={h.media.kind === "page" ? "globe" : "film"} size={18} /></span>
          <span class="tx"><b>{name(h)}</b><small>{t("{name} açtı", { name: h.by })} · {where(h.pos)}</small></span>
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .k { font-size: 12px; color: var(--text-3); }
  .grid { display: grid; grid-template-columns: repeat(var(--n), 1fr); gap: 12px; margin-top: 10px; }
  .card { padding: 0; border-radius: 14px; overflow: hidden; border: 1px solid var(--line); background: var(--glass, var(--raise)); text-align: left; display: block; min-width: 0; }
  .th { display: grid; place-items: center; aspect-ratio: 16 / 9; background: linear-gradient(135deg, #263445, #10151d); color: var(--text-3); }
  .tx { display: block; padding: 8px 10px; }
  b { font-size: 12.5px; display: block; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  small { color: var(--text-3); font-size: 11px; display: block; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
