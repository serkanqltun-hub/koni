<script lang="ts">
  // Mola sahnesi (T33): videonun yerinde; mpv gizli, overlay yok (kokpit modeli).
  import type { BreakState } from "./break.svelte";
  import { t } from "./i18n.svelte";

  let {
    state: b,
    names,
    me,
    countdown,
    canStart,
    onReady,
    onStart,
  }: {
    state: BreakState;
    /** Üye id → ad. */
    names: Record<string, string>;
    me: string | null;
    /** Herkes hazır olunca 3-2-1 (0 = yok). */
    countdown: number;
    /** Molayı beklemeden bitirebilir mi (oynatma yetkisi). */
    canStart: boolean;
    onReady: () => void;
    onStart: () => void;
  } = $props();

  let now = $state(Date.now());
  $effect(() => {
    const i = setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(i);
  });
  const left = $derived(Math.max(0, Math.ceil((b.until - now) / 1000)));
  const clock = $derived(`${Math.floor(left / 60)}:${String(left % 60).padStart(2, "0")}`);
  const mineReady = $derived(me != null && b.ready.includes(me));
  const nm = (id: string) => (names[id] ?? id).replace(/#\d+$/, "");
</script>

<div class="brk">
  {#if b.ending}
    <span class="k">{t("Herkes hazır")}</span>
    <div class="num">{countdown > 0 ? countdown : "…"}</div>
  {:else}
    <span class="k">{t("{name} mola verdi", { name: nm(b.by) })}</span>
    <div class="num">{left > 0 ? clock : t("Süre doldu")}</div>
    <p class="sub">{t("Herkes hazır olunca 3-2-1 ile kaldığınız yerden devam edilir.")}</p>
    <div class="who">
      {#each b.ready as id (id)}<span class="p ok">✓ {nm(id)}</span>{/each}
      {#each b.waiting as id (id)}<span class="p">{nm(id)}</span>{/each}
    </div>
    <div class="row">
      {#if mineReady}
        <button class="big g" disabled>{t("Hazırsın · {n}/{total}", { n: b.ready.length, total: b.ready.length + b.waiting.length })}</button>
      {:else}
        <button class="big w" onclick={onReady}>{t("Hazırım")}</button>
      {/if}
      {#if canStart}<button class="big g" onclick={onStart}>{t("Beklemeden başlat")}</button>{/if}
    </div>
  {/if}
</div>

<style>
  .brk { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; text-align: center; padding: 24px; }
  .k { font-size: 12px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-2); }
  .num { font-size: 72px; font-weight: 600; letter-spacing: -0.03em; font-variant-numeric: tabular-nums; line-height: 1; }
  .sub { color: var(--text-2); max-width: 340px; margin: 0; }
  .who { display: flex; flex-wrap: wrap; gap: 6px; justify-content: center; max-width: 440px; }
  .p { padding: 3px 10px; border-radius: 999px; background: var(--surface-2, rgba(255,255,255,0.06)); color: var(--text-2); font-size: 12px; }
  .p.ok { color: var(--text); }
  .row { display: flex; gap: 8px; margin-top: 6px; }
  .big { height: 52px; padding: 0 22px; border-radius: 15px; border: 0; font-weight: 800; font-size: 14.5px; white-space: nowrap; }
  .big.w { background: #fff; color: #000; }
  .big.w:hover { background: #e8e8ed; }
  .big.g { background: var(--glass-2); border: 1px solid var(--line-2); color: inherit; }
  .big:disabled { opacity: 0.7; }
</style>
