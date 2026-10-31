<script lang="ts">
  import { t } from "./i18n.svelte";
  import type { Snippet } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Logo from "./Logo.svelte";

  // Özel başlık çubuğu (decorations:false). Boş alanlar pencereyi sürükler,
  // çift tık büyütür (data-tauri-drag-region).
  let { crumb = null, sub = null, center, right }: { crumb?: string | null; sub?: string | null; center?: Snippet; right?: Snippet } = $props();

  const win = getCurrentWindow();
  let maximized = $state(false);

  $effect(() => {
    const sync = () => win.isMaximized().then((m) => (maximized = m)).catch(() => {});
    sync();
    const un = win.onResized(sync);
    return () => { un.then((f) => f()); };
  });
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <span class="mark"><Logo size={14} /></span>
    <span class="word">KONİ</span>
    {#if crumb}<span class="crumb" title={crumb}>/ {crumb}</span>{/if}
    {#if sub}<span class="sub" title={sub}>{sub}</span>{/if}
  </div>
  <div class="center">{@render center?.()}</div>
  <div class="right">
    {@render right?.()}
    <div class="wc">
      <button title={t("Küçült")} aria-label={t("Küçült")} onclick={() => win.minimize()}>
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" /></svg>
      </button>
      <button title={maximized ? t("Önceki boyut") : t("Büyüt")} aria-label={t("Büyüt")} onclick={() => win.toggleMaximize()}>
        {#if maximized}
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor"><rect x=".5" y="2.5" width="7" height="7" /><path d="M2.5 2.5V.5h7v7h-2" /></svg>
        {:else}
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor"><rect x=".5" y=".5" width="9" height="9" /></svg>
        {/if}
      </button>
      <button class="close" title={t("Kapat")} aria-label={t("Kapat")} onclick={() => win.close()}>
        <svg width="10" height="10" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" /></svg>
      </button>
    </div>
  </div>
</header>

<style>
  .titlebar {
    grid-area: title; position: relative; height: var(--top-bar); display: flex; align-items: center;
    padding-left: 12px; background: var(--bg); border-bottom: 1px solid var(--line); user-select: none;
  }
  .brand { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .mark {
    width: 22px; height: 22px; border-radius: 6px; display: grid; place-items: center;
    background: var(--raise); border: 1px solid var(--line);
  }
  .word { font-weight: 700; font-size: 13px; letter-spacing: 0.06em; }
  .crumb { color: var(--text-3); max-width: 240px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub {
    max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; color: var(--text-2);
    padding: 2px 8px; border-radius: 6px; background: var(--glass); border: 1px solid var(--line);
  }
  .center { position: absolute; left: 50%; transform: translateX(-50%); display: flex; }
  .right { margin-left: auto; display: flex; align-items: center; height: 100%; gap: 10px; }
  .wc { display: flex; height: 100%; }
  .wc button {
    width: 46px; height: 100%; border: 0; border-radius: 0; padding: 0; background: transparent;
    color: var(--text-2); display: grid; place-items: center;
  }
  .wc button:hover { background: rgba(255, 255, 255, 0.08); color: var(--text); }
  .wc button:active { transform: none; }
  .wc .close:hover { background: #c42b1c; color: #fff; }
</style>
