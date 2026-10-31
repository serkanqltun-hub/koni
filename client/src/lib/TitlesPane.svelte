<script lang="ts">
  // Film & Dizi: Cinemeta'da ara, bölüm seç, kullanıcının
  // eklediği addon'lardan kaynak al, Real-Debrid ile aç. KONİ kaynak listelemez; kurulum
  // eksikse (anahtar ya da addon yok) ilk kurulum ekranı burada açılır.
  // Sinema'da kendi arama çubuğuyla (`withSearch`), Keşfet'te BrowseView'in çubuğuyla çalışır.
  import {
    addonStreams, addonsList, catalogEpisodes, catalogPopular, catalogSearch, debridStatus,
    type CatalogTitle, type Episode, type Source,
  } from "./player";
  import DebridSettings from "./DebridSettings.svelte";
  import Icon from "./Icon.svelte";
  import SourceHunt from "./SourceHunt.svelte";
  import ListButtons from "./ListButtons.svelte";
  import { rankSources, type SourceInfo } from "./sources";
  import { loadResume } from "./resume";
  import { t, tx } from "./i18n.svelte";

  let { inRoom, onPlay, onError, withSearch = false, onExit, initial = "" }: {
    inRoom: boolean;
    onPlay: (s: Source, title: string, alts?: Source[]) => void;
    onError: (msg: string) => void;
    /** Sinema: üstte geri + arama + Real-Debrid durumu. */
    withSearch?: boolean;
    /** Sonuç listesindeyken "← Sinema". */
    onExit?: () => void;
    /** Açılışta aranacak metin (lobi kutusundan). */
    initial?: string;
  } = $props();

  let setup = $state(false);
  /** Real-Debrid hazır mı, kaç addon var (durum çipi; ikisi de yoksa ilk kurulum). */
  let rdOk = $state<boolean | null>(null);
  let addonCount = $state(0);
  async function refreshSetup() {
    const [rd, ad] = await Promise.allSettled([debridStatus(), addonsList()]);
    rdOk = rd.status === "fulfilled" ? rd.value.configured && !rd.value.error : false;
    addonCount = ad.status === "fulfilled" ? ad.value.length : 0;
  }
  $effect(() => {
    refreshSetup().then(() => { if (withSearch && addonCount === 0) setup = true; });
  });

  let failedPosters = $state(new Set<string>());
  let popular = $state<CatalogTitle[]>([]);
  let popKind = $state<"all" | "movie" | "series">("all");
  let popLoading = $state(false);

  const shownPopular = $derived(
    popKind === "all" ? popular : popular.filter((x) => x.type === popKind)
  );

  async function loadPopular() {
    popLoading = true;
    try {
      const list = await catalogPopular();
      popular = [...list].sort(() => Math.random() - 0.5);
    } catch {
      // Cinemeta gecikmesi durumunda
    } finally {
      popLoading = false;
    }
  }

  function shufflePopular() {
    popular = [...popular].sort(() => Math.random() - 0.5);
  }

  $effect(() => {
    loadPopular();
  });

  let q = $state("");
  let titles = $state<CatalogTitle[] | null>(null);
  let picked = $state<CatalogTitle | null>(null);
  let episodes = $state<Episode[] | null>(null);
  let season = $state(1);
  let episode = $state<Episode | null>(null);
  let sources = $state<SourceInfo[] | null>(null);
  let errors = $state<string[]>([]);
  let loading = $state(false);
  let needAddon = $state(false);

  const seasons = $derived([...new Set((episodes ?? []).map((e) => e.season))]);
  const shownEps = $derived((episodes ?? []).filter((e) => e.season === season));
  const code = (e: Episode) => `S${String(e.season).padStart(2, "0")}E${String(e.episode).padStart(2, "0")}`;
  /** Odaya gidecek başlık: dizi bölümünde "Ad S01E02". */
  const label = $derived(picked ? (episode ? `${picked.name} ${code(episode)}` : picked.name) : "");
  /** Yarım kalan bölümler (başlığa göre): ilerleme çizgisi + "kaldığınız yer". */
  const resume = $derived.by(() => {
    void picked;
    return new Map(loadResume().map((r) => [r.title, r]));
  });

  /** BrowseView arama çubuğundan da çağrılır. */
  export async function search(text: string) {
    q = text;
    if (!text.trim()) {
      titles = null;
      return;
    }
    loading = true;
    picked = null;
    try { titles = await catalogSearch(text); } catch (e) { onError(t("Arama: {err}", { err: tx(String(e)) })); } finally { loading = false; }
  }
  $effect(() => {
    const s = initial;
    if (s.trim()) search(s);
  });

  async function pick(ti: CatalogTitle) {
    picked = ti;
    episodes = null;
    episode = null;
    sources = null;
    if (ti.type === "movie") return loadSources("movie", ti.id);
    loading = true;
    try {
      episodes = await catalogEpisodes(ti.id);
      // Yarım kalan son bölüm varsa o sezon açılır.
      const last = episodes.find((e) => resume.has(`${ti.name} ${code(e)}`));
      season = last?.season ?? episodes.find((e) => e.season > 0)?.season ?? episodes[0]?.season ?? 1;
    } catch (e) {
      onError(t("Bölümler: {err}", { err: tx(String(e)) }));
    } finally {
      loading = false;
    }
  }

  function pickEpisode(e: Episode) {
    episode = e;
    loadSources("series", e.id);
  }

  async function loadSources(kind: "movie" | "series", id: string) {
    loading = true;
    sources = null;
    errors = [];
    needAddon = false;
    try {
      const r = await addonStreams(kind, id);
      sources = rankSources(r.sources);
      errors = r.errors;
    } catch (e) {
      // Addon yoksa komut bunu söyler; düğmeyle kuruluma gönder.
      needAddon = true;
      onError(tx(String(e)));
    } finally {
      loading = false;
    }
  }

  /** Kurulum kapanınca, kaynak isteği addon yokluğundan düştüyse yeniden dene. */
  function closeSetup() {
    setup = false;
    refreshSetup();
    if (!needAddon || !picked) return;
    if (picked.type === "movie") loadSources("movie", picked.id);
    else if (episode) loadSources("series", episode.id);
    else needAddon = false;
  }
  function back() {
    if (setup) return closeSetup();
    if (picked) { picked = null; return; }
    onExit?.();
  }
  const left = (r: { pos: number; duration: number }) => {
    const m = Math.round((r.duration - r.pos) / 60);
    return m >= 60 ? t("{h} sa {m} dk kaldı", { h: Math.floor(m / 60), m: m % 60 }) : t("{m} dk kaldı", { m });
  };
  /** Afiş yoksa addan sabit renk. */
  const tint = (name: string) => [...name].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7);
