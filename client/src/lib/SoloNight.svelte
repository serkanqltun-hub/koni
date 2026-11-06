<script lang="ts">
  // "Ne izlesem?" (PROTOCOL §3.0k): tek kişilik karar sihirbazı, solo lobiden açılır.
  // Ne izlesek? (NightView) ile aynı kabuk ve adım dili:
  // 1 Kur (film/dizi, hava, süre/dönem/puan, özel filtre, dublaj/altyazı) → 2 Oyla
  // (sunucudan deste; sağa/sola kaydır, her kart bitince `solo_night_pick` sıradakini
  // getirir) → 3 Çark (beğenilen 1 ise doğrudan künye; 2+ ise afişli çark, sonuç
  // Eşleşme ekranı diliyle) → 4 İzle (künye; Film & Dizi açılır, kaynak kendiliğinden
  // aranır). Açıkken video penceresi gizlidir (App).
  import { fade, fly, scale } from "svelte/transition";
  import { cubicOut, backOut } from "svelte/easing";
  import { browseGet, soloNight, soloNightPick, type BrowseQuery, type DetailRef, type Movie, type NightFilter, type NightKind } from "./player";
  import { social } from "./social.svelte";
  import { MOODS, YEAR, ERAS, GENRES, LANGS, COUNTS, type Draft, runtimeRange, blankDraft as blank, draftFilter as toFilter } from "./night/moods";
  import SwipeCard from "./night/SwipeCard.svelte";
  import WheelDisc from "./night/WheelDisc.svelte";
  import DualRange from "./DualRange.svelte";
  import MovieDetail from "./MovieDetail.svelte";
  import type { Prefer } from "./sources";
  import { t, locale, i18n } from "./i18n.svelte";

  let { hue, onClose, onError, onWatch }: {
    hue: (name: string) => number;
    onClose: () => void;
    onError: (msg: string) => void;
    /** Seçilen başlığı izle: Film & Dizi'de açılır; `prefer` kaynak avında TR dublaj/altyazı önce. */
    onWatch: (d: DetailRef, prefer: Prefer) => void;
  } = $props();

  type Phase = "setup" | "deck" | "wheel" | "detail";
  let phase = $state<Phase>("setup");
  const STEPS: [Phase, string][] = [["setup", "Kur"], ["deck", "Oyla"], ["wheel", "Çark"], ["detail", "İzle"]];
  const stepIdx = $derived(STEPS.findIndex((s) => s[0] === phase));

  // ---- 1 · Kur ---------------------------------------------------------------
  let kind = $state<NightKind>("movie");
  let moods = $state<string[]>([]);
  let maxRt = $state(0); // dakika; 0 = fark etmez
  let eras = $state<string[]>([]);
  let vote = $state(0);
  let custom = $state<NightFilter | null>(null);
  let panel = $state(false);
  let prefer = $state<Prefer>("");
  const tv = $derived(kind === "tv");
  const noun = $derived(tv ? t("dizi") : t("film"));
  const RT = $derived(runtimeRange(tv));
  const blankDraft = () => blank(tv);
  let draft = $state<Draft>(blankDraft());
  $effect(() => { void tv; maxRt = 0; draft = blankDraft(); custom = null; });

  function toggleMood(id: string) {
    custom = null;
    moods = moods.includes(id) ? moods.filter((m) => m !== id) : [...moods.slice(-1), id]; // en fazla 2
  }
  const toggleEra = (id: string) => (eras = eras.includes(id) ? eras.filter((e) => e !== id) : [...eras, id]);
  function cycleGenre(id: number) {
    if (draft.inc.includes(id)) { draft.inc = draft.inc.filter((g) => g !== id); draft.exc = [...draft.exc, id]; }
    else if (draft.exc.includes(id)) draft.exc = draft.exc.filter((g) => g !== id);
    else draft.inc = [...draft.inc, id];
  }
  const toggleLang = (l: string) => (draft.langs = draft.langs.includes(l) ? draft.langs.filter((x) => x !== l) : [...draft.langs, l]);
  const draftFilter = (d: Draft) => toFilter(d, tv);
  function applyCustom() { custom = draftFilter(draft); moods = []; panel = false; }

  const filter = $derived.by((): NightFilter | null => {
    if (custom) return custom;
    const picked = MOODS.filter((m) => moods.includes(m.id));
    const f: NightFilter = {};
    if (picked.length) {
      f.genres = [...new Set(picked.flatMap((m) => m.f.genres ?? []))];
      f.without = picked.length === 1 ? picked[0].f.without : undefined;
      f.voteMin = Math.max(vote, Math.min(...picked.map((m) => m.f.voteMin ?? 0)));
    } else if (vote) f.voteMin = vote;
    if (maxRt) f.runtimeMax = maxRt;
    const es = ERAS.filter((e) => eras.includes(e.id));
    if (es.length) { f.yearFrom = Math.min(...es.map((e) => e.from)); f.yearTo = Math.max(...es.map((e) => e.to)); }
    return Object.keys(f).length ? f : null;
  });
  const myLabel = $derived(custom ? t("Özel filtre") : MOODS.filter((m) => moods.includes(m.id)).map((m) => t(m.title)).join(" + "));
  const myLine = $derived.by(() => {
    const f = filter;
    if (!f) return "";
    const p: string[] = [];
    if (f.yearFrom || f.yearTo) p.push(`${f.yearFrom ?? "…"} → ${f.yearTo && f.yearTo < YEAR ? f.yearTo : t("bugün")}`);
    if (f.runtimeMax) p.push(`≤ ${dur2(f.runtimeMax)}`);
    if (f.voteMin) p.push(t("puan {v}+", { v: f.voteMin.toFixed(1) }));
    if (f.languages?.length) p.push(f.languages.map((l) => LANGS.find((x) => x[0] === l)?.[1] ?? l).map((x) => t(x)).join(", "));
    return p.join(" · ") || t("Süre ve dönem fark etmez");
  });

  // Afiş yelpazesi ve "yaklaşık n film": `browse` ilk sayfası (tek tür; sayı yaklaşık).
  let previews = $state<Record<string, { total: number; posters: string[] }>>({});
  const bkind = $derived(tv ? "series" : "movie");
  function preview(tag: string, f: NightFilter | null) {
    const url = social.server;
    if (!url || !f) return;
    const q: BrowseQuery = { genre: f.genres?.[0], yearFrom: f.yearFrom, yearTo: f.yearTo, minRating: f.voteMin, maxMin: f.runtimeMax, lang: f.languages?.[0], sort: "popular", page: 1 };
    const key = tag + ":" + bkind;
    browseGet(url, bkind, q).then((r) => {
      if (!r || r.error) return;
      previews[key] = { total: r.total, posters: r.items.map((m) => m.poster).filter((p): p is string => !!p).slice(0, 3) };
    }, () => {});
  }
  const pv = (tag: string) => previews[tag + ":" + bkind];
  $effect(() => { void bkind; for (const m of MOODS) if (!pv(m.id)) preview(m.id, m.f); });
  function debounced(tag: string, f: NightFilter | null, on: boolean) {
    if (!on || !f) return;
    const h = setTimeout(() => preview(tag, f), 550);
    return () => clearTimeout(h);
  }
  $effect(() => debounced("me", $state.snapshot(filter), phase === "setup"));
  $effect(() => debounced("custom", draftFilter($state.snapshot(draft) as Draft), panel));

  // Geçen seferki kurulum (yerel): "Tekrar kur" ile aynı hava geri gelir.
  type Last = { kind: NightKind; moods: string[]; title: string; poster?: string; at: number };
  const LAST = "koni.soloNight.last";
  let last = $state<Last | null>(null);
  try { const v = localStorage.getItem(LAST); if (v) last = JSON.parse(v); } catch { /* yok */ }
  function remember(m: Movie) {
    last = { kind, moods: custom ? [] : $state.snapshot(moods), title: m.title, poster: m.poster, at: Date.now() };
    try { localStorage.setItem(LAST, JSON.stringify(last)); } catch { /* depolama kapalı */ }
  }
  function redo() { if (!last) return; kind = last.kind; moods = last.moods.filter((id) => MOODS.some((m) => m.id === id)); custom = null; }
  const ago = (at: number) => { const d = Math.floor((Date.now() - at) / 864e5); return d <= 0 ? t("bugün") : d === 1 ? t("dün") : t("{n} gün önce", { n: d }); };

  // ---- 2 · Oyla (deste) ----------------------------------------------------------
  /** Gelen bütün kartlar sırayla (şerit); oyum `mine`. */
  let dealt = $state<Movie[]>([]);
  let mine = $state<Record<number, boolean>>({});
  let more = $state(0);
  let busy = $state(false);
  let pending = $state(0);
  let empty = $state("");
  const shown = new Set<string>();
  const key = (m: Movie) => m.imdb || `tmdb:${tv ? "tv" : "movie"}:${m.id}`;
  const card = $derived(dealt.find((m) => !(m.id in mine)) ?? null);
  const after = $derived(dealt.filter((m) => !(m.id in mine) && m.id !== card?.id).length);
  const liked = $derived(dealt.filter((m) => mine[m.id] === true));
  const votedN = $derived(Object.keys(mine).length);
  const passedN = $derived(votedN - liked.length);

  async function deal() {
    const url = social.server;
    if (!url) return onError(t("Ne izlesem? için bir sunucuda hesabın olmalı"));
    busy = true;
    empty = "";
    dealt = []; mine = {}; more = 0; won = null;
    phase = "deck";
    try {
      const r = await soloNight(url, kind, filter, undefined, prefer || undefined);
      if (!r) { empty = t("Ne izlesem? için bir sunucuda hesabın olmalı"); return; }
      if (r.error) { empty = r.error; return; }
      dealt = r.items.filter((m) => !shown.has(key(m)));
      more = r.more;
      if (!dealt.length) empty = t("Bu seçimle {noun} bulunamadı. Havayı ya da süreyi değiştir.", { noun });
    } catch (e) {
      empty = String(e);
    } finally {
      busy = false;
    }
  }
  function cast(like: boolean) {
    const m = card;
    if (!m) return;
    shown.add(key(m));
    mine[m.id] = like;
    const url = social.server;
    if (!url || !more) return;
    pending++;
    soloNightPick(url, key(m)).then(
      (r) => { more = r?.more ?? 0; if (r?.item && !shown.has(key(r.item))) dealt = [...dealt, r.item]; },
      () => {},
    ).finally(() => { pending--; });
  }
  /** "Hiçbiri": beğenileri bırak, yeni deste. */
  function noneOfThem() { deal(); }
  function choose() {
    if (liked.length === 1) openDetail(liked[0]);
    else if (liked.length > 1) { won = null; rot = 0; dur = 0; phase = "wheel"; }
  }

  // ---- 3 · Çark (yerel) --------------------------------------------------------
  let rot = $state(0);
  let dur = $state(0);
  let won = $state<Movie | null>(null);
  let spinning = $state(false);
  function spin() {
    if (spinning || liked.length < 2) return;
    const n = liked.length, a = 360 / n;
    const w = Math.floor(Math.random() * n);
    const jitter = (Math.random() - 0.5) * a * 0.6;
    // Tepedeki işaret dilim w'nin ortasına gelsin: saat yönünde k tur, eksi dilim açısı.
    const target = 360 * 6 - ((w + 0.5) * a + jitter);
    won = null;
    spinning = true;
    dur = 0; rot = 0;
    requestAnimationFrame(() => requestAnimationFrame(() => { dur = 4200; rot = target; }));
    setTimeout(() => { spinning = false; won = liked[w]; }, 4300);
  }
  function respin() { won = null; spin(); }

  // ---- 4 · İzle (künye) -------------------------------------------------------
  let detail = $state<DetailRef | null>(null);
  const refOf = (m: Movie): DetailRef => ({ ref: key(m), kind: tv ? "series" : "movie", title: m.title, poster: m.poster, year: m.year });
  function openDetail(m: Movie) { detail = refOf(m); phase = "detail"; }
  function watch(m: Movie) { remember(m); onWatch(refOf(m), prefer); }

  function back() {
    if (phase === "detail") phase = liked.length > 1 ? "wheel" : "deck";
    else if (phase === "wheel") { if (won) won = null; else phase = "deck"; }
    else if (phase === "deck") phase = "setup";
    else onClose();
  }

  let swipe = $state<SwipeCard | null>(null);
  let trailerKey = $state<string | null>(null);
  function onKey(e: KeyboardEvent) {
    if ((e.target as HTMLElement)?.closest?.("input, textarea")) return;
    if (e.key === "Escape") {
      e.stopPropagation();
      if (trailerKey) trailerKey = null;
      else if (panel) panel = false;
      else onClose();
    } else if (phase === "deck" && card && !trailerKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
      e.preventDefault();
      cast(e.key === "ArrowRight");
    } else if (phase === "deck" && card?.trailer && !trailerKey && (e.key === " " || e.key.toLowerCase() === "m")) {
      e.preventDefault();
      if (e.key === " ") swipe?.toggle(); else swipe?.mute();
    }
  }

  function dur2(min: number) {
    const h = Math.floor(min / 60), m = min % 60;
    return h ? t("{n} sa", { n: h }) + (m ? " " + t("{n} dk", { n: m }) : "") : t("{n} dk", { n: m });
  }
  const rating = (m: Movie) => m.ratings.imdb ?? (m.ratings.tmdb ? m.ratings.tmdb.toFixed(1) : "");
  const votes = (v?: number) => (!v ? "" : v >= 1e6 ? t("{n} M oy", { n: (v / 1e6).toFixed(1).replace(".", i18n.lang === "tr" ? "," : ".") }) : v >= 1000 ? t("{n} B oy", { n: Math.round(v / 1000) }) : t("{n} oy", { n: v }));
  const metaOf = (m: Movie) => [m.year, m.runtime ? (tv ? t("bölüm {n} dk", { n: m.runtime }) : dur2(m.runtime)) : null, m.seasons ? (m.seasons === 1 ? t("1 sezon") : t("{n} sezon", { n: m.seasons })) : null].filter(Boolean) as (string | number)[];
  const grad = (s: string) => `linear-gradient(160deg, hsl(${hue(s)} 45% 32%), hsl(${hue(s)} 40% 8%))`;
  const CAND = ["#ff9f0a", "#0a84ff", "#30d158", "#bf5af2", "#ff375f", "#64d2ff", "#ffd60a", "#ac8e68"];
