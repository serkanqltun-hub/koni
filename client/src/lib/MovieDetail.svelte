<script lang="ts" module>
  import type { TitleDetail } from "./player";
  /** Aynı oturumda geri dönünce yeniden sorulmasın. */
  const cache = new Map<string, TitleDetail>();
</script>

<script lang="ts">
  // Film/dizi detay sayfası (p0.25 `detail`): odaya girmeden künye, fragman ve "Benzerleri".
  // Sunucu künye vermezse (eski sunucu, hesap yok) elimizdeki başlık/afişle sade hali görünür.
  // Kaynak avı ve bölüm listesi `children` olarak TitlesPane'den gelir.
  import type { Snippet } from "svelte";
  import { detailGet, type DetailRef, type ListEntry, type Movie } from "./player";
  import { social } from "./social.svelte";
  import ListButtons from "./ListButtons.svelte";
  import TrailerCard from "./TrailerCard.svelte";
  import Icon from "./Icon.svelte";
  import { t, i18n } from "./i18n.svelte";

  let { title: base, watching = false, onWatch, onOpen, onImdb, top, children }: {
    title: DetailRef;
    /** Kaynak aranıyor/oynatılıyor: İzle düğmesi kilitli. */
    watching?: boolean;
    onWatch: () => void;
    /** Benzerleri rafından bir başlık. */
    onOpen: (d: DetailRef) => void;
    /** `tmdb:` ile açılan başlığın IMDb kimliği geldi (kaynaklar ona göre aranır). */
    onImdb?: (imdb: string) => void;
    /** Başlığın üstünde (ör. geri düğmesi). */
    top?: Snippet;
    children?: Snippet;
  } = $props();

  let res = $state<TitleDetail | null>(null);
  let loading = $state(false);
  $effect(() => {
    const { ref, kind } = base;
    const url = social.server;
    const hit = cache.get(ref) ?? null;
    res = hit;
    loading = false;
    if (hit || !url) return;
    let live = true;
    loading = true;
    detailGet(url, ref, kind).then(
      (r) => {
        if (r?.item) { cache.set(ref, r); if (r.item.imdb) cache.set(r.item.imdb, r); }
        if (live) res = r;
      },
      () => {},
    ).finally(() => { if (live) loading = false; });
    return () => { live = false; };
  });
  const item = $derived<Movie | undefined>(res?.item);
  const similar = $derived(res?.similar ?? []);
  $effect(() => { if (item?.imdb && !base.ref.startsWith("tt")) onImdb?.(item.imdb); });

  const tv = $derived(base.kind === "series");
  const name = $derived(item?.title || base.title);
  const poster = $derived(item?.poster || base.poster);
  const listRef = $derived(item?.imdb || base.ref.replace(/^tmdb:tv:/, "tmdb:series:"));
  const entry = $derived<ListEntry>({ ref: listRef, kind: base.kind, title: name, year: item?.year ?? base.year,
    poster: poster?.startsWith("https://") ? poster : undefined, source: "titles" });

  function dur(min: number) {
    const h = Math.floor(min / 60), m = min % 60;
    return h ? t("{n} sa", { n: h }) + (m ? " " + t("{n} dk", { n: m }) : "") : t("{n} dk", { n: m });
  }
  const votes = (v?: number) => (!v ? "" : v >= 1e6 ? t("{n} M oy", { n: (v / 1e6).toFixed(1).replace(".", i18n.lang === "tr" ? "," : ".") }) : v >= 1000 ? t("{n} B oy", { n: Math.round(v / 1000) }) : t("{n} oy", { n: v }));
  const meta = $derived([
    item?.year ?? base.year,
    item?.runtime ? (tv ? t("bölüm {n} dk", { n: item.runtime }) : dur(item.runtime)) : null,
    item?.seasons ? (item.seasons === 1 ? t("1 sezon") : t("{n} sezon", { n: item.seasons })) : null,
  ].filter(Boolean) as (string | number)[]);
  const imdbOrTmdb = $derived(item ? item.ratings.imdb ?? (item.ratings.tmdb ? item.ratings.tmdb.toFixed(1) : "") : "");

  function openSimilar(m: Movie) {
    const kind = m.kind === "tv" ? "series" : "movie";
    onOpen({ ref: m.imdb || `tmdb:${m.kind === "tv" ? "tv" : "movie"}:${m.id}`, kind, title: m.title, poster: m.poster, year: m.year });
  }
  const hue = (s: string) => [...s].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7);