</script>

<div class="fd" class:solo={withSearch}>
{#snippet posterImg(ti: CatalogTitle)}
  <span class="img" style="--h:{tint(ti.name)}">
    <u>{ti.type === "series" ? t("DİZİ") : t("FİLM")}</u>
    {#if ti.poster && ti.poster.trim() && !failedPosters.has(ti.id)}
      <img
        src={ti.poster}
        alt=""
        loading="lazy"
        referrerpolicy="no-referrer"
        onerror={(e) => {
          (e.currentTarget as HTMLElement).style.display = "none";
          failedPosters.add(ti.id);
        }}
      />
    {:else}
      <div class="ph">
        <Icon name={ti.type === "series" ? "activity" : "film"} size={26} />
        <span class="ph-t">{ti.name}</span>
      </div>
    {/if}
  </span>
{/snippet}
{#if withSearch}
  <div class="fd-top">
    {#if onExit || picked || setup}
      <button class="back" onclick={back}><Icon name="back" size={14} />{setup ? t("Geri") : picked ? t("Sonuçlar") : t("Sinema")}</button>
    {/if}
    <form class="q" onsubmit={(e) => { e.preventDefault(); search(q); }}>
      <Icon name="search" size={17} />
      <input class="bare" placeholder={t("Film ya da dizi ara")} bind:value={q} oninput={() => { if (!q.trim()) titles = null; }} />
      {#if q}
        <button type="button" class="clear-q" onclick={() => { q = ""; titles = null; }} title={t("Aramayı temizle")}><Icon name="x" size={14} /></button>
      {/if}
      <em>{t("Cinemeta · film ve dizi")}</em>
    </form>
    <button class="gear" onclick={() => (setup ? closeSetup() : (setup = true))} title={t("Sağlayıcı & addon'lar")}>
      <i class:off={addonCount === 0}></i>
      {rdOk === null ? t("Yükleniyor…") : addonCount === 0 ? t("Addon yok · Kurulum") : rdOk ? t("Sağlayıcı bağlı · {n} addon", { n: addonCount }) : t("{n} addon bağlı", { n: addonCount })}
    </button>
  </div>
{:else if !setup}
  <button class="small corner" onclick={() => (setup = true)}><Icon name="gear" size={13} /> {t("Sağlayıcı & addon'lar")}</button>
{/if}

{#if setup}
  <div class="stp">
    {#if !withSearch}<button class="small back2" onclick={closeSetup}><Icon name="back" size={13} /> {t("Geri")}</button>{/if}
    <span class="kick">{t("Film & Dizi · kurulum")}</span>
    <h2>{t("Kendi hesabınla izle")}</h2>
    <DebridSettings onChange={refreshSetup} />
    {#if addonCount > 0}<button class="white done" onclick={closeSetup}>{t("Hazır, aramaya dön")}</button>{/if}
  </div>
{:else if picked}
  {@const meta = [picked.releaseInfo, picked.type === "series" && episodes ? t("{n} sezon", { n: seasons.filter((s) => s > 0).length }) : null,
    picked.imdbRating ? `IMDb ${picked.imdbRating}` : null, picked.genres?.slice(0, 2).join(" · ")].filter(Boolean)}
  <div class="tt">
    <div class="ps" style="--h:{tint(picked.name)}">
      {#if picked.poster && picked.poster.trim() && !failedPosters.has(picked.id)}
        <img
          src={picked.poster}
          alt=""
          referrerpolicy="no-referrer"
          onerror={(e) => {
            (e.currentTarget as HTMLElement).style.display = "none";
            if (picked) failedPosters.add(picked.id);
          }}
        />
      {:else}
        <div class="ph-lg">
          <Icon name={picked.type === "series" ? "activity" : "film"} size={44} />
          <span>{picked.name}</span>
        </div>
      {/if}
    </div>
    <div class="tt-r">
      {#if !withSearch}<button class="small back2" onclick={() => (picked = null)}><Icon name="back" size={13} /> {t("Sonuçlar")}</button>{/if}
      <h2>{picked.name}</h2>
      <div class="meta">{#each meta as m, i (i)}{#if i}<i></i>{/if}<span>{m}</span>{/each}</div>
      {#if picked.id.startsWith("tt")}
        {@const y = parseInt(picked.releaseInfo ?? "", 10)}
        <ListButtons entry={{ ref: picked.id, kind: picked.type, title: picked.name, year: Number.isFinite(y) ? y : undefined,
          poster: picked.poster?.startsWith("https://") ? picked.poster : undefined, source: "titles" }} />
      {/if}

      {#if episodes}
        <div class="seas">
          {#each seasons as s (s)}
            <button class:on={s === season} onclick={() => (season = s)}>{s === 0 ? t("Özel") : t("{n}. sezon", { n: s })}</button>
          {/each}
        </div>
        <div class="eps">
          {#each shownEps as e (e.id)}
            {@const r = resume.get(`${picked.name} ${code(e)}`)}
            <button class="ep" class:on={episode?.id === e.id} onclick={() => pickEpisode(e)}>
              <span class="n">{e.episode}</span>
              {#if e.thumbnail}<span class="th" style="--w:{r ? (r.pos / r.duration) * 100 : 0}%"><img src={e.thumbnail} alt="" loading="lazy" referrerpolicy="no-referrer" onerror={(e) => { (e.currentTarget as HTMLElement).style.display = "none"; }} /></span>{/if}
              <span class="et"><b>{e.name || t("{n}. bölüm", { n: e.episode })}</b>{#if r}<small class="pink">{left(r)} · {t("kaldığınız yer")}</small>{/if}</span>
            </button>
          {/each}
        </div>
      {/if}

      {#if loading}
        <div class="intro"><span>{t("Kaynaklar addon'lardan alınıyor…")}</span></div>
      {:else if needAddon}
        <div class="intro">
          <span>{t("Kaynaklar senin eklediğin addon'lardan gelir.")}</span>
          <button class="white" onclick={() => (setup = true)}>{t("Addon ekle")}</button>
        </div>
      {:else if sources}
        {#if sources.length === 0}
          <div class="intro"><span>{t("Addon'ların bu başlık için kaynak vermedi.")}</span></div>
        {:else}
          <SourceHunt {sources} {inRoom} onAddon={() => (setup = true)}
            kicker={episode ? t("{code} · {s}. sezon, {e}. bölüm", { code: code(episode), s: episode.season, e: episode.episode }) : [t("Film"), picked.releaseInfo].filter(Boolean).join(" · ")}
            heading={episode ? episode.name || t("{n}. bölüm", { n: episode.episode }) : picked.name}
            onPlay={(src) => onPlay(src, label)} />
        {/if}
        {#each errors as e (e)}<p class="warn">{tx(e)}</p>{/each}
      {:else if episodes}
        <div class="hint">{t("Bir bölüm seç; kaynaklar senin addon'larından gelir.")}</div>
      {/if}
    </div>
  </div>
{:else if loading}
  <div class="intro"><span>{t("Aranıyor…")}</span></div>
{:else if titles === null}
  <div class="pop-header">
    <div class="pop-left">
      <b>{t("Film & Dizi Galerisi")}</b>
      <div class="pop-tabs">
        <button class:on={popKind === "all"} onclick={() => (popKind = "all")}>{t("Tümü")}</button>
        <button class:on={popKind === "movie"} onclick={() => (popKind = "movie")}>{t("Filmler")}</button>
        <button class:on={popKind === "series"} onclick={() => (popKind = "series")}>{t("Diziler")}</button>
      </div>
    </div>
    <button class="rnd-btn" onclick={shufflePopular} title={t("Rastgele karıştır")}>
      <Icon name="shuffle" size={13} /> {t("Rastgele karıştır")}
    </button>
  </div>

  {#if popLoading}
    <div class="intro"><span>{t("Popüler film ve diziler yükleniyor…")}</span></div>
  {:else if shownPopular.length}
    <div class="pgrid">
      {#each shownPopular as ti (ti.id)}
        <button class="pg" title={ti.name} onclick={() => pick(ti)}>
          {@render posterImg(ti)}
          <b>{ti.name}</b>
          <small>{[ti.releaseInfo, ti.imdbRating ? `IMDb ${ti.imdbRating}` : null].filter(Boolean).join(" · ")}</small>
        </button>
      {/each}
    </div>
  {:else}
    <div class="intro">
      <b>{t("Film & Dizi")}</b>
      <span>{t("Yukarıdan film ya da dizi ara. Kaynaklar senin eklediğin addon'lardan gelir ve kendi hesabınla açılır; odada herkes kendi hesabıyla izler.")}</span>
    </div>
  {/if}
{:else if titles.length === 0}
  <div class="intro"><span>{t("Sonuç yok.")}</span></div>
{:else}
  <div class="lab">{t("Sonuçlar")} <span>{t("Afişe tıkla: kaynaklar senin addon'larından gelir")}</span></div>
  <div class="pgrid">
    {#each titles as ti (ti.id)}
      <button class="pg" title={ti.name} onclick={() => pick(ti)}>
        {@render posterImg(ti)}
        <b>{ti.name}</b>
        <small>{[ti.releaseInfo, ti.imdbRating ? `IMDb ${ti.imdbRating}` : null].filter(Boolean).join(" · ")}</small>
      </button>
    {/each}
  </div>
{/if}
</div>

<style>
  .fd { display: flex; flex-direction: column; gap: 18px; min-height: 0; }
  /* Sinema'da tüm sahne: yeşilimsi zemin, kendi kaydırması. */
  .fd.solo {
    position: absolute; inset: 0; padding: 26px 40px; overflow-y: auto;
    background: radial-gradient(60% 50% at 15% 0%, rgba(48, 209, 88, 0.1), transparent 70%), #08090b;
  }
  .fd-top { display: flex; align-items: center; gap: 12px; flex: none; }
  .back { height: 40px; padding: 0 12px; border-radius: 11px; color: var(--text-2); font-weight: 600; display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; }
  .q {
    flex: 1; height: 46px; border-radius: 14px; background: rgba(255, 255, 255, 0.07); border: 1px solid rgba(255, 255, 255, 0.2);
    display: flex; align-items: center; gap: 10px; padding: 0 16px; color: var(--text-2);
  }
  .q:focus-within { border-color: rgba(48, 209, 88, 0.6); }
  .q input.bare, .q input.bare:focus-visible { flex: 1; min-width: 0; height: 100%; background: none; border: 0; padding: 0; font-size: 14.5px; color: var(--text); box-shadow: none; outline: none; border-radius: 0; }
  .q em { font-style: normal; color: var(--text-3); font-size: 12px; white-space: nowrap; }
  .gear { height: 46px; padding: 0 14px; border-radius: 14px; display: flex; align-items: center; gap: 8px; color: var(--text-2); font-weight: 600; white-space: nowrap; }
  .gear i { width: 8px; height: 8px; border-radius: 50%; background: var(--green); box-shadow: 0 0 8px var(--green); }
  .gear i.off { background: var(--orange, #ff9f0a); box-shadow: 0 0 8px var(--orange, #ff9f0a); }
  /* Keşfet: BrowseView'in sekme satırının sağ ucu (.scroll konumlandırılmış kapsayıcı). */
  .corner { position: absolute; top: 16px; right: 18px; height: 30px; display: inline-flex; gap: 5px; align-items: center; }
  .small { padding: 4px 10px; font-size: 12px; border-radius: 8px; }
  .back2 { align-self: flex-start; display: inline-flex; gap: 4px; align-items: center; }

  .intro { display: flex; flex-direction: column; gap: 10px; align-items: center; text-align: center; margin: 8vh auto 0; max-width: 440px; color: var(--text-2); }
  .intro b { font-size: 18px; color: var(--text); letter-spacing: -0.01em; }
  .lab { display: flex; justify-content: space-between; gap: 12px; color: var(--text-2); font-size: 12px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.06em; }
  .lab span { text-transform: none; letter-spacing: 0; color: var(--text-3); font-weight: 500; }
  .hint { color: var(--text-3); font-size: 11.5px; }
  .warn { margin: 0; font-size: 12px; color: var(--text-3); }

  .pg img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .pg img:not([src]), .pg img[src=""] { display: none !important; }
  .ps img:not([src]), .ps img[src=""] { display: none !important; }

  .ph {
    position: absolute; inset: 0; display: flex; flex-direction: column;
    align-items: center; justify-content: center; gap: 8px; padding: 28px 10px 12px;
    text-align: center; color: rgba(255, 255, 255, 0.9);
  }
  .ph :global(svg) { opacity: 0.4; color: #fff; }
  .ph-t {
    font-size: 13px; font-weight: 750; line-height: 1.25; color: #fff;
    display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden;
  }
  .ph-lg {
    position: absolute; inset: 0; display: flex; flex-direction: column;
    align-items: center; justify-content: center; gap: 14px; padding: 24px;
    text-align: center; color: #fff;
  }
  .ph-lg :global(svg) { opacity: 0.45; }
  .ph-lg span { font-size: 22px; font-weight: 850; line-height: 1.2; letter-spacing: -0.02em; }

  .pgrid { display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 18px 14px; }
  .pg { display: flex; flex-direction: column; gap: 5px; padding: 0; border: 0; background: transparent; text-align: left; min-width: 0; }
  .pg .img {
    position: relative; aspect-ratio: 2 / 3; border-radius: 11px; overflow: hidden; display: flex; align-items: flex-end; padding: 10px;
    background: linear-gradient(165deg, hsl(var(--h, 220) 45% 32%), hsl(var(--h, 220) 50% 5%)); box-shadow: 0 14px 34px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.07);
    font-weight: 900; font-size: 15px; line-height: 1; transition: box-shadow 0.2s, transform 0.25s;
  }
  .pg:hover .img { box-shadow: 0 0 0 3px #fff, 0 14px 34px rgba(0, 0, 0, 0.5); transform: translateY(-2px); }
  .pg img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .pg u { position: absolute; z-index: 1; top: 8px; left: 8px; text-decoration: none; font-size: 10px; font-weight: 800; padding: 2px 7px; border-radius: 6px; background: rgba(0, 0, 0, 0.6); backdrop-filter: blur(6px); }
  .pg b { font-size: 13px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; margin-top: 4px; }
  .pg small { color: var(--text-3); font-size: 11.5px; }

  .tt { display: grid; grid-template-columns: minmax(150px, 220px) minmax(0, 1fr); gap: 30px; align-items: start; }
  .ps {
    position: sticky; top: 0; aspect-ratio: 2 / 3; border-radius: 14px; overflow: hidden; display: flex; align-items: flex-end; padding: 18px;
    background: linear-gradient(165deg, hsl(var(--h, 220) 45% 32%), hsl(var(--h, 220) 50% 5%)); box-shadow: 0 30px 70px rgba(0, 0, 0, 0.6), 0 0 0 1px rgba(255, 255, 255, 0.08);
    font-weight: 900; font-size: 26px; line-height: 1;
  }
  .ps img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .tt-r { display: flex; flex-direction: column; gap: 12px; min-width: 0; }
  .tt-r h2 { margin: 0; font-size: 36px; font-weight: 900; letter-spacing: -0.035em; line-height: 1.05; }
  .meta { display: flex; flex-wrap: wrap; gap: 10px; color: var(--text-2); font-size: 13.5px; align-items: center; }
  .meta i { width: 4px; height: 4px; border-radius: 50%; background: var(--text-3); }

  .seas { display: flex; flex-wrap: wrap; gap: 6px; }
  .seas button { padding: 5px 12px; height: auto; border-radius: 99px; font-weight: 600; font-size: 12.5px; color: var(--text-2); }
  .seas button.on { background: #fff; color: #000; border-color: #fff; }
  .eps { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 8px; }
  .ep { display: flex; gap: 12px; align-items: center; text-align: left; padding: 8px; border-radius: 12px; min-width: 0; height: auto; }
  .ep.on { border-color: rgba(10, 132, 255, 0.6); background: rgba(10, 132, 255, 0.12); }
  .ep .n { font: 700 12px var(--font-mono); color: var(--text-3); width: 20px; text-align: center; flex: none; }
  .ep .th { position: relative; width: 104px; height: 58px; border-radius: 8px; overflow: hidden; flex: none; background: #0d1218; }
  .ep .th img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .ep .th::after { content: ""; position: absolute; left: 0; bottom: 0; height: 3px; width: var(--w, 0); background: #ff375f; }
  .et { display: flex; flex-direction: column; min-width: 0; line-height: 1.3; }
  .et b { font-size: 13px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .et small { color: var(--text-3); font-size: 11.5px; }
  .et small.pink { color: #ff6f8e; }

  .pop-header { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-top: 4px; }
  .pop-left { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .pop-left b { font-size: 15px; font-weight: 750; letter-spacing: -0.01em; }
  .pop-tabs { display: flex; gap: 6px; }
  .pop-tabs button {
    height: 30px; padding: 0 12px; border-radius: 99px; font-size: 12px; font-weight: 600;
    color: var(--text-2); background: rgba(255, 255, 255, 0.05); border: 1px solid var(--line);
    cursor: pointer; transition: background 0.15s, color 0.15s;
  }
  .pop-tabs button:hover { color: var(--text); background: rgba(255, 255, 255, 0.1); }
  .pop-tabs button.on { background: #fff; color: #000; border-color: #fff; }
  .rnd-btn {
    height: 32px; padding: 0 12px; border-radius: 10px; font-size: 12px; font-weight: 600;
    color: var(--text-2); background: rgba(255, 255, 255, 0.06); border: 1px solid var(--line-2);
    display: inline-flex; align-items: center; gap: 6px; cursor: pointer; transition: background 0.15s, color 0.15s;
  }
  .rnd-btn:hover { background: rgba(255, 255, 255, 0.12); color: var(--text); }
  .clear-q { background: none; border: 0; padding: 4px; color: var(--text-3); cursor: pointer; display: flex; align-items: center; }
  .clear-q:hover { color: var(--text); }

  .stp { max-width: 640px; width: 100%; margin: 0 auto; display: flex; flex-direction: column; gap: 12px; }
  .fd.solo .stp { margin-top: 2vh; }
  .kick { color: var(--text-2); font-size: 12px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em; }
  .stp h2 { margin: 0; font-size: 30px; font-weight: 900; letter-spacing: -0.03em; }
  .done { align-self: flex-start; margin-top: 6px; }
</style>