</script>

<svelte:window onkeydown={onKey} />

{#snippet poster(m: Movie, cls = "")}
  <div class="poster {cls}" style="--p:{grad(m.title)}">
    {#if m.poster}<img src={m.poster} alt="" draggable="false" onerror={(e) => { (e.currentTarget as HTMLElement).style.display = "none"; }} />{:else}<b>{m.title}</b>{/if}
  </div>
{/snippet}

{#snippet ratings(m: Movie)}
  <div class="ratings">
    <div class="rt" class:none={!m.ratings.imdb && !m.ratings.tmdb}>
      <span class="lg" style="background:#f5c518">{m.ratings.imdb ? "IMDb" : "TMDB"}</span>
      <div><b>{rating(m) || "–"}</b><small>{votes(m.votes) || (m.ratings.imdb ? "IMDb" : t("puan"))}</small></div>
    </div>
    <div class="rt" class:none={!m.ratings.rt}><span class="lg" style="background:#fa320a;color:#fff">RT</span><div><b>{m.ratings.rt ?? "–"}</b><small>{t("eleştirmen")}</small></div></div>
    <div class="rt" class:none={!m.ratings.meta}><span class="lg" style="background:#66cc33">MC</span><div><b>{m.ratings.meta ?? "–"}</b><small>Metacritic</small></div></div>
  </div>
{/snippet}

{#snippet meta(m: Movie)}
  <div class="meta">
    {#each metaOf(m) as x, i (i)}{#if i}<i></i>{/if}<span>{x}</span>{/each}
    {#each (m.genres ?? []).slice(0, 3) as g (g)}<span class="genre">{t(g)}</span>{/each}
  </div>
{/snippet}

<div class="scrim" transition:fade={{ duration: 220 }} onclick={onClose} role="presentation"></div>
<section class="sheet" in:fly={{ y: 40, duration: 480, easing: cubicOut }} out:fade={{ duration: 180 }}>
  <div class="sh-head">
    {#if phase !== "setup"}<button class="x" title={t("Geri")} onclick={back}>←</button>{/if}
    <h1><b>✦</b>{t("Ne izlesem?")}</h1>
    <div class="steps">
      {#each STEPS as [k, label], i (k)}
        <span class:on={i === stepIdx} class:done={i < stepIdx}><i>{i < stepIdx ? "✓" : i + 1}</i>{t(label)}</span>
      {/each}
    </div>
    <span class="who">{tv ? t("📺 Dizi · tek başına") : t("🎬 Film · tek başına")}</span>
    <button class="x" title={t("Kapat (Esc)")} onclick={onClose}>✕</button>
  </div>

  {#key phase}
    <div class="body" in:fly={{ x: 40, duration: 420, easing: cubicOut }}>
      {#if phase === "setup"}
        <div class="two-col">
          <div class="main">
            <h2 class="q">{t("Bu akşam ne havadasın?")}<span>{t("Seç, kartları kaydır, gerisini bize bırak.")}</span></h2>
            <div class="row">
              <div class="kind">
                <button class:on={!tv} onclick={() => (kind = "movie")}>🎬 {t("Film")} <small>{t("tek oturuşluk")}</small></button>
                <button class:on={tv} onclick={() => (kind = "tv")}>📺 {t("Dizi")} <small>{t("yeni bir seriye başla")}</small></button>
              </div>
              <span class="soft">{t("Bir ya da iki hava seç, ya da kendi filtreni kur")}</span>
            </div>
            <div class="moods">
              {#each MOODS as m, i (m.id)}
                <button class="mood" class:on={moods.includes(m.id)} style="--m:{m.grad}" onclick={() => toggleMood(m.id)}
                  in:fly={{ y: 18, delay: 60 * i, duration: 420, easing: cubicOut }}>
                  {#if moods.includes(m.id)}<span class="tick" in:scale={{ duration: 250, easing: backOut }}>✓</span>{/if}
                  <div class="fan">
                    {#each pv(m.id)?.posters ?? [] as src (src)}<div class="poster"><img {src} alt="" /></div>{/each}
                  </div>
                  <h3>{t(m.title)}</h3><p>{t(m.sub)}</p>
                </button>
              {/each}
            </div>
            <div class="filters" class:dim={!!custom}>
              <div class="f">
                <div class="lab">{tv ? t("Bölüm süresi") : t("Süre")} <b>{maxRt ? t("en fazla {d}", { d: dur2(maxRt) }) : t("fark etmez")}</b></div>
                <input class="range" type="range" min={tv ? 20 : 80} max={tv ? 70 : 190} step="5" disabled={!!custom}
                  value={maxRt || (tv ? 70 : 190)} oninput={(e) => { const v = Number(e.currentTarget.value); maxRt = v >= (tv ? 70 : 190) ? 0 : v; }}
                  style="--w:{(((maxRt || (tv ? 70 : 190)) - (tv ? 20 : 80)) / (tv ? 50 : 110)) * 100}%" />
                <div class="ticks">{#if tv}<span>{t("{n} dk", { n: 20 })}</span><span>{t("{n} dk", { n: 45 })}</span><span>{t("fark etmez")}</span>{:else}<span>{t("{n} dk", { n: 80 })}</span><span>{t("{n} sa", { n: 2 })}</span><span>{t("fark etmez")}</span>{/if}</div>
              </div>
              <div class="f">
                <div class="lab">{t("Dönem")} <b>{eras.length ? "" : t("hepsi")}</b></div>
                <div class="eras">{#each ERAS as e (e.id)}<button class:on={eras.includes(e.id)} disabled={!!custom} onclick={() => toggleEra(e.id)}>{t(e.label)}</button>{/each}</div>
              </div>
              <div class="f">
                <div class="lab">{t("Puan")} <b>{vote ? `${vote.toFixed(1)}+` : t("fark etmez")}</b></div>
                <input class="range" type="range" min="0" max="8.5" step="0.5" bind:value={vote} disabled={!!custom} style="--w:{(vote / 8.5) * 100}%" />
                <div class="ticks"><span>{t("fark etmez")}</span><span>7</span><span>8.5+</span></div>
              </div>
            </div>
            <button class="custom" class:on={!!custom} onclick={() => (panel = true)}>
              ⚙ {custom ? t("Özel filtren hazır") : t("Özel filtre kur")} <span>{t("Tür dahil/hariç, yıl, süre, dil, oy sayısı")}</span><b>→</b>
            </button>
          </div>
          <aside class="side">
            <h4>{t("Senin gecen")}</h4>
            <div class="sum">
              {#if filter}
                <b>{myLabel ? t("Senin seçimin: {label}", { label: myLabel }) : t("Senin seçimin")}</b>{myLine}
                {#if pv("me")}<br />{t("Bu seçimle yaklaşık")} <b class="in">{pv("me").total.toLocaleString(locale())}</b> {t("{noun} var.", { noun })}{/if}
              {:else}
                <b>{t("Bir hava seç")}</b>{t("Ya da öylece getir: hava seçmezsen popüler olanlar gelir.")}
              {/if}
            </div>
            <div class="note"><b>{t("İzledim dediklerin desteye girmez.")}</b>{t("Profildeki izlediklerin listesi kullanılır.")}</div>
            <div class="grp">
              <h4>{t("Ses ve altyazı")}</h4>
              <div class="chips">
                <button class:on={prefer === ""} onclick={() => (prefer = "")}>{t("fark etmez")}</button>
                <button class:on={prefer === "dub"} onclick={() => (prefer = "dub")}>{t("TR dublaj")}</button>
                <button class:on={prefer === "sub"} onclick={() => (prefer = "sub")}>{t("TR altyazı")}</button>
              </div>
              {#if prefer}<div class="fine left">{t("İzle'ye basınca önce bu dilde kaynak aranır.")}</div>{/if}
            </div>
            {#if last}
              <button class="last" onclick={redo}>
                <span class="lp" style="--p:{grad(last.title)}">{#if last.poster}<img src={last.poster} alt="" />{/if}</span>
                <span class="lt"><b>{t("Geçen sefer: {title}", { title: last.title })}</b><small>{[last.moods.map((id) => t(MOODS.find((m) => m.id === id)?.title ?? id)).join(" + "), ago(last.at)].filter(Boolean).join(" · ")}</small></span>
                <i>{t("Tekrar kur")}</i>
              </button>
            {/if}
            <button class="cta" onclick={deal}>{t("Kartları getir")} <span>→</span></button>
            <div class="fine">{t("Beğendiklerin çarka gider. Tek beğeni varsa doğrudan künyeye geçersin.")}</div>
          </aside>
        </div>

        {#if panel}
          <div class="fpanel" transition:fly={{ x: 440, duration: 380, easing: cubicOut }}>
            <div class="fp-head"><b>{t("Özel filtre")}</b><button class="x" onclick={() => (panel = false)}>✕</button></div>
            <div class="fp-body">
              <div class="fg"><div class="fl">{t("Türler")} <small>{t("bir kez: dahil · iki kez: hariç")}</small></div>
                <div class="tags">{#each GENRES as [id, name] (id)}<button class:in={draft.inc.includes(id)} class:ex={draft.exc.includes(id)} onclick={() => cycleGenre(id)}>{t(name)}</button>{/each}</div></div>
              <div class="fg"><div class="fl">{t("Yıl")} <b>{draft.y0 <= 1960 ? t("eski") : draft.y0} – {draft.y1 >= YEAR ? t("bugün") : draft.y1}</b></div>
                <DualRange min={1960} max={YEAR} bind:lo={draft.y0} bind:hi={draft.y1} />
                <div class="ticks"><span>1960</span><span>1990</span><span>{t("bugün")}</span></div></div>
              <div class="fg"><div class="fl">{tv ? t("Bölüm süresi") : t("Süre")} <b>{draft.r0} – {draft.r1 >= RT.max ? `${RT.max}+` : draft.r1} {t("dk")}</b></div>
                <DualRange min={RT.min} max={RT.max} step={5} bind:lo={draft.r0} bind:hi={draft.r1} /></div>
              <div class="fg two">
                <div><div class="fl">{t("Puan en az")}</div><div class="step"><button onclick={() => (draft.vote = Math.max(0, draft.vote - 0.5))}>−</button><b>{draft.vote ? draft.vote.toFixed(1) : "–"}</b><button onclick={() => (draft.vote = Math.min(9, draft.vote + 0.5))}>+</button></div></div>
                <div><div class="fl">{t("Oy sayısı en az")}</div><div class="step">
                  <button onclick={() => (draft.count = COUNTS[Math.max(0, COUNTS.indexOf(draft.count) - 1)])}>−</button>
                  <b>{draft.count ? (draft.count >= 1000 ? t("{n} B", { n: draft.count / 1000 }) : draft.count) : "–"}</b>
                  <button onclick={() => (draft.count = COUNTS[Math.min(COUNTS.length - 1, COUNTS.indexOf(draft.count) + 1)])}>+</button></div></div>
              </div>
              <div class="fg"><div class="fl">{t("Orijinal dil")}</div>
                <div class="tags">{#each LANGS as [id, name] (id)}<button class:in={draft.langs.includes(id)} onclick={() => toggleLang(id)}>{t(name)}</button>{/each}</div></div>
            </div>
            <div class="fp-foot">
              <span>{#if pv("custom")}<b>{pv("custom").total.toLocaleString(locale())}</b> {t("{noun} uygun", { noun })}{/if}</span>
              <button class="ghost sm" onclick={() => (draft = blankDraft())}>{t("Temizle")}</button>
              <button class="cta sm" onclick={applyCustom}>{t("Uygula")}</button>
            </div>
          </div>
        {/if}

      {:else if phase === "deck"}
        <div class="deck" style="--p:{card ? grad(card.title) : 'none'}">
          {#if card}
            {#key card.id}<div class="amb" in:fade={{ duration: 600 }}>{#if card.poster}<img src={card.poster} alt="" />{/if}</div>{/key}
            <div class="tstack">
              <SwipeCard bind:this={swipe} {card} under={Math.min(after, 2)} {grad} onCast={cast} onExpand={(k) => (trailerKey = k)} />
              <div class="hint">{t("Kartı sağa çek beğen, sola çek geç")} · <kbd>←</kbd> <kbd>→</kbd>{#if card.trailer} · <kbd>{t("Boşluk")}</kbd> {t("durdur")} · <kbd>M</kbd> {t("ses")}{/if}</div>
            </div>
            {#key card.id}
              <div class="info" in:fly={{ x: 30, duration: 420, easing: cubicOut }}>
                <div class="kick"><span>{t("Deste · kart {a} / {b}", { a: votedN + 1, b: dealt.length + more })}</span><span class="likes">♥ {t("{n} beğendin", { n: liked.length })}</span><span>{t("{n} geçtin", { n: passedN })}</span></div>
                <h2 class:long={card.title.length > 22}>{card.title}</h2>
                {@render meta(card)}
                {@render ratings(card)}
                {#if card.overview}<p class="ov">{card.overview}</p>{/if}
                {#if card.cast?.length}<div class="cast">{t("Oyuncular:")} {#each card.cast.slice(0, 3) as c, i (i)}{i ? ", " : ""}<b>{c}</b>{/each}{#if card.director}&nbsp;· {tv ? t("Yaratıcı") : t("Yönetmen")}: <b>{card.director}</b>{/if}</div>{/if}
                <div class="votes">
                  <div class="vlab"><button class="vb pass" onclick={() => cast(false)} title={t("Geç")}>✕</button>{t("Geç")} <kbd>←</kbd></div>
                  <div class="vlab"><button class="vb like" onclick={() => cast(true)} title={t("Beğen")}>♥</button>{t("Beğen")} <kbd>→</kbd></div>
                </div>
                <div class="acts">
                  {#if liked.length}<button class="cta sm" onclick={choose}>{liked.length === 1 ? t("Bunu seç") : t("Çarka git ({n})", { n: liked.length })} →</button>{/if}
                  <button class="ghost sm" onclick={noneOfThem}>{t("Hiçbiri, yeni deste")}</button>
                </div>
              </div>
            {/key}
          {:else if busy || pending}
            <div class="waiting"><div class="spin"></div><h2>{t("Kartlar hazırlanıyor…")}</h2></div>
          {:else}
            <div class="waiting" in:scale={{ start: 0.94, duration: 400 }}>
              <h2>{empty || (liked.length ? t("Deste bitti · {n} beğendin", { n: liked.length }) : t("Bu destede beğendiğin olmadı"))}</h2>
              <div class="acts">
                {#if !empty && liked.length}<button class="cta sm" onclick={choose}>{liked.length === 1 ? t("Bunu seç") : t("Çarka git ({n})", { n: liked.length })} →</button>{/if}
                {#if !empty}<button class={liked.length ? "ghost sm" : "cta sm"} onclick={noneOfThem}>{t("Yeni deste")}</button>{/if}
                <button class="ghost sm" onclick={() => (phase = "setup")}>{t("Seçimi değiştir")}</button>
              </div>
            </div>
          {/if}
          {#if dealt.length}
            <div class="strip">
              <span class="lbl">{t("Deste")}</span>
              {#each dealt as m (m.id)}
                <div class="sp" class:liked={mine[m.id] === true} class:passed={mine[m.id] === false} class:cur={m.id === card?.id} class:next={!(m.id in mine) && m.id !== card?.id}>
                  {@render poster(m)}</div>
              {/each}
            </div>
          {/if}
        </div>

      {:else if phase === "wheel" && won}
        {@const m = won}
        <div class="match">
          <div class="bd">{#if m.poster}<img src={m.poster} alt="" />{/if}</div><div class="bd2"></div>
          <div class="confetti" aria-hidden="true">{#each Array(28) as _, i (i)}<i style="--i:{i}"></i>{/each}</div>
          <div class="mp" in:scale={{ start: 0.6, duration: 800, easing: backOut }}>{@render poster(m)}</div>
          <div class="m-info" in:fly={{ x: 40, delay: 200, duration: 600, easing: cubicOut }}>
            <span class="badge">🎡 {t("ÇARK SEÇTİ · {n} beğeniden", { n: liked.length })}</span>
            <h2 class:long={m.title.length > 20}>{m.title}</h2>
            <div class="meta">
              {#each metaOf(m) as x, i (i)}{#if i}<i></i>{/if}<span>{x}</span>{/each}
              {#each (m.genres ?? []).slice(0, 3) as g (g)}<span class="genre">{t(g)}</span>{/each}
              {#if rating(m)}<span class="genre star">★ {rating(m)}</span>{/if}
            </div>
            <div class="losers"><span>{t("Elenenler:")}</span>{#each liked.filter((x) => x.id !== m.id) as x (x.id)}<button class="pbtn" title={x.title} onclick={() => openDetail(x)}>{@render poster(x)}</button>{/each}</div>
            <div class="row big-row">
              <button class="big w" onclick={() => watch(m)}>▶ {t("İzle")}</button>
              {#if m.trailer}<button class="big g" onclick={() => (trailerKey = m.trailer!)}>{t("Fragman")}</button>{/if}
              <button class="big g" onclick={() => openDetail(m)}>{t("Künye")}</button>
              <button class="big g" onclick={respin}>{t("Tekrar çevir")}</button>
            </div>
            <div class="fine left">{prefer === "dub" ? t("İzle: Film & Dizi açılır, önce TR dublaj kaynağı aranır.") : prefer === "sub" ? t("İzle: Film & Dizi açılır, önce TR altyazılı kaynak aranır.") : t("İzle: Film & Dizi açılır, kaynak kendiliğinden aranır.")}</div>
          </div>
        </div>

      {:else if phase === "wheel"}
        <div class="wheelp">
          <div class="amb soft-amb"></div>
          <div class="wl">
            <WheelDisc items={liked.map((m) => ({ id: m.id, title: m.title, poster: m.poster }))} {rot} {dur} size={460} onSpin={spin} {spinning} {hue} />
          </div>
          <div class="wr">
            <h2 class="q">{t("Kader karar versin")}<span>{t("{n} {noun} beğendin. Çarkı çevir ya da afişe dokunup kendin seç.", { n: liked.length, noun })}</span></h2>
            <div class="cands">
              {#each liked as m, i (m.id)}
                <button class="cand" onclick={() => openDetail(m)} disabled={spinning} title={m.title}>
                  <span class="cp">{@render poster(m)}<i class="dot" style="background:{CAND[i % CAND.length]}"></i></span>
                  <span class="ct"><b>{m.title}</b><small>{[m.year, rating(m)].filter(Boolean).join(" · ")}</small></span>
                </button>
              {/each}
            </div>
            <div class="row">
              <button class="big w" disabled={spinning} onclick={spin}>🎡 {spinning ? t("Dönüyor…") : t("Çarkı çevir")}</button>
              <button class="big g" disabled={spinning} onclick={() => (phase = "deck")}>← {t("Kartlara dön")}</button>
              <button class="big g" disabled={spinning} onclick={noneOfThem}>{t("Hiçbiri, yeni deste")}</button>
            </div>
            <div class="fine left">{t("Tepedeki ok durduğu dilimi seçer; sonra İzle, Fragman ya da Künye.")}</div>
          </div>
        </div>

      {:else if phase === "detail" && detail}
        <div class="detail">
          <MovieDetail title={detail} onWatch={() => { const d = detail!; remember({ id: 0, title: d.title, poster: d.poster, ratings: {} }); onWatch(d, prefer); }} onOpen={(d) => (detail = d)}>
            {#snippet top()}
              <div class="d-top">
                <button class="ghost sm" onclick={back}>← {liked.length > 1 ? t("Çarka dön") : t("Kartlara dön")}</button>
                <button class="ghost sm" onclick={noneOfThem}>{t("Hiçbiri, yeni deste")}</button>
              </div>
            {/snippet}
          </MovieDetail>
        </div>
      {/if}
    </div>
  {/key}

  {#if trailerKey}
    <div class="tr-over" transition:fade={{ duration: 200 }} onclick={() => (trailerKey = null)} role="presentation">
      <div class="tr-box" in:scale={{ start: 0.9, duration: 300 }}>
        <iframe title={t("Fragman")} allow="autoplay; encrypted-media; fullscreen" allowfullscreen referrerpolicy="strict-origin-when-cross-origin"
          src="https://www.youtube-nocookie.com/embed/{trailerKey}?autoplay=1&modestbranding=1&playsinline=1&rel=0"></iframe>
      </div>
    </div>
  {/if}
</section>

<style>
  .scrim { position: fixed; z-index: 40; inset: var(--top-bar) 64px 0 0; background: rgba(0, 0, 0, 0.55); backdrop-filter: blur(10px); }
  .sheet {
    position: fixed; z-index: 41; left: 16px; right: 80px; top: calc(var(--top-bar) + 14px); bottom: 14px; border-radius: 22px; overflow: hidden;
    background: linear-gradient(180deg, rgba(28, 29, 34, 0.97), rgba(16, 17, 20, 0.99)); border: 1px solid var(--line-2);
    box-shadow: 0 40px 120px rgba(0, 0, 0, 0.7), inset 0 1px 0 rgba(255, 255, 255, 0.06);
    display: grid; grid-template-rows: 64px 1fr;
  }
  button { font: inherit; color: inherit; cursor: pointer; }
  button:disabled { cursor: default; }
  kbd { font: 500 10.5px var(--font-mono); padding: 1px 5px; border-radius: 5px; border: 1px solid var(--line-2); color: var(--text-2); }
  .sh-head { display: flex; align-items: center; gap: 14px; padding: 0 18px 0 22px; border-bottom: 1px solid var(--line); position: relative; z-index: 3; min-width: 0; }
  .sh-head h1 { margin: 0; font-size: 20px; font-weight: 800; letter-spacing: -0.02em; display: flex; align-items: center; gap: 10px; white-space: nowrap; }
  .sh-head h1 b { width: 30px; height: 30px; border-radius: 9px; display: grid; place-items: center; background: linear-gradient(135deg, #0a84ff, #5e5ce6); font-size: 15px; }
  .steps { display: flex; gap: 4px; margin-left: 10px; }
  .steps span { display: flex; align-items: center; gap: 7px; padding: 5px 12px 5px 6px; border-radius: 99px; font-size: 12px; font-weight: 600; color: var(--text-3); transition: background 0.3s, color 0.3s; }
  .steps span i { width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; font-style: normal; font-size: 11px; background: var(--glass-2); transition: background 0.3s; }
  .steps span.on { color: #fff; background: var(--glass-2); }
  .steps span.on i { background: var(--accent); }
  .steps span.done i { background: var(--green); color: #000; }
  .who { margin-left: auto; color: var(--text-2); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; padding-left: 8px; }
  .x { flex: none; width: 32px; height: 32px; border-radius: 10px; border: 1px solid var(--line-2); background: var(--glass); display: grid; place-items: center; color: var(--text-2); padding: 0; }
  .x:hover { color: #fff; background: var(--glass-2); }

  .body { min-height: 0; position: relative; display: grid; overflow: hidden; }
  .two-col { display: grid; grid-template-columns: 1fr 300px; min-height: 0; }
  .main { padding: 22px 26px; display: flex; flex-direction: column; gap: 16px; min-width: 0; overflow-y: auto; }
  .side { border-left: 1px solid var(--line); padding: 22px; display: flex; flex-direction: column; gap: 12px; background: rgba(0, 0, 0, 0.18); min-height: 0; overflow-y: auto; }
  .side h4, .grp h4 { margin: 0; font-size: 11.5px; color: var(--text-2); text-transform: uppercase; letter-spacing: 0.06em; }
  .q { font-size: 30px; font-weight: 850; letter-spacing: -0.03em; margin: 0; line-height: 1.15; }
  .q span { color: var(--text-2); font-weight: 600; font-size: 15px; letter-spacing: 0; margin-left: 10px; }
  .row { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .soft { color: var(--text-3); }
  .kind { display: inline-flex; padding: 4px; border-radius: 14px; background: rgba(0, 0, 0, 0.35); border: 1px solid var(--line); }
  .kind button { padding: 7px 14px; border-radius: 10px; border: 0; background: none; font-weight: 650; font-size: 13px; color: var(--text-2); display: flex; align-items: center; gap: 6px; transition: background 0.2s, color 0.2s; }
  .kind button small { font-weight: 500; font-size: 11.5px; opacity: 0.7; }
  .kind button.on { background: #fff; color: #000; }

  /* Poster */
  .poster { position: relative; aspect-ratio: 2 / 3; border-radius: 12px; overflow: hidden; background: var(--p, #1a1b1f); box-shadow: 0 18px 40px rgba(0, 0, 0, 0.55), 0 0 0 1px rgba(255, 255, 255, 0.07); display: flex; flex-direction: column; justify-content: flex-end; padding: 10% 9%; }
  .poster img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .poster::after { content: ""; position: absolute; inset: 0; pointer-events: none; background: linear-gradient(160deg, rgba(255, 255, 255, 0.14), transparent 35%); }
  .poster b { position: relative; z-index: 1; font-weight: 900; letter-spacing: -0.02em; line-height: 1; font-size: 15px; }
  .pbtn { display: block; padding: 0; border: 0; background: none; text-align: left; }

  /* 1 · Kur */
  .moods { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
  .mood { position: relative; height: 128px; border-radius: 16px; overflow: hidden; padding: 14px 16px; display: flex; flex-direction: column; justify-content: flex-end; text-align: left; background: var(--m); border: 1px solid rgba(255, 255, 255, 0.08); transition: transform 0.25s cubic-bezier(0.34, 1.4, 0.5, 1), outline-color 0.2s; outline: 2px solid transparent; outline-offset: 2px; }
  .mood::after { content: ""; position: absolute; inset: 0; background: linear-gradient(90deg, rgba(0, 0, 0, 0.7) 30%, transparent 78%); }
  .mood:hover { transform: translateY(-2px); }
  .mood:hover .fan .poster:nth-child(1) { transform: rotate(-14deg) translateX(-6px); }
  .mood:hover .fan .poster:nth-child(3) { transform: rotate(12deg) translateX(6px); }
  .mood .fan { position: absolute; right: 10px; top: 14px; bottom: -26px; width: 150px; }
  .mood .fan .poster { position: absolute; width: 74px; padding: 6px; border-radius: 7px; transition: transform 0.35s cubic-bezier(0.34, 1.4, 0.5, 1); }
  .mood .fan .poster:nth-child(1) { right: 70px; top: 18px; transform: rotate(-10deg); }
  .mood .fan .poster:nth-child(2) { right: 36px; top: 4px; transform: rotate(-2deg); z-index: 1; }
  .mood .fan .poster:nth-child(3) { right: 0; top: 16px; transform: rotate(8deg); }
  .mood h3 { position: relative; z-index: 2; margin: 0; font-size: 17px; font-weight: 800; letter-spacing: -0.02em; }
  .mood p { position: relative; z-index: 2; margin: 2px 0 0; color: rgba(255, 255, 255, 0.72); font-size: 12px; max-width: 60%; }
  .mood.on { outline-color: #fff; transform: translateY(-2px); }
  .tick { position: absolute; z-index: 3; top: 12px; left: 14px; width: 24px; height: 24px; border-radius: 50%; background: #fff; color: #000; display: grid; place-items: center; font-weight: 900; font-size: 13px; }
  .filters { display: grid; grid-template-columns: 1.3fr 1fr 1fr; gap: 14px; transition: opacity 0.2s; }
  .filters.dim { opacity: 0.4; }
  .f { background: var(--glass); border: 1px solid var(--line); border-radius: 14px; padding: 12px 14px; }
  .lab { display: flex; justify-content: space-between; color: var(--text-2); font-size: 11.5px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em; }
  .lab b { color: #fff; text-transform: none; letter-spacing: 0; font-size: 13px; }
  .range { -webkit-appearance: none; appearance: none; width: 100%; height: 26px; margin: 6px 0 0; background: linear-gradient(90deg, #fff var(--w), rgba(255, 255, 255, 0.14) var(--w)) center / 100% 4px no-repeat; }
  .range::-webkit-slider-thumb { -webkit-appearance: none; width: 18px; height: 18px; border-radius: 50%; background: #fff; box-shadow: 0 2px 8px rgba(0, 0, 0, 0.5); cursor: grab; }
  .range:disabled { opacity: 0.5; }
  .ticks { display: flex; justify-content: space-between; color: var(--text-3); font-size: 10.5px; }
  .eras { display: flex; gap: 4px; margin-top: 10px; }
  .eras button { flex: 1; text-align: center; padding: 7px 0; border-radius: 9px; border: 0; background: rgba(255, 255, 255, 0.05); color: var(--text-2); font-weight: 600; font-size: 12px; transition: background 0.2s, box-shadow 0.2s; }
  .eras button.on { background: rgba(10, 132, 255, 0.22); color: #fff; box-shadow: inset 0 0 0 1px rgba(10, 132, 255, 0.6); }
  .custom { display: flex; align-items: center; gap: 12px; padding: 14px 16px; border-radius: 14px; border: 1px dashed rgba(255, 255, 255, 0.22); background: rgba(255, 255, 255, 0.03); font-weight: 750; font-size: 14px; text-align: left; transition: background 0.2s, border-color 0.2s; }
  .custom:hover { background: rgba(255, 255, 255, 0.06); }
  .custom.on { border-style: solid; border-color: rgba(10, 132, 255, 0.7); background: rgba(10, 132, 255, 0.12); }
  .custom span { font-weight: 500; font-size: 12.5px; color: var(--text-2); }
  .custom b { margin-left: auto; }
  .sum { padding: 14px; border-radius: 14px; background: linear-gradient(135deg, rgba(10, 132, 255, 0.18), rgba(94, 92, 230, 0.12)); border: 1px solid rgba(10, 132, 255, 0.3); font-size: 12.5px; color: rgba(255, 255, 255, 0.85); }
  .sum > b:first-child { display: block; font-size: 15px; color: #fff; margin-bottom: 4px; }
  .sum .in { color: #fff; }
  .note { padding: 10px 12px; border-radius: 12px; background: var(--glass); border: 1px solid var(--line); font-size: 11.5px; color: var(--text-3); display: flex; flex-direction: column; gap: 2px; }
  .note b { color: var(--text); font-size: 12.5px; font-weight: 650; }
  .grp { display: flex; flex-direction: column; gap: 8px; }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .chips button { padding: 6px 11px; border-radius: 99px; border: 1px solid var(--line-2); background: var(--glass); font-size: 12px; font-weight: 600; color: var(--text-2); transition: background 0.2s, color 0.2s; }
  .chips button.on { background: #fff; color: #000; border-color: #fff; }
  .last { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: 12px; background: var(--glass); border: 1px solid var(--line); text-align: left; min-width: 0; }
  .last:hover { background: var(--glass-2); }
  .last .lp { flex: none; position: relative; width: 30px; aspect-ratio: 2 / 3; border-radius: 4px; overflow: hidden; background: var(--p); }
  .last .lp img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .last .lt { display: flex; flex-direction: column; line-height: 1.2; min-width: 0; }
  .last .lt b { font-size: 12.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .last .lt small { color: var(--text-3); font-size: 11px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .last i { margin-left: auto; flex: none; font-style: normal; color: var(--accent); font-weight: 650; font-size: 12px; }
  .side .cta { margin-top: auto; }
  .cta { flex: none; height: 52px; border-radius: 16px; border: 0; background: #fff; color: #000; font-weight: 800; font-size: 15px; display: flex; align-items: center; justify-content: center; gap: 10px; box-shadow: 0 12px 30px -8px rgba(10, 132, 255, 0.6); transition: transform 0.15s; }
  .cta:hover:not(:disabled) { transform: translateY(-1px); }
  .cta:active:not(:disabled) { transform: scale(0.98); }
  .cta:disabled { opacity: 0.45; }
  .cta.sm { height: 38px; padding: 0 18px; border-radius: 12px; font-size: 13px; box-shadow: none; }
  .fine { color: var(--text-3); font-size: 11.5px; text-align: center; }
  .fine.left { text-align: left; }
  .ghost { height: 38px; padding: 0 16px; border-radius: 12px; background: var(--glass-2); border: 1px solid var(--line-2); display: inline-flex; align-items: center; gap: 8px; font-weight: 600; }
  .ghost:hover { background: var(--glass-3); }
  .ghost.sm { height: 32px; padding: 0 12px; font-size: 12px; }
  .acts { display: flex; gap: 10px; flex-wrap: wrap; align-items: center; margin-top: 4px; }

  /* Özel filtre */
  .fpanel { position: absolute; top: 0; right: 0; bottom: 0; width: 430px; z-index: 5; display: flex; flex-direction: column; background: #17181c; border-left: 1px solid var(--line-2); box-shadow: -30px 0 80px rgba(0, 0, 0, 0.6); }
  .fp-head { display: flex; align-items: center; justify-content: space-between; padding: 14px 20px; border-bottom: 1px solid var(--line); font-size: 16px; }
  .fp-body { flex: 1; overflow-y: auto; padding: 16px 20px; display: flex; flex-direction: column; gap: 16px; }
  .fl { display: flex; justify-content: space-between; align-items: baseline; font-size: 11.5px; font-weight: 700; color: var(--text-2); text-transform: uppercase; letter-spacing: 0.06em; margin-bottom: 8px; }
  .fl small, .fl b { text-transform: none; letter-spacing: 0; font-weight: 500; }
  .fl b { color: #fff; font-weight: 700; font-size: 13px; }
  .tags { display: flex; flex-wrap: wrap; gap: 6px; }
  .tags button { padding: 6px 11px; border-radius: 9px; background: rgba(255, 255, 255, 0.06); border: 1px solid var(--line); font-size: 12.5px; font-weight: 550; color: var(--text-2); transition: background 0.15s, border-color 0.15s; }
  .tags button.in { background: rgba(48, 209, 88, 0.16); border-color: rgba(48, 209, 88, 0.5); color: #fff; }
  .tags button.in::before { content: "✓ "; color: var(--green); }
  .tags button.ex { background: rgba(255, 69, 58, 0.12); border-color: rgba(255, 69, 58, 0.45); color: #ff8a80; text-decoration: line-through; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .step { display: flex; align-items: center; justify-content: space-between; padding: 6px; border-radius: 12px; background: rgba(255, 255, 255, 0.05); border: 1px solid var(--line); }
  .step button { width: 30px; height: 30px; border-radius: 9px; border: 0; display: grid; place-items: center; background: rgba(255, 255, 255, 0.08); font-weight: 700; padding: 0; }
  .step b { font-size: 16px; }
  .fp-foot { display: flex; align-items: center; gap: 10px; padding: 14px 20px; border-top: 1px solid var(--line); }
  .fp-foot > span { margin-right: auto; color: var(--text-2); }
  .fp-foot > span b { color: #fff; font-size: 16px; }

  /* 2 · Oyla */
  .deck { position: relative; display: grid; grid-template-columns: minmax(0, 1.45fr) minmax(320px, 1fr); gap: 34px; padding: 22px 34px 100px; min-height: 0; overflow: hidden; }
  .tstack { position: relative; min-width: 0; }
  .tstack .hint { margin-top: 22px; text-align: center; color: var(--text-3); font-size: 12px; }
  .amb { position: absolute; inset: 0; pointer-events: none; background: var(--p); }
  .amb img { position: absolute; inset: -10%; width: 120%; height: 120%; object-fit: cover; filter: blur(80px) saturate(1.5); opacity: 0.4; }
  .amb::after { content: ""; position: absolute; inset: 0; background: linear-gradient(90deg, rgba(16, 17, 20, 0.2), rgba(16, 17, 20, 0.9) 70%); }
  .info { position: relative; display: flex; flex-direction: column; gap: 12px; padding-top: 6px; min-width: 0; }
  .kick { color: var(--text-2); font-size: 12px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.08em; display: flex; gap: 14px; align-items: center; flex-wrap: wrap; }
  .kick .likes { color: #ff375f; }
  .info h2 { margin: 0; font-size: 50px; font-weight: 900; letter-spacing: -0.035em; line-height: 1.06; padding-bottom: 0.06em; }
  .info h2.long, .m-info h2.long { font-size: 36px; }
  .meta { display: flex; gap: 10px; align-items: center; color: rgba(255, 255, 255, 0.8); font-size: 14px; flex-wrap: wrap; }
  .meta i { width: 4px; height: 4px; border-radius: 50%; background: var(--text-3); }
  .genre { padding: 3px 10px; border-radius: 99px; border: 1px solid var(--line-2); font-size: 12px; color: var(--text-2); }
  .genre.star { color: #f5c518; }
  .ratings { display: flex; gap: 10px; flex-wrap: wrap; }
  .rt { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-radius: 12px; background: var(--glass); border: 1px solid var(--line); }
  .rt.none { opacity: 0.4; }
  .rt > div { display: flex; flex-direction: column; line-height: 1.1; }
  .rt b { font-size: 18px; font-weight: 800; }
  .rt small { color: var(--text-2); font-size: 11px; }
  .rt .lg { width: 30px; height: 26px; border-radius: 6px; display: grid; place-items: center; font-weight: 900; font-size: 9.5px; color: #000; }
  .ov { color: rgba(255, 255, 255, 0.78); font-size: 14px; line-height: 1.55; max-width: 620px; margin: 0; display: -webkit-box; -webkit-line-clamp: 4; line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden; }
  .cast { color: var(--text-2); font-size: 12.5px; }
  .cast b { color: #fff; font-weight: 600; }
  .votes { display: flex; align-items: center; gap: 22px; margin-top: 8px; }
  .vb { width: 72px; height: 72px; border-radius: 50%; border: 0; display: grid; place-items: center; font-size: 26px; font-weight: 900; padding: 0; transition: transform 0.18s cubic-bezier(0.34, 1.6, 0.5, 1); }
  .vb:hover { transform: scale(1.08); }
  .vb:active { transform: scale(0.92); }
  .vb.pass { background: rgba(255, 255, 255, 0.08); color: #ff6961; box-shadow: inset 0 0 0 2px rgba(255, 105, 97, 0.4); }
  .vb.like { background: linear-gradient(135deg, #ff375f, #ff6b3d); color: #fff; box-shadow: 0 14px 34px -8px rgba(255, 55, 95, 0.7); width: 84px; height: 84px; font-size: 30px; }
  .vlab { display: flex; flex-direction: column; align-items: center; gap: 6px; color: var(--text-2); font-size: 11.5px; }
  .strip { position: absolute; left: 0; right: 0; bottom: 0; height: 92px; display: flex; align-items: center; gap: 8px; padding: 0 40px; background: linear-gradient(0deg, rgba(0, 0, 0, 0.5), transparent); overflow-x: auto; }
  .strip .sp { width: 44px; flex: none; transition: transform 0.3s, filter 0.3s, opacity 0.3s; }
  .strip .poster { padding: 4px; border-radius: 6px; }
  .strip .poster b { font-size: 5px; }
  .strip .sp.liked .poster { box-shadow: 0 0 0 2px #ff375f; }
  .strip .sp.passed { filter: grayscale(1) brightness(0.45); }
  .strip .sp.cur { transform: translateY(-6px); }
  .strip .sp.cur .poster { box-shadow: 0 0 0 2px #fff; }
  .strip .sp.next { opacity: 0.5; filter: blur(3px) brightness(0.6); }
  .strip .lbl { color: var(--text-2); font-size: 11.5px; margin-right: 10px; white-space: nowrap; }
  .waiting { grid-column: 1 / -1; align-self: center; justify-self: center; display: flex; flex-direction: column; align-items: center; gap: 12px; text-align: center; position: relative; }
  .waiting h2 { margin: 6px 0 0; font-size: 26px; font-weight: 850; max-width: 560px; }
  .spin { width: 46px; height: 46px; border-radius: 50%; border: 3px solid rgba(255, 255, 255, 0.12); border-top-color: var(--accent); animation: rot 0.9s linear infinite; }
  @keyframes rot { to { transform: rotate(360deg); } }

  /* 3 · Çark */
  .wheelp { position: relative; display: grid; grid-template-columns: 500px minmax(0, 1fr); gap: 36px; align-items: center; padding: 10px 50px 10px 36px; overflow: hidden; min-height: 0; }
  .soft-amb { background: radial-gradient(45% 70% at 25% 50%, rgba(255, 160, 60, 0.16), transparent 70%); }
  .wl { position: relative; display: grid; place-items: center; }
  .wr { position: relative; display: flex; flex-direction: column; gap: 16px; min-width: 0; }
  .wr .q span { display: block; margin: 6px 0 0; font-size: 14px; }
  .cands { display: grid; grid-template-columns: repeat(auto-fill, minmax(104px, 1fr)); gap: 12px; max-width: 560px; }
  .cand { display: flex; flex-direction: column; gap: 6px; padding: 0; border: 0; background: none; text-align: left; min-width: 0; }
  .cand:disabled { opacity: 0.6; }
  .cand .cp { position: relative; display: block; }
  .cand .poster { padding: 9%; border-radius: 10px; transition: transform 0.25s cubic-bezier(0.34, 1.4, 0.5, 1); }
  .cand:hover:not(:disabled) .poster { transform: translateY(-4px); }
  .cand .dot { position: absolute; top: 8px; left: 8px; width: 14px; height: 14px; border-radius: 50%; box-shadow: 0 0 0 2px rgba(0, 0, 0, 0.5); z-index: 2; }
  .cand .ct { display: flex; flex-direction: column; line-height: 1.25; min-width: 0; }
  .cand .ct b { font-weight: 650; font-size: 12.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cand .ct small { color: var(--text-3); font-size: 11px; }

  /* Çark seçti (Eşleşme dili) */
  .match { position: relative; display: grid; grid-template-columns: 330px 1fr; gap: 48px; align-items: center; padding: 0 70px; overflow: hidden; }
  .match .bd { position: absolute; inset: -40px; overflow: hidden; }
  .match .bd img { width: 100%; height: 100%; object-fit: cover; filter: blur(60px) saturate(1.4); opacity: 0.55; }
  .match .bd2 { position: absolute; inset: 0; background: linear-gradient(90deg, rgba(10, 10, 12, 0.2), rgba(10, 10, 12, 0.85) 60%); }
  .mp { position: relative; width: 330px; transform: rotate(-3deg); }
  .mp .poster { box-shadow: 0 40px 90px rgba(0, 0, 0, 0.7), 0 0 0 1px rgba(255, 255, 255, 0.1), 0 0 90px -10px rgba(255, 180, 60, 0.7); }
  .mp .poster b { font-size: 40px; }
  .m-info { position: relative; display: flex; flex-direction: column; gap: 14px; min-width: 0; }
  .badge { align-self: flex-start; padding: 6px 14px; border-radius: 99px; background: linear-gradient(135deg, #ffd60a, #ff9f0a); color: #2a1a00; font-weight: 800; font-size: 12.5px; letter-spacing: 0.02em; }
  .m-info h2 { margin: 0; font-size: 60px; font-weight: 900; letter-spacing: -0.04em; line-height: 1.04; padding-bottom: 0.06em; }
  .losers { display: flex; align-items: center; gap: 8px; color: rgba(255, 255, 255, 0.8); font-size: 13.5px; }
  .losers > span { margin-right: 4px; }
  .losers .pbtn { width: 34px; }
  .losers .poster { padding: 3px; border-radius: 5px; box-shadow: none; transition: transform 0.2s; }
  .losers .pbtn:hover .poster { transform: translateY(-3px); }
  .losers .poster b { font-size: 6px; }
  .big-row { margin-top: 8px; }
  .big { height: 54px; padding: 0 24px; white-space: nowrap; border-radius: 16px; border: 0; font-weight: 800; font-size: 15px; display: flex; align-items: center; gap: 10px; transition: transform 0.15s; }
  .big:hover:not(:disabled) { transform: translateY(-1px); }
  .big:disabled { opacity: 0.5; }
  .big.w { background: #fff; color: #000; box-shadow: 0 14px 34px -10px rgba(255, 255, 255, 0.4); }
  .big.g { background: var(--glass-2); border: 1px solid var(--line-2); }
  .confetti { position: absolute; inset: 0; pointer-events: none; overflow: hidden; z-index: 3; }
  .confetti i {
    position: absolute; top: -20px; left: calc(var(--i) * 3.6%); width: 8px; height: 14px; border-radius: 2px;
    background: hsl(calc(var(--i) * 47) 90% 60%); animation: fall 2.6s cubic-bezier(0.3, 0.6, 0.5, 1) calc(var(--i) * 45ms) 1 forwards;
  }
  @keyframes fall { to { transform: translateY(760px) rotate(720deg); opacity: 0; } }

  .detail { padding: 24px 30px; overflow-y: auto; min-height: 0; }
  .d-top { display: flex; gap: 8px; margin-bottom: 12px; }

  .tr-over { position: absolute; inset: 64px 0 0; z-index: 8; background: rgba(0, 0, 0, 0.75); display: grid; place-items: center; backdrop-filter: blur(6px); }
  .tr-box { width: min(86%, 1000px); aspect-ratio: 16 / 9; border-radius: 16px; overflow: hidden; box-shadow: 0 40px 120px rgba(0, 0, 0, 0.8); }
  .tr-box iframe { width: 100%; height: 100%; border: 0; }
</style>