</script>

<div class="md">
  {#if item?.backdrop}<img class="bd" src={item.backdrop} alt="" referrerpolicy="no-referrer" />{/if}
  <div class="tt">
    <div class="ps" style="--h:{hue(name)}">
      {#if poster}<img src={poster} alt="" referrerpolicy="no-referrer" onerror={(e) => { (e.currentTarget as HTMLElement).style.display = "none"; }} />{/if}
      <div class="ph-lg"><Icon name={tv ? "activity" : "film"} size={44} /><span>{name}</span></div>
    </div>
    <div class="tt-r">
      {@render top?.()}
      <div class="head">
        <div class="info">
          <span class="kick">{tv ? t("DİZİ") : t("FİLM")}</span>
          <h2>{name}</h2>
          {#if item?.original && item.original !== name}<div class="orig">{item.original}</div>{/if}
          <div class="meta">
            {#each meta as x, i (i)}{#if i}<i></i>{/if}<span>{x}</span>{/each}
            {#each (item?.genres ?? []).slice(0, 3) as g (g)}<span class="genre">{t(g)}</span>{/each}
          </div>
          {#if item}
            <div class="ratings">
              {#if imdbOrTmdb}<div class="rt"><span class="lg" style="background:#f5c518">{item.ratings.imdb ? "IMDb" : "TMDB"}</span><div><b>{imdbOrTmdb}</b><small>{votes(item.votes) || t("puan")}</small></div></div>{/if}
              {#if item.ratings.rt}<div class="rt"><span class="lg" style="background:#fa320a;color:#fff">RT</span><div><b>{item.ratings.rt}</b><small>{t("eleştirmen")}</small></div></div>{/if}
              {#if item.ratings.meta}<div class="rt"><span class="lg" style="background:#66cc33">MC</span><div><b>{item.ratings.meta}</b><small>Metacritic</small></div></div>{/if}
            </div>
          {/if}
          {#if item?.overview}<p class="ov">{item.overview}</p>{/if}
          {#if item?.director || item?.cast?.length}
            <dl class="kunye">
              {#if item.director}<dt>{tv ? t("Yaratıcı") : t("Yönetmen")}</dt><dd>{item.director}</dd>{/if}
              {#if item.cast?.length}<dt>{t("Oyuncular")}</dt><dd>{item.cast.slice(0, 6).join(", ")}</dd>{/if}
            </dl>
          {/if}
          <div class="acts">
            <button class="white play" disabled={watching} onclick={onWatch}>▶ {t("İzle")}</button>
            <ListButtons {entry} />
          </div>
        </div>
        {#if item?.trailer || loading}
          <div class="tr"><TrailerCard title={name} {poster} trailer={item?.trailer} {loading} /></div>
        {/if}
      </div>
      {@render children?.()}
    </div>
  </div>

  {#if similar.length}
    <div class="sim">
      <div class="lab">{t("Benzerleri")}</div>
      <div class="row">
        {#each similar as m (m.id)}
          <button class="sc" title={m.title} onclick={() => openSimilar(m)}>
            <span class="img" style="--h:{hue(m.title)}">
              {#if m.poster}<img src={m.poster} alt="" loading="lazy" referrerpolicy="no-referrer" onerror={(e) => { (e.currentTarget as HTMLElement).style.display = "none"; }} />{/if}
              <em>{m.title}</em>
            </span>
            <b>{m.title}</b>
            <small>{[m.year, m.ratings?.tmdb ? `TMDB ${m.ratings.tmdb.toFixed(1)}` : null].filter(Boolean).join(" · ")}</small>
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .md { position: relative; display: flex; flex-direction: column; gap: 30px; }
  .bd {
    position: absolute; inset: 0 0 auto; width: 100%; height: 420px; z-index: 0; pointer-events: none;
    object-fit: cover; object-position: center 25%; opacity: 0.35; border-radius: 18px;
    mask-image: linear-gradient(to bottom, #000 35%, transparent); -webkit-mask-image: linear-gradient(to bottom, #000 35%, transparent);
  }
  .tt, .sim { position: relative; z-index: 1; }
  .tt { display: grid; grid-template-columns: minmax(150px, 220px) minmax(0, 1fr); gap: 30px; align-items: start; }
  .ps {
    position: sticky; top: 0; aspect-ratio: 2 / 3; border-radius: 14px; overflow: hidden;
    background: linear-gradient(165deg, hsl(var(--h, 220) 45% 32%), hsl(var(--h, 220) 50% 5%)); box-shadow: 0 30px 70px rgba(0, 0, 0, 0.6), 0 0 0 1px rgba(255, 255, 255, 0.08);
  }
  .ps img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; z-index: 1; }
  .ph-lg { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; padding: 24px; text-align: center; color: #fff; }
  .ph-lg :global(svg) { opacity: 0.45; }
  .ph-lg span { font-size: 22px; font-weight: 850; line-height: 1.2; letter-spacing: -0.02em; }
  .tt-r { display: flex; flex-direction: column; gap: 14px; min-width: 0; }
  .head { display: flex; flex-wrap: wrap; gap: 24px; align-items: flex-start; }
  .info { flex: 1 1 340px; display: flex; flex-direction: column; gap: 11px; min-width: 0; }
  .tr { flex: 0 1 420px; min-width: 280px; display: flex; flex-direction: column; }
  .kick, .lab { color: var(--text-2); font-size: 12px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.08em; }
  h2 { margin: 0; font-size: 36px; font-weight: 900; letter-spacing: -0.035em; line-height: 1.05; }
  .orig { color: var(--text-2); font-style: italic; margin-top: -6px; }
  .meta { display: flex; gap: 10px; align-items: center; color: rgba(255, 255, 255, 0.8); font-size: 14px; flex-wrap: wrap; }
  .meta i { width: 4px; height: 4px; border-radius: 50%; background: var(--text-3); }
  .genre { padding: 3px 10px; border-radius: 99px; border: 1px solid var(--line-2); font-size: 12px; color: var(--text-2); }
  .ratings { display: flex; gap: 10px; flex-wrap: wrap; }
  .rt { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-radius: 12px; background: var(--glass); border: 1px solid var(--line); }
  .rt > div { display: flex; flex-direction: column; line-height: 1.1; }
  .rt b { font-size: 18px; font-weight: 800; }
  .rt small { color: var(--text-2); font-size: 11px; }
  .rt .lg { width: 30px; height: 26px; border-radius: 6px; display: grid; place-items: center; font-weight: 900; font-size: 9.5px; color: #000; }
  .ov { margin: 0; color: rgba(255, 255, 255, 0.85); line-height: 1.55; font-size: 14px; max-width: 70ch; }
  .kunye { display: grid; grid-template-columns: auto 1fr; gap: 4px 14px; margin: 0; font-size: 13px; }
  .kunye dt { color: var(--text-3); }
  .kunye dd { margin: 0; color: var(--text-2); }
  .acts { display: flex; flex-direction: column; gap: 10px; align-items: flex-start; margin-top: 4px; }
  .play { height: 42px; padding: 0 26px; border-radius: 12px; font-size: 15px; font-weight: 700; }
  .sim { display: flex; flex-direction: column; gap: 12px; }
  .row { display: flex; gap: 14px; overflow-x: auto; padding-bottom: 8px; }
  .sc { flex: 0 0 132px; display: flex; flex-direction: column; gap: 3px; text-align: left; padding: 0; background: none; border: 0; height: auto; min-width: 0; }
  .sc .img {
    position: relative; aspect-ratio: 2 / 3; border-radius: 11px; overflow: hidden; display: flex; align-items: flex-end; padding: 10px;
    background: linear-gradient(165deg, hsl(var(--h, 220) 45% 32%), hsl(var(--h, 220) 50% 5%)); box-shadow: 0 14px 34px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.07);
    transition: box-shadow 0.2s, transform 0.25s;
  }
  .sc:hover .img { box-shadow: 0 0 0 3px #fff, 0 14px 34px rgba(0, 0, 0, 0.5); transform: translateY(-2px); }
  .sc img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; z-index: 1; }
  .sc em { font-style: normal; font-weight: 850; font-size: 13px; line-height: 1.2; color: #fff; }
  .sc b { font-size: 13px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; margin-top: 4px; }
  .sc small { color: var(--text-3); font-size: 11.5px; }
</style>
