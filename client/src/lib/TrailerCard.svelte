<script lang="ts">
  // 16:9 fragman kartı: önce bulanık afiş + ▶, tıklayınca YouTube gömmesi.
  // Ne izlesek künyesi ve film/dizi detay sayfası ortak kullanır.
  import { t } from "./i18n.svelte";

  let { title, poster, trailer, loading = false }: {
    title: string;
    poster?: string;
    /** YouTube video anahtarı. */
    trailer?: string;
    /** Künye henüz gelmedi: "yükleniyor", geldiyse ve fragman yoksa "bulunamadı". */
    loading?: boolean;
  } = $props();

  let on = $state(false);
  // Başka başlığa geçince fragman kapanır.
  $effect(() => { void trailer; void title; on = false; });

  const hue = (s: string) => [...s].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7);
  const grad = (s: string) => `linear-gradient(160deg, hsl(${hue(s)} 45% 32%), hsl(${hue(s)} 40% 8%))`;
</script>

<div class="trailer">
  {#if on && trailer}
    <iframe title={t("Fragman")} allow="autoplay; encrypted-media; fullscreen" allowfullscreen referrerpolicy="strict-origin-when-cross-origin"
      src="https://www.youtube-nocookie.com/embed/{trailer}?autoplay=1&modestbranding=1&playsinline=1&rel=0"></iframe>
  {:else}
    <div class="tr-img" style="--p:{grad(title)}">{#if poster}<img src={poster} alt="" referrerpolicy="no-referrer" />{/if}</div>
    {#if trailer}
      <button class="tr-play" onclick={() => (on = true)} title={t("Fragmanı oynat")}>▶</button>
      <span class="tr-cap">{t("Resmî fragman")}</span>
    {:else}
      <span class="tr-cap">{loading ? t("Künye yükleniyor…") : t("Fragman bulunamadı")}</span>
    {/if}
  {/if}
</div>

<style>
  .trailer { position: relative; width: 100%; aspect-ratio: 16 / 9; align-self: start; border-radius: 16px; overflow: hidden; background: #000; box-shadow: 0 30px 70px rgba(0, 0, 0, 0.6), 0 0 0 1px rgba(255, 255, 255, 0.08); }
  .trailer iframe { position: absolute; inset: 0; width: 100%; height: 100%; border: 0; }
  .tr-img { position: absolute; inset: 0; background: var(--p); }
  .tr-img img { position: absolute; inset: -10%; width: 120%; height: 120%; object-fit: cover; filter: blur(24px) brightness(0.6) saturate(1.3); }
  .tr-play { position: absolute; left: 50%; top: 50%; transform: translate(-50%, -50%); width: 74px; height: 74px; border-radius: 50%; border: 0; background: rgba(255, 255, 255, 0.92); color: #000; display: grid; place-items: center; font-size: 26px; padding-left: 5px; box-shadow: 0 10px 40px rgba(0, 0, 0, 0.6); transition: transform 0.2s cubic-bezier(0.34, 1.4, 0.5, 1); }
  .tr-play:hover { transform: translate(-50%, -50%) scale(1.08); }
  .tr-cap { position: absolute; left: 16px; bottom: 12px; font-size: 12px; color: rgba(255, 255, 255, 0.85); }
</style>
