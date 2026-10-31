<script lang="ts">
  // Film kartı (docs/design/discovery.md §7.3): poster, puan rozeti, 1 sn üzerinde
  // durunca sessiz 10 sn fragman (youtube-nocookie). Hub/çekmecede video kutusu
  // yanında durduğu için gömülü oynatıcıyla çakışmaz.
  import type { Movie } from "./player";
  import { t } from "./i18n.svelte";

  let { movie, compact = false }: { movie: Movie; compact?: boolean } = $props();

  let trailer = $state(false);
  let wait: ReturnType<typeof setTimeout> | undefined;
  let stop: ReturnType<typeof setTimeout> | undefined;
  function enter() {
    if (!movie.trailer) return;
    clearTimeout(wait);
    wait = setTimeout(() => {
      trailer = true;
      stop = setTimeout(() => (trailer = false), 10_000);
    }, 1000);
  }
  function leave() {
    clearTimeout(wait);
    clearTimeout(stop);
    trailer = false;
  }
  $effect(() => {
    void movie.id; // yeni kartta fragman kapanır
    return leave;
  });

  const meta = $derived([movie.year, movie.runtime ? t("{n} dk", { n: movie.runtime }) : null, movie.genres?.slice(0, 2).join(", ")].filter(Boolean).join(" · "));
</script>

<div class="card" class:compact role="figure" onpointerenter={enter} onpointerleave={leave}>
  <div class="poster">
    {#if movie.poster}<img src={movie.poster} alt="" draggable="false" onerror={(e) => { (e.currentTarget as HTMLElement).style.display = "none"; }} />{:else}<div class="noposter">{movie.title}</div>{/if}
    {#if trailer && movie.trailer}
      <iframe title={t("Fragman")} referrerpolicy="strict-origin-when-cross-origin" allow="autoplay; encrypted-media"
        src="https://www.youtube-nocookie.com/embed/{movie.trailer}?autoplay=1&mute=1&controls=0&modestbranding=1&playsinline=1&rel=0"></iframe>
    {:else if movie.trailer && !compact}
      <span class="tr-hint">▶ {t("fragman için üzerinde dur")}</span>
    {/if}
  </div>
  <div class="info">
    <b class="title">{movie.title}</b>
    {#if movie.original}<small class="orig">{movie.original}</small>{/if}
    {#if meta}<small>{meta}</small>{/if}
    <div class="ratings">
      <span class:none={!movie.ratings.imdb} title="IMDb">IMDb {movie.ratings.imdb ?? "–"}</span>
      <span class:none={!movie.ratings.rt} title={t("Rotten Tomatoes eleştirmen")}>🍅 {movie.ratings.rt ?? "–"}</span>
      <span class:none={!movie.ratings.tmdb} title="TMDB">TMDB {movie.ratings.tmdb ? movie.ratings.tmdb.toFixed(1) : "–"}</span>
    </div>
    {#if movie.overview && !compact}<p class="overview">{movie.overview}</p>{/if}
  </div>
</div>

<style>
  .card { display: flex; flex-direction: column; border-radius: 16px; overflow: hidden; background: var(--glass); border: 1px solid var(--line); user-select: none; }
  .poster { position: relative; aspect-ratio: 2 / 3; background: #111; }
  .compact .poster { aspect-ratio: 16 / 9; }
  .poster img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .noposter { height: 100%; display: grid; place-items: center; padding: 16px; text-align: center; color: var(--text-2); font-weight: 600; }
  iframe { position: absolute; inset: 0; width: 100%; height: 100%; border: 0; background: #000; }
  .tr-hint { position: absolute; left: 8px; bottom: 8px; font-size: 11px; padding: 3px 8px; border-radius: 999px; background: rgba(0, 0, 0, 0.6); color: #ddd; }
  .info { padding: 10px 12px 12px; display: flex; flex-direction: column; gap: 3px; }
  .title { font-size: 15px; font-weight: 650; }
  .info small { color: var(--text-2); font-size: 12px; }
  .orig { font-style: italic; }
  .ratings { display: flex; gap: 6px; flex-wrap: wrap; margin-top: 4px; }
  .ratings span { font-size: 11px; font-weight: 600; padding: 2px 7px; border-radius: 6px; background: rgba(245, 197, 24, 0.16); color: #f5c518; }
  .ratings span:nth-child(2) { background: rgba(250, 50, 10, 0.16); color: #ff7a59; }
  .ratings span:nth-child(3) { background: rgba(1, 180, 228, 0.16); color: #4fc3f7; }
  .ratings span.none { background: rgba(255, 255, 255, 0.06); color: var(--text-3); }
  .overview { margin: 6px 0 0; color: var(--text-2); font-size: 12px; line-height: 1.45; display: -webkit-box; -webkit-line-clamp: 4; line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden; }
</style>
