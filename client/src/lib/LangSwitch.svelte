<script lang="ts">
  import { i18n, setLang, type Lang } from "./i18n.svelte";

  // Dil seçici: Ayarlar ve giriş ekranı.
  const LANGS: [Lang, string][] = [["tr", "Türkçe"], ["en", "English"]];
</script>

<div class="lang" role="radiogroup" aria-label="Dil / Language">
  {#each LANGS as [id, label] (id)}
    <button role="radio" aria-checked={i18n.lang === id} class:on={i18n.lang === id} onclick={() => setLang(id)}>{label}</button>
  {/each}
</div>

<style>
  .lang { display: inline-flex; gap: 2px; padding: 2px; border-radius: 10px; background: var(--glass-2, rgba(255, 255, 255, 0.06)); }
  button { background: transparent; border: 0; padding: 6px 12px; border-radius: 8px; color: var(--text-2); }
  button.on { background: var(--glass-3, rgba(255, 255, 255, 0.14)); color: var(--text); }
</style>
