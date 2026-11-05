<script lang="ts">
  // Ortak çark (docs/design/discovery.md §7.3). Sonucu sunucu seçer; herkes aynı
  // `turns`/`spinMs` ile döndürür. Dilim i, tepedeki işaretten saat yönünde
  // [i/n, (i+1)/n) turu kaplar; çark saat yönünde `turns` tur döner (PROTOCOL §4.13).
  import { fun } from "./fun.svelte";
  import { syncFun } from "./player";
  import MovieCard from "./MovieCard.svelte";
  import WheelDisc from "./night/WheelDisc.svelte";
  import { t } from "./i18n.svelte";

  let { canControl, me, onError }: { canControl: boolean; me: string | null; onError: (msg: string) => void } = $props();

  let query = $state("");
  let rot = $state(0);
  let dur = $state(0);
  let shown = $state<string | null>(null);
  let lastKey = "";
  let revealTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const r = fun.result;
    if (!r) {
      lastKey = "";
      clearTimeout(revealTimer);
      rot = 0; dur = 0; shown = null;
      return;
    }
    const key = r.winner + "@" + r.at;
    if (key === lastKey) return;
    lastKey = key;
    clearTimeout(revealTimer);
    if (Date.now() - r.at > r.spinMs + 2000) { // geç katılan: animasyonsuz
      dur = 0; rot = r.turns * 360; shown = r.winner;
      return;
    }
    dur = 0; rot = 0; shown = null;
    requestAnimationFrame(() => requestAnimationFrame(() => { dur = r.spinMs; rot = r.turns * 360; }));
    revealTimer = setTimeout(() => (shown = r.winner), r.spinMs);
  });

  const winner = $derived(fun.proposals.find((p) => p.id === shown) ?? null);

  function send(msg: Record<string, unknown>) {
    syncFun(msg).catch((e) => onError(String(e)));
  }
  function propose() {
    const q = query.trim();
    if (!q) return;
    send({ t: "propose", query: q });
    query = "";
  }
</script>

<div class="eyebrow">{t("Çark")}</div>
{#if fun.proposals.length}
  <div class="wheel-wrap">
    <WheelDisc items={fun.proposals.map((p) => ({ id: p.id, title: p.item.title }))} {rot} {dur} />
  </div>
  {#if winner}
    <div class="winner"><span>🎡 {t("Kazanan")}</span><MovieCard movie={winner.item} compact /></div>
  {/if}
  <ul class="props">
    {#each fun.proposals as p (p.id)}
      <li class:win={p.id === shown}>
        <span class="t">{p.item.title}{#if p.item.year}<small> ({p.item.year})</small>{/if}</span>
        <small class="by">{p.byName}</small>
        {#if !fun.result && (p.by === me || canControl)}
          <button class="icon" title={t("Çıkar")} onclick={() => send({ t: "unpropose", proposalId: p.id })}>×</button>
        {/if}
      </li>
    {/each}
  </ul>
{:else}
  <div class="hint">{t("Herkes 1–2 film önersin, sonra çark seçsin.")}</div>
{/if}
<div class="row">
  <input placeholder={fun.result ? t("Yeni tur için film öner") : t("Film adı yaz, Enter")} bind:value={query} maxlength="80"
    onkeydown={(e) => e.key === "Enter" && propose()} />
  <button disabled={!query.trim()} onclick={propose}>{t("Öner")}</button>
</div>
{#if canControl && fun.proposals.length}
  <div class="row">
    <button class="white grow" disabled={fun.proposals.length < 2 || !!fun.result} onclick={() => send({ t: "spin" })}>{t("Çarkı çevir")}</button>
    <button onclick={() => send({ t: "wheel_clear" })}>{t("Temizle")}</button>
  </div>
{/if}

<style>
  .wheel-wrap { display: grid; place-items: center; margin: 4px 0 10px; }
  .winner { display: flex; flex-direction: column; gap: 6px; margin-bottom: 10px; }
  .winner > span { font-weight: 650; }
  .props { list-style: none; margin: 0 0 10px; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .props li { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-radius: 10px; background: var(--glass); }
  .props li.win { outline: 1px solid #f5c518; }
  .props .t { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .props small { color: var(--text-3); }
  .props .icon { width: 22px; height: 22px; padding: 0; }
  .row { display: flex; gap: 8px; margin-top: 8px; }
  .row input { flex: 1; min-width: 0; }
  .grow { flex: 1; }
  .hint { color: var(--text-2); font-size: 12px; margin-bottom: 4px; }
</style>
