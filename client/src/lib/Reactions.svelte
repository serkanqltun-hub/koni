<script lang="ts">
  import { t } from "./i18n.svelte";
  import { REACTIONS, syncFun } from "./player";
  import { fun } from "./fun.svelte";

  // Hızlı tepkiler (PROTOCOL p0.14): alt bardaki düğmeler ve gelen tepkilerin baloncukları.
  // Video native pencere olduğundan baloncuklar video üstüne değil bu şeride çizilir;
  // tam ekranda Rust OSD'ye kısa metin düşer.
  // `compact`: tek düğme, tıklayınca emojiler şeridin içinde açılır (video üstüne taşmaz).
  let { compact = false }: { compact?: boolean } = $props();
  let open = $state(false);
  const react = (emoji: string) => syncFun({ t: "react", emoji }).catch(() => {});
</script>

<div class="reacts">
  {#if compact}
    <button class="face" class:on={open} title={t("Tepki")} onclick={() => (open = !open)}>
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="9" /><path d="M8 14s1.5 2 4 2 4-2 4-2M9 9h.01M15 9h.01" /></svg>
    </button>
  {/if}
  {#if !compact || open}
    {#each REACTIONS as e (e)}
      <button title={t("Tepki: {e}", { e })} onclick={() => react(e)}>{e}</button>
    {/each}
  {/if}
  <div class="bubbles" aria-live="polite">
    {#each fun.reactions as b (b.id)}
      <span class="bubble" style="left:{b.x}%">{b.emoji} <small>{b.name}</small></span>
    {/each}
  </div>
</div>

<style>
  .reacts { position: relative; align-self: stretch; display: flex; align-items: center; gap: 1px; flex: none; }
  button {
    width: 26px; height: 26px; padding: 0; border: 0; border-radius: 8px; background: transparent;
    font-size: 15px; line-height: 1; display: grid; place-items: center; filter: saturate(0.85);
  }
  button:hover { background: var(--glass-2); filter: none; transform: scale(1.12); }
  button:active { transform: scale(0.95); }
  .face { width: 34px; height: 34px; border-radius: 10px; color: var(--text-2); filter: none; }
  .face:hover { transform: none; color: var(--text); }
  .face.on { color: var(--text); background: var(--glass-2); }
  .bubbles { position: absolute; inset: 0; pointer-events: none; }
  .bubble {
    position: absolute; bottom: 4px; transform: translateX(-50%); white-space: nowrap;
    padding: 2px 8px; border-radius: 99px; font-size: 15px; line-height: 20px;
    background: rgba(36, 37, 41, 0.92); border: 1px solid var(--line-2);
    animation: rise 1.8s ease-out forwards;
  }
  .bubble small { font-size: 11px; color: var(--text-2); font-weight: 550; }
  @keyframes rise {
    0% { opacity: 0; translate: 0 6px; }
    12% { opacity: 1; translate: 0 0; }
    70% { opacity: 1; }
    100% { opacity: 0; translate: 0 -34px; }
  }
</style>
