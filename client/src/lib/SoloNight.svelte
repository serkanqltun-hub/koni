<script lang="ts">
  // "Ne izlesem?" (PROTOCOL §3.0k): tek kişilik karar sihirbazı, solo lobiden açılır.
  // 1 Kur (film/dizi, hava, süre, dublaj/altyazı) → 2 sunucudan deste, sağa/sola kaydır
  // (her kart bitince `solo_night_pick` sıradakini getirir) → 3 beğenilen 1 ise künye,
  // 2+ ise çark → 4 künye → ▶ İzle (Film & Dizi açılır, kaynak kendiliğinden aranır).
  // Açıkken video penceresi gizlidir (App).
  import { fade, fly, scale } from "svelte/transition";
  import { cubicOut, backOut } from "svelte/easing";
  import { soloNight, soloNightPick, type DetailRef, type Movie, type NightFilter, type NightKind } from "./player";
  import { social } from "./social.svelte";
  import { MOODS } from "./night/moods";
  import SwipeCard from "./night/SwipeCard.svelte";
  import WheelDisc from "./night/WheelDisc.svelte";
  import MovieDetail from "./MovieDetail.svelte";
  import type { Prefer } from "./sources";
  import { t, tx } from "./i18n.svelte";

  let { hue, onClose, onError, onWatch }: {
    hue: (name: string) => number;
    onClose: () => void;
    onError: (msg: string) => void;
    /** Seçilen başlığı izle: Film & Dizi'de açılır; `prefer` kaynak avında TR dublaj/altyazı önce. */
    onWatch: (d: DetailRef, prefer: Prefer) => void;
  } = $props();

  type Phase = "setup" | "deck" | "wheel" | "detail";
  let phase = $state<Phase>("setup");
  const STEPS: [Phase, string][] = [["setup", "Kur"], ["deck", "Kaydır"], ["wheel", "Çark"], ["detail", "İzle"]];
  const stepIdx = $derived(STEPS.findIndex((s) => s[0] === phase));

  // ---- 1 · Kur ---------------------------------------------------------------
  let kind = $state<NightKind>("movie");
  let moods = $state<string[]>([]);
  let budget = $state(0); // dakika; 0 = fark etmez
  let prefer = $state<Prefer>("");
  const tv = $derived(kind === "tv");
  const BUDGETS = $derived(tv ? [0, 25, 45, 60] : [0, 90, 120, 150]);
  $effect(() => { if (!BUDGETS.includes(budget)) budget = 0; });
  function toggleMood(id: string) {
    moods = moods.includes(id) ? moods.filter((m) => m !== id) : [...moods.slice(-1), id]; // en fazla 2
  }
  const filter = $derived.by((): NightFilter | null => {
    const picked = MOODS.filter((m) => moods.includes(m.id));
    if (!picked.length) return null;
    return {
      genres: [...new Set(picked.flatMap((m) => m.f.genres ?? []))],
      without: picked.length === 1 ? picked[0].f.without : undefined,
      voteMin: Math.min(...picked.map((m) => m.f.voteMin ?? 0)),
    };
  });

  // ---- 2 · Deste ----------------------------------------------------------------
  let queue = $state<Movie[]>([]);
  let liked = $state<Movie[]>([]);
  /** Gösterilen kartlar (yeni destede tekrar gelmesin). */
  const shown = new Set<string>();
  let passed = $state(0);
  let busy = $state(false);
  /** Sıradaki kart isteği yolda. */
  let pending = $state(0);
  let empty = $state("");
  const key = (m: Movie) => m.imdb || `tmdb:${tv ? "tv" : "movie"}:${m.id}`;
  const card = $derived(queue[0] ?? null);

  async function deal() {
    const url = social.server;
    if (!url) return onError(t("Ne izlesem? için bir sunucuda hesabın olmalı"));
    busy = true;
    empty = "";
    queue = [];
    phase = "deck";
    try {
      const r = await soloNight(url, kind, filter, budget || undefined, prefer || undefined);
      if (!r) { empty = t("Ne izlesem? için bir sunucuda hesabın olmalı"); return; }
      if (r.error) { empty = tx(r.error); return; }
      queue = r.items.filter((m) => !shown.has(key(m)));
      if (!queue.length) empty = t("Bu seçimle {noun} bulunamadı. Havayı ya da süreyi değiştir.", { noun: tv ? t("dizi") : t("film") });
    } catch (e) {
      empty = tx(String(e));
    } finally {
      busy = false;
    }
  }
  function cast(like: boolean) {
    const m = card;
    if (!m) return;
    shown.add(key(m));
    queue = queue.slice(1);
    if (like) liked = [...liked, m];
    else passed++;
    const url = social.server;
    if (!url) return;
    pending++;
    soloNightPick(url, key(m)).then(
      (r) => { if (r?.item && !shown.has(key(r.item))) queue = [...queue, r.item]; },
      () => {},
    ).finally(() => { pending--; });
  }
  /** "Hiçbiri": beğenileri bırak, yeni deste. */
  function noneOfThem() {
    liked = [];
    passed = 0;
    wheelWinner = null;
    deal();
  }
  function choose() {
    if (liked.length === 1) openDetail(liked[0]);
    else if (liked.length > 1) { wheelWinner = null; rot = 0; dur = 0; phase = "wheel"; }
  }

  // ---- 3 · Çark (yerel) --------------------------------------------------------
  let rot = $state(0);
  let dur = $state(0);
  let wheelWinner = $state<Movie | null>(null);
  let spinning = $state(false);
  function spin() {
    if (spinning || liked.length < 2) return;
    const n = liked.length, a = 360 / n;
    const w = Math.floor(Math.random() * n);
    const jitter = (Math.random() - 0.5) * a * 0.6;
    // Tepedeki işaret dilim w'nin ortasına gelsin: saat yönünde k tur, eksi dilim açısı.
    const target = 360 * 6 - ((w + 0.5) * a + jitter);
    wheelWinner = null;
    spinning = true;
    dur = 0; rot = 0;
    requestAnimationFrame(() => requestAnimationFrame(() => { dur = 4200; rot = target; }));
    setTimeout(() => { spinning = false; wheelWinner = liked[w]; }, 4300);
  }

  // ---- 4 · Künye -------------------------------------------------------------
  let detail = $state<DetailRef | null>(null);
  function openDetail(m: Movie) {
    detail = { ref: key(m), kind: tv ? "series" : "movie", title: m.title, poster: m.poster, year: m.year };
    phase = "detail";
  }

  function back() {
    if (phase === "detail") phase = liked.length > 1 ? "wheel" : "deck";
    else if (phase === "wheel") phase = "deck";
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
  const metaOf = (m: Movie) => [m.year, m.runtime ? (tv ? t("bölüm {n} dk", { n: m.runtime }) : dur2(m.runtime)) : null, m.seasons ? (m.seasons === 1 ? t("1 sezon") : t("{n} sezon", { n: m.seasons })) : null].filter(Boolean) as (string | number)[];
  const grad = (s: string) => `linear-gradient(160deg, hsl(${hue(s)} 45% 32%), hsl(${hue(s)} 40% 8%))`;
</script>

<svelte:window onkeydown={onKey} />

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
    <button class="x end" title={t("Kapat (Esc)")} onclick={onClose}>✕</button>
  </div>

  {#key phase}
    <div class="body" in:fly={{ x: 40, duration: 420, easing: cubicOut }}>
      {#if phase === "setup"}
        <div class="setup">
          <h2 class="q">{t("Bu akşam ne havadasın?")}<span>{t("Seç, kartları kaydır, gerisini bize bırak.")}</span></h2>
          <div class="kinds">
            <button class="k" class:on={kind === "movie"} onclick={() => (kind = "movie")}><span>🎬</span><b>{t("Film")}</b><small>{t("tek oturuşluk")}</small></button>
            <button class="k" class:on={kind === "tv"} onclick={() => (kind = "tv")}><span>📺</span><b>{t("Dizi")}</b><small>{t("yeni bir seriye başla")}</small></button>
          </div>
          <div class="lab">{t("Ruh hali")} <b>{moods.length ? "" : t("fark etmez")}</b></div>
          <div class="moods">
            {#each MOODS as m, i (m.id)}
              <button class="mood" class:on={moods.includes(m.id)} style="--m:{m.grad}" onclick={() => toggleMood(m.id)}
                in:fly={{ y: 18, delay: 50 * i, duration: 400, easing: cubicOut }}>
                {#if moods.includes(m.id)}<span class="tick" in:scale={{ duration: 250, easing: backOut }}>✓</span>{/if}
                <h3>{t(m.title)}</h3><p>{t(m.sub)}</p>
              </button>
            {/each}
          </div>
          <div class="row2">
            <div>
              <div class="lab">{tv ? t("Bölüm başına kaç dakikan var?") : t("Kaç dakikan var?")}</div>
              <div class="chips">
                {#each BUDGETS as b (b)}<button class:on={budget === b} onclick={() => (budget = b)}>{b ? t("en fazla {d}", { d: dur2(b) }) : t("fark etmez")}</button>{/each}
              </div>
            </div>
            <div>
              <div class="lab">{t("Ses ve altyazı")}</div>
              <div class="chips">
                <button class:on={prefer === ""} onclick={() => (prefer = "")}>{t("fark etmez")}</button>
                <button class:on={prefer === "dub"} onclick={() => (prefer = "dub")}>{t("TR dublaj")}</button>
                <button class:on={prefer === "sub"} onclick={() => (prefer = "sub")}>{t("TR altyazı")}</button>
              </div>
            </div>
          </div>
          <div class="foot">
            <span class="fine">{prefer ? t("İzle'ye basınca önce bu dilde kaynak aranır.") : ""}</span>
            <button class="cta" onclick={() => { liked = []; passed = 0; deal(); }}>{t("Kartları getir")} <span>→</span></button>
          </div>
        </div>

      {:else if phase === "deck"}
        {#if card}
          <div class="deck">
            {#key card.id}<div class="amb" style="--p:{grad(card.title)}" in:fade={{ duration: 600 }}>{#if card.poster}<img src={card.poster} alt="" />{/if}</div>{/key}
            <div class="tstack">
              <SwipeCard bind:this={swipe} {card} under={queue.length - 1} {grad} onCast={cast} onExpand={(k) => (trailerKey = k)} />
              <div class="hint">{t("Kartı sağa çek beğen, sola çek geç")} · <kbd>←</kbd> <kbd>→</kbd>{#if card.trailer} · <kbd>{t("Boşluk")}</kbd> {t("durdur")} · <kbd>M</kbd> {t("ses")}{/if}</div>
            </div>
            {#key card.id}
              <div class="info" in:fly={{ x: 30, duration: 420, easing: cubicOut }}>
                <div class="kick">{t("♥ {a} beğendin · {b} geçtin", { a: liked.length, b: passed })}</div>
                <h2 class:long={card.title.length > 22}>{card.title}</h2>
                <div class="meta">
                  {#each metaOf(card) as x, i (i)}{#if i}<i></i>{/if}<span>{x}</span>{/each}
                  {#each (card.genres ?? []).slice(0, 3) as g (g)}<span class="genre">{t(g)}</span>{/each}
                  {#if rating(card)}<span class="rt">★ {rating(card)}</span>{/if}
                </div>
                {#if card.overview}<p class="ov">{card.overview}</p>{/if}
                {#if card.cast?.length}<div class="cast">{t("Oyuncular:")} {card.cast.slice(0, 3).join(", ")}</div>{/if}
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
          </div>
        {:else if busy || pending}
          <div class="waiting"><div class="spin"></div><h2>{t("Kartlar hazırlanıyor…")}</h2></div>
        {:else}
          <div class="waiting">
            <h2>{empty || (liked.length ? t("Deste bitti · {n} beğendin", { n: liked.length }) : t("Bu destede beğendiğin olmadı"))}</h2>
            <div class="acts">
              {#if !empty && liked.length}<button class="cta sm" onclick={choose}>{liked.length === 1 ? t("Bunu seç") : t("Çarka git ({n})", { n: liked.length })} →</button>{/if}
              {#if !empty}<button class={liked.length ? "ghost sm" : "cta sm"} onclick={noneOfThem}>{t("Yeni deste")}</button>{/if}
              <button class="ghost sm" onclick={() => (phase = "setup")}>{t("Seçimi değiştir")}</button>
            </div>
          </div>
        {/if}

      {:else if phase === "wheel"}
        <div class="wheel">
          <div class="wl">
            <WheelDisc items={liked.map((m) => ({ id: m.id, title: m.title }))} {rot} {dur} size={340} />
          </div>
          <div class="wr">
            <h2 class="q">{t("Kader karar versin")}<span>{t("{n} başlık beğendin. Çarkı çevir ya da birine dokun.", { n: liked.length })}</span></h2>
            <div class="liked">
              {#each liked as m (m.id)}
                <button class="li" class:win={wheelWinner?.id === m.id} onclick={() => openDetail(m)} disabled={spinning}>
                  <span class="lp" style="--p:{grad(m.title)}">{#if m.poster}<img src={m.poster} alt="" />{/if}</span>
                  <span class="lt"><b>{m.title}</b><small>{metaOf(m).join(" · ")}</small></span>
                </button>
              {/each}
            </div>
            {#if wheelWinner}
              <div class="won" in:scale={{ start: 0.8, duration: 450, easing: backOut }}>🎡 {t("Kazanan")}: <b>{wheelWinner.title}</b></div>
              <div class="acts">
                <button class="cta" onclick={() => openDetail(wheelWinner!)}>{t("Künyeye geç")} →</button>
                <button class="ghost sm" onclick={spin}>{t("Tekrar çevir")}</button>
              </div>
            {:else}
              <div class="acts"><button class="cta" disabled={spinning} onclick={spin}>{spinning ? t("Dönüyor…") : t("Çarkı çevir")}</button></div>
            {/if}
          </div>
        </div>

      {:else if phase === "detail" && detail}
        <div class="detail">
          <MovieDetail title={detail} onWatch={() => onWatch(detail!, prefer)} onOpen={(d) => (detail = d)}>
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
  button:disabled { cursor: default; opacity: 0.5; }
  kbd { font: 500 10.5px var(--font-mono); padding: 1px 5px; border-radius: 5px; border: 1px solid var(--line-2); color: var(--text-2); }
  .sh-head { display: flex; align-items: center; gap: 14px; padding: 0 18px 0 22px; border-bottom: 1px solid var(--line); min-width: 0; }
  .sh-head h1 { margin: 0; font-size: 20px; font-weight: 800; letter-spacing: -0.02em; display: flex; align-items: center; gap: 10px; white-space: nowrap; }
  .sh-head h1 b { width: 30px; height: 30px; border-radius: 9px; display: grid; place-items: center; background: linear-gradient(135deg, #0a84ff, #5e5ce6); font-size: 15px; }
  .steps { display: flex; gap: 4px; margin-left: 10px; }
  .steps span { display: flex; align-items: center; gap: 7px; padding: 5px 12px 5px 6px; border-radius: 99px; font-size: 12px; font-weight: 600; color: var(--text-3); }
  .steps span i { width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; font-style: normal; font-size: 11px; background: var(--glass-2); }
  .steps span.on { color: #fff; background: var(--glass-2); }
  .steps span.on i { background: var(--accent); }
  .steps span.done i { background: var(--green); color: #000; }
  .x { flex: none; width: 32px; height: 32px; border-radius: 10px; border: 1px solid var(--line-2); background: var(--glass); display: grid; place-items: center; color: var(--text-2); padding: 0; }
  .x:hover { color: #fff; background: var(--glass-2); }
  .x.end { margin-left: auto; }

  .body { min-height: 0; position: relative; display: grid; overflow: hidden; }
  .q { margin: 0; font-size: 24px; font-weight: 800; letter-spacing: -0.02em; display: flex; flex-direction: column; gap: 4px; }
  .q span { font-size: 13px; font-weight: 500; color: var(--text-2); letter-spacing: 0; }
  .lab { font-size: 12px; font-weight: 700; color: var(--text-2); text-transform: uppercase; letter-spacing: 0.06em; }
  .lab b { color: var(--text-3); font-weight: 600; text-transform: none; letter-spacing: 0; margin-left: 6px; }
  .cta { display: inline-flex; align-items: center; gap: 8px; padding: 12px 22px; border-radius: 14px; border: 0; background: var(--accent); color: #fff; font-weight: 750; font-size: 15px; }
  .cta.sm { padding: 9px 16px; font-size: 13px; border-radius: 11px; }
  .ghost { padding: 9px 14px; border-radius: 11px; border: 1px solid var(--line-2); background: var(--glass); color: var(--text-2); font-weight: 650; font-size: 13px; }
  .ghost:hover { color: #fff; background: var(--glass-2); }
  .acts { display: flex; gap: 10px; flex-wrap: wrap; align-items: center; }
  .fine { color: var(--text-3); font-size: 12px; }

  .setup { padding: 24px 30px; display: flex; flex-direction: column; gap: 16px; overflow-y: auto; min-height: 0; }
  .kinds { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; max-width: 520px; }
  .k { display: grid; grid-template-columns: auto 1fr; grid-template-rows: auto auto; column-gap: 12px; align-items: center; text-align: left; padding: 14px 16px; border-radius: 16px; border: 1px solid var(--line-2); background: var(--glass); }
  .k span { grid-row: span 2; font-size: 26px; }
  .k b { font-size: 16px; }
  .k small { color: var(--text-3); font-size: 12px; }
  .k.on { border-color: var(--accent); background: rgba(10, 132, 255, 0.16); }
  .moods { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
  .mood { position: relative; text-align: left; padding: 16px 18px; min-height: 92px; border-radius: 16px; border: 1px solid rgba(255, 255, 255, 0.08); background: var(--m); transition: transform 0.2s, box-shadow 0.2s; }
  .mood:hover { transform: translateY(-2px); }
  .mood.on { box-shadow: 0 0 0 2px #fff; }
  .mood h3 { margin: 0 0 4px; font-size: 16px; font-weight: 800; }
  .mood p { margin: 0; font-size: 12.5px; color: rgba(255, 255, 255, 0.75); }
  .tick { position: absolute; right: 12px; top: 12px; width: 22px; height: 22px; border-radius: 50%; background: #fff; color: #000; display: grid; place-items: center; font-size: 12px; font-weight: 900; }
  .row2 { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; }
  .row2 > div { display: flex; flex-direction: column; gap: 8px; }
  .chips { display: flex; gap: 8px; flex-wrap: wrap; }
  .chips button { padding: 8px 14px; border-radius: 99px; border: 1px solid var(--line-2); background: var(--glass); font-size: 13px; font-weight: 600; color: var(--text-2); }
  .chips button.on { background: #fff; color: #000; border-color: #fff; }
  .foot { display: flex; align-items: center; justify-content: space-between; gap: 14px; margin-top: auto; padding-top: 8px; }

  .deck { position: relative; display: grid; grid-template-columns: minmax(0, 1.45fr) minmax(300px, 1fr); gap: 34px; padding: 22px 34px 30px; min-height: 0; overflow: hidden; }
  .amb { position: absolute; inset: 0; pointer-events: none; background: var(--p); }
  .amb img { position: absolute; inset: -10%; width: 120%; height: 120%; object-fit: cover; filter: blur(80px) saturate(1.5); opacity: 0.4; }
  .amb::after { content: ""; position: absolute; inset: 0; background: linear-gradient(90deg, rgba(16, 17, 20, 0.2), rgba(16, 17, 20, 0.9) 70%); }
  .tstack { position: relative; min-width: 0; align-self: center; }
  .tstack .hint { margin-top: 22px; text-align: center; color: var(--text-3); font-size: 12px; }
  .info { position: relative; display: flex; flex-direction: column; gap: 12px; align-self: center; min-width: 0; }
  .kick { font-size: 12px; font-weight: 700; color: var(--text-2); }
  .info h2 { margin: 0; font-size: 34px; font-weight: 850; letter-spacing: -0.03em; line-height: 1.05; }
  .info h2.long { font-size: 26px; }
  .meta { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; font-size: 13px; color: var(--text-2); }
  .meta i { width: 3px; height: 3px; border-radius: 50%; background: var(--text-3); }
  .meta .genre, .meta .rt { padding: 2px 8px; border-radius: 99px; background: var(--glass-2); font-size: 11.5px; }
  .meta .rt { color: #f5c518; }
  .ov { margin: 0; color: var(--text-2); font-size: 13.5px; line-height: 1.55; display: -webkit-box; -webkit-line-clamp: 5; line-clamp: 5; -webkit-box-orient: vertical; overflow: hidden; }
  .cast { color: var(--text-3); font-size: 12.5px; }
  .votes { display: flex; gap: 22px; margin-top: 4px; }
  .vlab { display: flex; flex-direction: column; align-items: center; gap: 6px; font-size: 12px; color: var(--text-2); }
  .vb { width: 64px; height: 64px; border-radius: 50%; border: 1px solid var(--line-2); background: rgba(0, 0, 0, 0.35); font-size: 24px; display: grid; place-items: center; transition: transform 0.15s; }
  .vb:hover { transform: scale(1.08); }
  .vb.pass { color: #ff6961; }
  .vb.like { color: var(--green); }

  .waiting { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 16px; text-align: center; padding: 30px; }
  .waiting h2 { margin: 0; font-size: 20px; max-width: 520px; }
  .spin { width: 34px; height: 34px; border-radius: 50%; border: 3px solid var(--glass-2); border-top-color: var(--accent); animation: sp 0.9s linear infinite; }
  @keyframes sp { to { transform: rotate(360deg); } }

  .wheel { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 40px; padding: 30px 40px; align-items: center; min-height: 0; overflow-y: auto; }
  .wl { display: grid; place-items: center; }
  .wr { display: flex; flex-direction: column; gap: 16px; min-width: 0; }
  .liked { display: flex; flex-direction: column; gap: 8px; max-height: 300px; overflow-y: auto; }
  .li { display: flex; align-items: center; gap: 12px; text-align: left; padding: 8px; border-radius: 12px; border: 1px solid transparent; background: var(--glass); }
  .li:hover { background: var(--glass-2); }
  .li.win { border-color: #f5c518; }
  .lp { flex: none; position: relative; width: 38px; aspect-ratio: 2 / 3; border-radius: 6px; overflow: hidden; background: var(--p); }
  .lp img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .lt { display: flex; flex-direction: column; min-width: 0; }
  .lt b { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lt small { color: var(--text-3); font-size: 12px; }
  .won { font-size: 18px; }

  .detail { padding: 24px 30px; overflow-y: auto; min-height: 0; }
  .d-top { display: flex; gap: 8px; margin-bottom: 12px; }

  .tr-over { position: absolute; inset: 64px 0 0; z-index: 8; background: rgba(0, 0, 0, 0.75); display: grid; place-items: center; backdrop-filter: blur(6px); }
  .tr-box { width: min(86%, 1000px); aspect-ratio: 16 / 9; border-radius: 16px; overflow: hidden; box-shadow: 0 40px 120px rgba(0, 0, 0, 0.8); }
  .tr-box iframe { width: 100%; height: 100%; border: 0; }
</style>
