<script lang="ts">
  const hour = new Date().getHours();
  // "Ne izlesek?" v2 (PROTOCOL §3.9a): Sinema'nın içinde, pencere genişliğinde açılır.
  // Host film/dizi ve kişi başı öneri sayısını kilitler → herkeste geri sayım → Kur
  // (herkes kendi havası/filtresi, herkes hazır olunca geçer) → Öner (filtreye göre
  // afişler + arama, künye ve fragman; isimsiz havuz) → Oyla (havuz destesi) → Eşleşme.
  // Açıkken video penceresi gizlidir (App), fragman burada güvenle oynar.
  import { fade, fly, scale } from "svelte/transition";
  import { cubicOut, backOut } from "svelte/easing";
  import { fun, nextCard } from "./fun.svelte";
  import { listsGet, syncFun, type ListEntry, type WantEntry, type Movie, type NightFilter, type NightKind } from "./player";
  import Wheel from "./Wheel.svelte";
  import ListButtons from "./ListButtons.svelte";
  import { social } from "./social.svelte";
  import DualRange from "./DualRange.svelte";
  import { t, locale, i18n } from "./i18n.svelte";

  let { me, canControl, hue, people, away = false, onClose, onError, onFind }: {
    me: string | null;
    /** Odadaki kişi sayısı (host başlatma ekranı). */
    people: number;
    /** Odada bağlantısı geçici kopmuş üye var mı. */
    away?: boolean;
    canControl: boolean;
    hue: (name: string) => number;
    onClose: () => void;
    onError: (msg: string) => void;
    /** Eşleşen başlık için kaynak bul (Video ekle). */
    onFind: (m: Movie) => void;
  } = $props();

  const send = (msg: Record<string, unknown>) => syncFun(msg).catch((e) => onError(String(e)));
  const n = $derived(fun.night);
  const tv = $derived(n?.kind === "tv");
  const noun = $derived(tv ? t("dizi") : t("film"));
  const kindLine = $derived(n ? (tv ? t("📺 Dizi gecesi · kişi başı {n} öneri", { n: n.perUser }) : t("🎬 Film gecesi · kişi başı {n} öneri", { n: n.perUser })) : "");
  const meM = $derived(n?.members.find((m) => m.id === me) ?? null);
  /** Sunucuya Türkçe giden hava etiketini ("A + B") gösterirken çevirir. */
  const lbl = (s?: string) => (s ? s.split(" + ").map((x) => t(x)).join(" + ") : "");
  const initial = (s: string) => (s.trim()[0] ?? "?").toLocaleUpperCase(locale());

  // ---- Host başlatır --------------------------------------------------------
  let hostKind = $state<NightKind>("movie");
  let hostPer = $state(3);
  /** Biten oturumdan sonra "Yeni tur". */
  let again = $state(false);
  const showHost = $derived(!n || again);
  function start() {
    again = false;
    send({ t: "night_start", kind: hostKind, perUser: hostPer });
  }

  // ---- Açılış: yeni oturumda herkeste 3-2-1 ----------------------------------
  let introId = $state("");
  let count = $state(0);
  let seenId = "";
  $effect(() => {
    const id = n?.id ?? "";
    if (!id || id === seenId) return;
    seenId = id;
    resetLocal();
    if (n!.phase !== "setup") return;
    introId = id;
    count = 3;
    const t = setInterval(() => {
      count -= 1;
      if (count <= 0) { clearInterval(t); introId = ""; }
    }, 900);
  });

  // ---- 1 · Kur ---------------------------------------------------------------
  interface MoodTile { id: string; title: string; sub: string; grad: string; f: NightFilter }
  const MOODS: MoodTile[] = [
    { id: "gerim", title: "Gerim gerim", sub: "Gerilim, gizem, son sahnede ağzın açık kalsın", grad: "linear-gradient(135deg,#3a1414,#7a1f1f)", f: { genres: [53, 9648, 80], voteMin: 6.8 } },
    { id: "hafif", title: "Kafa dağıtmalık", sub: "Hafif komedi, kafa yormaz", grad: "linear-gradient(135deg,#3a2e0e,#8a6a12)", f: { genres: [35], without: [27, 53], voteMin: 6.3 } },
    { id: "bilim", title: "Akıl yakan bilimkurgu", sub: "Büyük fikirler, büyük ekran", grad: "linear-gradient(135deg,#0e2a3a,#14526f)", f: { genres: [878], voteMin: 7 } },
    { id: "kalp", title: "Kalbe dokunsun", sub: "Romantik, biraz da hüzün", grad: "linear-gradient(135deg,#2a0e2a,#6a1a5a)", f: { genres: [10749, 18], voteMin: 7 } },
    { id: "korku", title: "Işıkları kapat", sub: "Korku; yalnız izlenmez", grad: "linear-gradient(135deg,#1a1a1a,#4a4a4a)", f: { genres: [27], voteMin: 6.2 } },
    { id: "herkes", title: "Hepimize uyar", sub: "Animasyon, her yaşa", grad: "linear-gradient(135deg,#0e3a20,#1f7a3f)", f: { genres: [16, 10751], voteMin: 7 } },
  ];
  const YEAR = new Date().getFullYear();
  const ERAS = [
    { id: "80", label: "80'ler", from: 1980, to: 1989 },
    { id: "90", label: "90'lar", from: 1990, to: 1999 },
    { id: "00", label: "2000'ler", from: 2000, to: 2014 },
    { id: "new", label: "Yeni", from: 2015, to: YEAR },
  ];
  const GENRES: [number, string][] = [
    [53, "Gerilim"], [80, "Suç"], [9648, "Gizem"], [18, "Dram"], [28, "Aksiyon"], [878, "Bilimkurgu"], [27, "Korku"], [35, "Komedi"],
    [10749, "Romantik"], [16, "Animasyon"], [99, "Belgesel"], [10752, "Savaş"], [37, "Western"], [14, "Fantastik"], [12, "Macera"], [10751, "Aile"],
  ];
  const LANGS: [string, string][] = [["en", "İngilizce"], ["tr", "Türkçe"], ["ko", "Korece"], ["fr", "Fransızca"], ["es", "İspanyolca"], ["ja", "Japonca"], ["it", "İtalyanca"], ["de", "Almanca"], ["hi", "Hintçe"]];
  const COUNTS = [0, 100, 500, 1000, 5000, 20000, 50000, 100000];

  let moods = $state<string[]>([]);
  let maxRt = $state(0); // 0 = fark etmez
  let eras = $state<string[]>([]);
  let vote = $state(0);
  let custom = $state<NightFilter | null>(null);
  let panel = $state(false);
  type Draft = { inc: number[]; exc: number[]; y0: number; y1: number; r0: number; r1: number; vote: number; count: number; langs: string[]; hideSeen: boolean };
  const RT = $derived(tv ? { min: 15, max: 90 } : { min: 60, max: 200 });
  const blankDraft = (): Draft => ({ inc: [], exc: [], y0: 1960, y1: YEAR, r0: RT.min, r1: RT.max, vote: 0, count: 0, langs: [], hideSeen: true });
  let draft = $state<Draft>(blankDraft());

  function resetLocal() {
    moods = []; maxRt = 0; eras = []; vote = 0; custom = null; panel = false; draft = blankDraft();
    suggestedFor = ""; query = ""; detailOf = null; trailerKey = null;
  }

  function toggleMood(id: string) {
    if (meM?.ready) return;
    custom = null;
    moods = moods.includes(id) ? moods.filter((m) => m !== id) : [...moods.slice(-1), id]; // en fazla 2
  }
  function toggleEra(id: string) {
    if (meM?.ready) return;
    eras = eras.includes(id) ? eras.filter((e) => e !== id) : [...eras, id];
  }
  function cycleGenre(id: number) {
    if (draft.inc.includes(id)) { draft.inc = draft.inc.filter((g) => g !== id); draft.exc = [...draft.exc, id]; }
    else if (draft.exc.includes(id)) draft.exc = draft.exc.filter((g) => g !== id);
    else draft.inc = [...draft.inc, id];
  }
  const toggleLang = (l: string) => (draft.langs = draft.langs.includes(l) ? draft.langs.filter((x) => x !== l) : [...draft.langs, l]);
  function draftFilter(d: Draft): NightFilter {
    return {
      genres: d.inc, without: d.exc,
      yearFrom: d.y0 > 1960 ? d.y0 : undefined, yearTo: d.y1 < YEAR ? d.y1 : undefined,
      runtimeMin: d.r0 > RT.min ? d.r0 : undefined, runtimeMax: d.r1 < RT.max ? d.r1 : undefined,
      voteMin: d.vote || undefined, voteCountMin: d.count || undefined, languages: d.langs, hideSeen: d.hideSeen,
    };
  }
  function applyCustom() {
    custom = draftFilter(draft);
    moods = [];
    panel = false;
  }

  const myFilter = $derived.by((): NightFilter | null => {
    if (custom) return custom;
    const picked = MOODS.filter((m) => moods.includes(m.id));
    if (!picked.length) return null;
    const f: NightFilter = {
      genres: [...new Set(picked.flatMap((m) => m.f.genres ?? []))],
      without: picked.length === 1 ? picked[0].f.without : undefined,
      voteMin: Math.max(vote, Math.min(...picked.map((m) => m.f.voteMin ?? 0))),
      hideSeen: true,
    };
    if (maxRt) f.runtimeMax = maxRt;
    const es = ERAS.filter((e) => eras.includes(e.id));
    if (es.length) { f.yearFrom = Math.min(...es.map((e) => e.from)); f.yearTo = Math.max(...es.map((e) => e.to)); }
    return f;
  });
  const myLabel = $derived(custom ? "Özel filtre" : MOODS.filter((m) => moods.includes(m.id)).map((m) => m.title).join(" + "));
  const myLine = $derived.by(() => {
    const f = myFilter;
    if (!f) return "";
    const p: string[] = [];
    if (f.yearFrom || f.yearTo) p.push(`${f.yearFrom ?? "…"} → ${f.yearTo && f.yearTo < YEAR ? f.yearTo : t("bugün")}`);
    if (f.runtimeMax) p.push(`≤ ${dur(f.runtimeMax)}`);
    if (f.voteMin) p.push(t("puan {v}+", { v: f.voteMin.toFixed(1) }));
    if (f.languages?.length) p.push(f.languages.map((l) => LANGS.find((x) => x[0] === l)?.[1] ?? l).map((x) => t(x)).join(", "));
    return p.join(" · ") || t("Süre ve dönem fark etmez");
  });
  const readyN = $derived(n ? n.members.filter((m) => m.ready).length : 0);

  function ready() {
    if (!n || !myFilter) return;
    if (!meM?.ready && away && people > 1) {
      onError(t("Odadaki diğer kişinin bağlantısı bekleniyor…"));
      return;
    }
    send({ t: "night_ready", ready: !meM?.ready, mood: myLabel });
  }

  // Afiş yelpazesi ve "n film" sayıları (sunucu önbellekli; yalnızca bana gelir).
  let previewedFor = "";
  $effect(() => {
    if (!n || n.phase !== "setup" || previewedFor === n.id + n.kind) return;
    previewedFor = n.id + n.kind;
    send({ t: "night_preview", filters: MOODS.map((m) => ({ tag: m.id, filter: m.f })) });
  });
  function debouncedPreview(tag: string, f: NightFilter | null, on: boolean) {
    if (!on || !f) return;
    const t = setTimeout(() => send({ t: "night_preview", filters: [{ tag, filter: f }] }), 550);
    return () => clearTimeout(t);
  }
  $effect(() => debouncedPreview("me", $state.snapshot(myFilter), n?.phase === "setup"));
  $effect(() => debouncedPreview("custom", draftFilter($state.snapshot(draft) as Draft), panel));

  // ---- 2 · Öner ----------------------------------------------------------------
  let suggestedFor = "";
  function suggest() {
    if (!myFilter) return;
    fun.suggest.busy = true;
    send({ t: "night_suggest", filter: $state.snapshot(myFilter) });
  }
  $effect(() => {
    if (n?.phase === "pick" && suggestedFor !== n.id) {
      suggestedFor = n.id;
      fun.suggest = { busy: true, items: [], total: 0, error: "" };
      if (myFilter) send({ t: "night_suggest", filter: $state.snapshot(myFilter) });
      else fun.suggest.busy = false;
    }
  });
  // Listelerinizden (p0.23): İzleyeceklerim'deki, bu oturumun türündeki başlıklar.
  let fromList = $state<WantEntry[]>([]);
  let listFor = "";
  $effect(() => {
    if (n?.phase !== "pick" || listFor === n.id || !social.server) return;
    listFor = n.id;
    const kind = n.kind === "tv" ? "series" : "movie";
    listsGet(social.server).then((l) => {
      fromList = (l?.want ?? []).filter((e) => e.kind === kind && e.ref.startsWith("tt"))
        .sort((a, b) => (b.mates?.length ?? 0) - (a.mates?.length ?? 0)).slice(0, 12);
    }, () => {});
  });
  const pickedRef = (ref: string) => mine.some((m) => m.imdb === ref);
  function pickRef(e: WantEntry) {
    if (!n || meM?.done || pickedRef(e.ref)) return;
    if (full) onError(t("En fazla {n} öneri seçebilirsin", { n: n.perUser }));
    else send({ t: "night_pick", imdb: e.ref });
  }
  let query = $state("");
  $effect(() => {
    const q = query.trim();
    if (!q || n?.phase !== "pick") return;
    const t = setTimeout(() => send({ t: "night_search", query: q }), 380);
    return () => clearTimeout(t);
  });
  const results = $derived(query.trim() && fun.results.query === query.trim() ? fun.results.items : []);
  const mine = $derived(n?.mine ?? []);
  const isMine = (id: number) => mine.some((m) => m.id === id);
  const full = $derived(!!n && mine.length + n.pending >= n.perUser);
  const doneN = $derived(n ? n.members.filter((m) => m.done).length : 0);
  function togglePick(m: Movie) {
    if (!n || meM?.done) return;
    if (isMine(m.id)) send({ t: "night_unpick", itemId: m.id });
    else if (full) onError(t("En fazla {n} öneri seçebilirsin", { n: n.perUser }));
    else send({ t: "night_pick", itemId: m.id });
  }

  // Künye + fragman
  let detailOf = $state<Movie | null>(null);
  let trailerOn = $state(false);
  function openDetail(m: Movie) {
    detailOf = m;
    trailerOn = false;
    if (fun.detail?.id !== m.id) send({ t: "night_detail", itemId: m.id });
    query = "";
  }
  /** Profil listeleri (p0.23): IMDb kimliği varsa o, yoksa TMDB kimliği. */
  function listEntry(m: Movie): ListEntry {
    const kind = n?.kind === "tv" ? "series" : "movie";
    return { ref: m.imdb || `tmdb:${kind}:${m.id}`, kind, title: m.title, year: m.year, poster: m.poster, source: "night" };
  }
  const detail = $derived(detailOf ? (fun.detail?.id === detailOf.id ? fun.detail : detailOf) : null);
  const more = $derived(fun.suggest.items.filter((m) => m.id !== detailOf?.id).slice(0, 6));

  // ---- 3 · Oyla ---------------------------------------------------------------
  const deck = $derived(n?.deckId && fun.deck?.id === n.deckId ? fun.deck : null);
  const card = $derived(nextCard(deck));
  const after = $derived(deck && card ? deck.items.slice(deck.items.indexOf(card) + 1, deck.items.indexOf(card) + 3) : []);
  const votedN = $derived(deck ? Object.keys(deck.mine).length : 0);
  function cast(like: boolean) {
    const d = deck;
    if (!d || !card || d.match || d.over) return;
    d.mine[card.id] = like;
    const remaining = d.items.filter((m) => !(m.id in d.mine)).length;
    const hasUnsentLikes = d.items.some((m) => d.mine[m.id] === true && !d.sent[m.id]);
    if (!like && (remaining > 0 || !hasUnsentLikes)) {
      d.sent[card.id] = true;
      send({ t: "swipe", deckId: d.id, itemId: card.id, vote: "pass" });
    }
  }
  $effect(() => {
    const d = deck;
    if (!d || d.match || d.over || (away && people > 1)) return;
    if (nextCard(d) !== null) return;
    const unsentLikes = d.items.filter((m) => d.mine[m.id] === true && !d.sent[m.id]);
    const unsentPasses = d.items.filter((m) => d.mine[m.id] === false && !d.sent[m.id]);
    for (const m of [...unsentLikes, ...unsentPasses]) {
      d.sent[m.id] = true;
      send({ t: "swipe", deckId: d.id, itemId: m.id, vote: d.mine[m.id] ? "like" : "pass" });
    }
  });
  let dx = $state(0);
  let dragFrom = $state<number | null>(null);
  function down(e: PointerEvent) { dragFrom = e.clientX; (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); }
  function move(e: PointerEvent) { if (dragFrom !== null) dx = e.clientX - dragFrom; }
  function up() {
    if (dragFrom === null) return;
    const d = dx;
    dragFrom = null; dx = 0;
    if (Math.abs(d) > 90) cast(d > 0);
  }
  let trailerKey = $state<string | null>(null);

  // Oylama kartındaki fragman: YouTube gömmesini postMessage ile yönetiriz (ayrı betik yüklemeden).
  let tvFrame = $state<HTMLIFrameElement | null>(null);
  let tvMuted = $state(true);
  let tvPaused = $state(false);
  let tvAt = $state(0);
  let tvLen = $state(0);
  $effect(() => {
    void card?.id;
    tvMuted = true; tvPaused = false; tvAt = 0; tvLen = 0;
  });
  function yt(func: string) {
    tvFrame?.contentWindow?.postMessage(JSON.stringify({ event: "command", func, args: [] }), "*");
  }
  function ytListen() {
    tvFrame?.contentWindow?.postMessage(JSON.stringify({ event: "listening", id: 1, channel: "widget" }), "*");
  }
  function ytMute() { tvMuted = !tvMuted; yt(tvMuted ? "mute" : "unMute"); }
  function ytToggle() { tvPaused = !tvPaused; yt(tvPaused ? "pauseVideo" : "playVideo"); }
  function ytInfo(e: MessageEvent) {
    if (!tvFrame || e.source !== tvFrame.contentWindow || typeof e.data !== "string") return;
    try {
      const info = JSON.parse(e.data)?.info;
      if (!info) return;
      if (typeof info.currentTime === "number") tvAt = info.currentTime;
      if (typeof info.duration === "number" && info.duration > 0) tvLen = info.duration;
      if (info.playerState === 1) tvPaused = false;
      else if (info.playerState === 2 || info.playerState === 0) tvPaused = true;
    } catch { /* YouTube dışı ileti */ }
  }
  const clock = (sec: number) => `${Math.floor(sec / 60)}:${String(Math.floor(sec % 60)).padStart(2, "0")}`;

  // ---- Adımlar -------------------------------------------------------------------
  const phase = $derived(n?.phase ?? "setup");
  const STEPS = [["setup", "Kur"], ["pick", "Öner"], ["vote", "Oyla"], ["done", "Eşleşme"]] as const;
  const stepIdx = $derived(STEPS.findIndex((s) => s[0] === phase));

  function onKey(e: KeyboardEvent) {
    if ((e.target as HTMLElement)?.closest?.("input, textarea")) {
      if (e.key === "Escape") query = "";
      return;
    }
    if (e.key === "Escape") {
      e.stopPropagation();
      if (trailerKey) trailerKey = null;
      else if (panel) panel = false;
      else if (detailOf) detailOf = null;
      else onClose();
    } else if (phase === "vote" && !trailerKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
      e.preventDefault();
      cast(e.key === "ArrowRight");
    } else if (phase === "vote" && !trailerKey && card?.trailer && (e.key === " " || e.key.toLowerCase() === "m")) {
      e.preventDefault();
      if (e.key === " ") ytToggle(); else ytMute();
    }
  }

  function dur(min: number) {
    const h = Math.floor(min / 60), m = min % 60;
    return h ? t("{n} sa", { n: h }) + (m ? " " + t("{n} dk", { n: m }) : "") : t("{n} dk", { n: m });
  }
  const rating = (m: Movie) => m.ratings.imdb ?? (m.ratings.tmdb ? m.ratings.tmdb.toFixed(1) : "");
  const sub = (m: Movie) => [m.year, rating(m)].filter(Boolean).join(" · ");
  const votes = (v?: number) => (!v ? "" : v >= 1e6 ? t("{n} M oy", { n: (v / 1e6).toFixed(1).replace(".", i18n.lang === "tr" ? "," : ".") }) : v >= 1000 ? t("{n} B oy", { n: Math.round(v / 1000) }) : t("{n} oy", { n: v }));
  const metaOf = (m: Movie) => [m.year, m.runtime ? (tv ? t("bölüm {n} dk", { n: m.runtime }) : dur(m.runtime)) : null, m.seasons ? (m.seasons === 1 ? t("1 sezon") : t("{n} sezon", { n: m.seasons })) : null].filter(Boolean) as (string | number)[];
  const grad = (t: string) => `linear-gradient(160deg, hsl(${hue(t)} 45% 32%), hsl(${hue(t)} 40% 8%))`;
</script>

<svelte:window onkeydown={onKey} onmessage={ytInfo} />

{#snippet poster(m: Movie, cls = "")}
  <div class="poster {cls}" style="--p:{grad(m.title)}">
    {#if m.poster}<img src={m.poster} alt="" draggable="false" onerror={(e) => { (e.currentTarget as HTMLElement).style.display = "none"; }} />{:else}<b>{m.title}</b>{/if}
  </div>
{/snippet}

{#snippet av(name: string, cls = "")}
  <span class="av {cls}" style="--h:{hue(name)}" title={name}>{initial(name)}</span>
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
    {#each metaOf(m) as x, i}{#if i}<i></i>{/if}<span>{x}</span>{/each}
    {#each (m.genres ?? []).slice(0, 3) as g}<span class="genre">{t(g)}</span>{/each}
  </div>
{/snippet}

<div class="scrim" transition:fade={{ duration: 220 }} onclick={onClose} role="presentation"></div>

{#if showHost}
  <div class="hostm" transition:fade={{ duration: 200 }}>
    <div class="hm-card" in:scale={{ start: 0.92, duration: 320, easing: backOut }}>
      <div class="hm-head"><b>✦</b><div><h3>{t("Ne izlesek? başlat")}</h3><small>{canControl ? t("Seçimin herkese kilitlenir") : t("Oturumu host ya da yardımcı host başlatır")}</small></div>
        <button class="x" onclick={() => (again ? (again = false) : onClose())} title={t("Kapat (Esc)")}>✕</button></div>
      {#if canControl}
        <div class="hm-l">{hour < 5 ? t("Bu gece ne izliyoruz?") : hour < 12 ? t("Bu sabah ne izliyoruz?") : hour < 18 ? t("Bugün ne izliyoruz?") : t("Bu akşam ne izliyoruz?")}</div>
        <div class="kinds">
          <button class="k" class:on={hostKind === "movie"} onclick={() => (hostKind = "movie")}><span>🎬</span><b>{t("Film")}</b><small>{t("tek oturuşluk")}</small></button>
          <button class="k" class:on={hostKind === "tv"} onclick={() => (hostKind = "tv")}><span>📺</span><b>{t("Dizi")}</b><small>{t("yeni bir seriye başla")}</small></button>
        </div>
        <div class="hm-l">{t("Kişi başı en fazla öneri")}</div>
        <div class="nums">{#each [1, 2, 3, 4, 5] as k}<button class:on={hostPer === k} onclick={() => (hostPer = k)}>{k}</button>{/each}</div>
        <div class="hm-note">{t("{people} kişi × {per} öneri = en fazla", { people, per: hostPer })} <b>{t("{n} kartlık", { n: people * hostPer })}</b> {t("deste")}</div>
        <button class="cta" onclick={start}>{t("Herkesi çağır")} <span>→</span></button>
      {:else}
        <p class="hm-note">{t("Host başlatınca bu ekran herkeste geri sayımla açılır.")}</p>
      {/if}
    </div>
  </div>
{:else if n && introId === n.id}
  <div class="intro" transition:fade={{ duration: 350 }}>
    <div class="rays"></div>
    <div class="ib">
      <div class="i-k" in:fly={{ y: -16, duration: 500 }}>{t("{name} başlattı · {line}", { name: n.by, line: kindLine })}</div>
      <h2 in:scale={{ start: 0.7, duration: 700, easing: backOut }}>{t("Ne izlesek?")}</h2>
      <div class="count">
        <svg viewBox="0 0 120 120"><circle cx="60" cy="60" r="54" class="bg" /><circle cx="60" cy="60" r="54" class="fg" style="stroke-dashoffset:{339 - (339 * Math.max(count, 0)) / 3}" /></svg>
        {#key count}<b in:scale={{ start: 1.8, duration: 450, easing: cubicOut }}>{Math.max(count, 1)}</b>{/key}
      </div>
      <div class="i-av">{#each n.members as m, i (m.id)}<span in:scale={{ delay: 250 + i * 90, duration: 380, easing: backOut }}>{@render av(m.name)}</span>{/each}</div>
      <div class="i-sub">{t("Patlamış mısırlar hazır mı? 🍿")}</div>
    </div>
  </div>
{:else if n}
  <section class="sheet" in:fly={{ y: 40, duration: 480, easing: cubicOut }} out:fade={{ duration: 180 }}>
    <div class="sh-head">
      <h1><b>✦</b>{t("Ne izlesek?")}</h1>
      <div class="steps">
        {#each STEPS as [k, label], i}
          <span class:on={i === stepIdx} class:done={i < stepIdx}><i>{i < stepIdx ? "✓" : i + 1}</i>{t(label)}</span>
        {/each}
      </div>
      <div class="who">
        {#each n.members as m (m.id)}{@render av(m.name)}{/each}
        <span class="kl">{kindLine}</span>
      </div>
      {#if canControl}<button class="ghost sm" onclick={() => send({ t: "night_close" })} title={t("Oturumu herkes için bitir")}>{t("Bitir")}</button>{/if}
      <button class="x" title={t("Kapat (Esc) · oturum sürer, ✦ ile dönersin")} onclick={onClose}>✕</button>
    </div>

    {#key phase}
      <div class="body" in:fly={{ x: 40, duration: 420, easing: cubicOut }}>
        {#if phase === "setup"}
          <div class="two-col">
            <div class="main">
              <h2 class="q">{t("Sen ne havadasın?")}<span>{t("Herkes kendi seçer, öneriler havuza gider.")}</span></h2>
              <div class="row"><span class="lock">{tv ? t("📺 Dizi gecesi") : t("🎬 Film gecesi")} <em>{t("🔒 host seçti")}</em></span><span class="soft">{t("Bir ya da iki hava seç, ya da kendi filtreni kur")}</span></div>
              <div class="moods" class:locked={meM?.ready}>
                {#each MOODS as m, i (m.id)}
                  {@const pv = fun.previews[m.id]}
                  <button class="mood" class:on={moods.includes(m.id)} style="--m:{m.grad}" onclick={() => toggleMood(m.id)}
                    in:fly={{ y: 18, delay: 60 * i, duration: 420, easing: cubicOut }}>
                    {#if moods.includes(m.id)}<span class="tick" in:scale={{ duration: 250, easing: backOut }}>✓</span>{/if}
                    <div class="fan">
                      {#each pv?.posters ?? [] as src}<div class="poster"><img {src} alt="" /></div>{/each}
                    </div>
                    <h3>{t(m.title)}</h3><p>{t(m.sub)}</p>
                  </button>
                {/each}
              </div>
              <div class="filters" class:dim={!!custom}>
                <div class="f">
                  <div class="lab">{tv ? t("Bölüm süresi") : t("Süre")} <b>{maxRt ? t("en fazla {d}", { d: dur(maxRt) }) : t("fark etmez")}</b></div>
                  <input class="range" type="range" min={tv ? 20 : 80} max={tv ? 70 : 190} step={tv ? 5 : 5} disabled={meM?.ready || !!custom}
                    value={maxRt || (tv ? 70 : 190)} oninput={(e) => { const v = Number(e.currentTarget.value); maxRt = v >= (tv ? 70 : 190) ? 0 : v; }}
                    style="--w:{(((maxRt || (tv ? 70 : 190)) - (tv ? 20 : 80)) / (tv ? 50 : 110)) * 100}%" />
                  <div class="ticks">{#if tv}<span>{t("{n} dk", { n: 20 })}</span><span>{t("{n} dk", { n: 45 })}</span><span>{t("fark etmez")}</span>{:else}<span>{t("{n} dk", { n: 80 })}</span><span>{t("{n} sa", { n: 2 })}</span><span>{t("fark etmez")}</span>{/if}</div>
                </div>
                <div class="f">
                  <div class="lab">{t("Dönem")} <b>{eras.length ? "" : t("hepsi")}</b></div>
                  <div class="eras">{#each ERAS as e}<button class:on={eras.includes(e.id)} disabled={meM?.ready || !!custom} onclick={() => toggleEra(e.id)}>{t(e.label)}</button>{/each}</div>
                </div>
                <div class="f">
                  <div class="lab">{t("Puan")} <b>{vote ? `${vote.toFixed(1)}+` : t("fark etmez")}</b></div>
                  <input class="range" type="range" min="0" max="8.5" step="0.5" bind:value={vote} disabled={meM?.ready || !!custom} style="--w:{(vote / 8.5) * 100}%" />
                  <div class="ticks"><span>{t("fark etmez")}</span><span>7</span><span>8.5+</span></div>
                </div>
              </div>
              <button class="custom" class:on={!!custom} disabled={meM?.ready} onclick={() => (panel = true)}>
                ⚙ {custom ? t("Özel filtren hazır") : t("Özel filtre kur")} <span>{t("Tür dahil/hariç, yıl, süre, dil, oy sayısı, izlediklerimizi gizle")}</span><b>→</b>
              </button>
            </div>
            <aside class="side">
              <h4>{t("Odadakiler")}</h4>
              <div class="ready">
                {#each n.members as m (m.id)}
                  <div>
                    {@render av(m.name)}
                    <span>{m.name}<small>{m.id === me ? t("sen · {x}", { x: m.ready ? lbl(m.mood) || t("hazır") : lbl(myLabel) || t("seçiyor") }) : m.ready ? lbl(m.mood) || t("hazır") : t("seçiyor")}</small></span>
                    {#if m.ready}<em in:scale={{ duration: 260, easing: backOut }}>✓ {t("hazır")}</em>{:else}<em class="wait">…</em>{/if}
                  </div>
                {/each}
              </div>
              <div class="sum">
                {#if myFilter}
                  <b>{t("Senin seçimin: {label}", { label: lbl(myLabel) })}</b>{myLine}
                  {#if fun.previews.me}<br />{t("Sana bu filtreyle")} <b class="in">{fun.previews.me.total.toLocaleString(locale())}</b> {t("{noun} arasından öneri gelecek.", { noun })}{/if}
                {:else}
                  <b>{t("Bir hava seç")}</b>{t("Ya da özel filtreni kur. Seçince ne kadar {noun} çıktığını burada görürsün.", { noun })}
                {/if}
              </div>
              <button class="cta" class:on={meM?.ready} disabled={!myFilter} onclick={ready}>
                {meM?.ready ? t("Hazırım ✓") : t("Hazırım")} <span class="cnt">{t("{a}/{b} hazır", { a: readyN, b: n.members.length })}</span>
              </button>
              <div class="fine">{meM?.ready ? t("Değiştirmek için tekrar bas.") + " " : ""}{t("Herkes hazır olunca öneri adımı kendiliğinden açılır.")}</div>
            </aside>
          </div>

          {#if panel}
            <div class="fpanel" transition:fly={{ x: 440, duration: 380, easing: cubicOut }}>
              <div class="fp-head"><b>{t("Özel filtre")}</b><button class="x" onclick={() => (panel = false)}>✕</button></div>
              <div class="fp-body">
                <div class="fg"><div class="fl">{t("Türler")} <small>{t("bir kez: dahil · iki kez: hariç")}</small></div>
                  <div class="tags">{#each GENRES as [id, name]}<button class:in={draft.inc.includes(id)} class:ex={draft.exc.includes(id)} onclick={() => cycleGenre(id)}>{t(name)}</button>{/each}</div></div>
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
                  <div class="tags">{#each LANGS as [id, name]}<button class:in={draft.langs.includes(id)} onclick={() => toggleLang(id)}>{t(name)}</button>{/each}</div></div>
                <button class="sw" onclick={() => (draft.hideSeen = !draft.hideSeen)}>
                  <span><b>{t("İzlediklerimizi gizle")}</b><small>{t("Bu odada daha önce destede çıkanlar")}</small></span><i class:on={draft.hideSeen}></i></button>
              </div>
              <div class="fp-foot">
                <span>{#if fun.previews.custom}<b>{fun.previews.custom.total.toLocaleString(locale())}</b> {t("{noun} uygun", { noun })}{/if}</span>
                <button class="ghost sm" onclick={() => (draft = blankDraft())}>{t("Temizle")}</button>
                <button class="cta sm" onclick={applyCustom}>{t("Uygula")}</button>
              </div>
            </div>
          {/if}

        {:else if phase === "pick" && detail}
          <div class="detail">
            <button class="d-back" onclick={() => (detailOf = null)}>← {t("Önerilere dön")}</button>
            <div class="d-grid">
              <div class="trailer">
                {#if trailerOn && detail.trailer}
                  <iframe title={t("Fragman")} allow="autoplay; encrypted-media; fullscreen" allowfullscreen referrerpolicy="strict-origin-when-cross-origin"
                    src="https://www.youtube-nocookie.com/embed/{detail.trailer}?autoplay=1&modestbranding=1&playsinline=1&rel=0"></iframe>
                {:else}
                  <div class="tr-img" style="--p:{grad(detail.title)}">{#if detail.poster}<img src={detail.poster} alt="" />{/if}</div>
                  {#if detail.trailer}
                    <button class="tr-play" onclick={() => (trailerOn = true)} title={t("Fragmanı oynat")}>▶</button>
                    <span class="tr-cap">{t("Resmî fragman")}</span>
                  {:else}
                    <span class="tr-cap">{fun.detail?.id === detail.id ? t("Fragman bulunamadı") : t("Künye yükleniyor…")}</span>
                  {/if}
                {/if}
              </div>
              <div class="d-info">
                <div class="d-kick">{myLabel ? t("Sana göre · {x}", { x: lbl(myLabel) }) : t("Künye")}</div>
                <h2>{detail.title}</h2>
                {#if detail.original}<div class="orig">{detail.original}</div>{/if}
                {@render meta(detail)}
                {@render ratings(detail)}
                {#if detail.overview}<p class="ov">{detail.overview}</p>{/if}
                <dl class="kunye">
                  {#if detail.director}<dt>{tv ? t("Yaratıcı") : t("Yönetmen")}</dt><dd>{detail.director}</dd>{/if}
                  {#if detail.cast?.length}<dt>{t("Oyuncular")}</dt><dd>{detail.cast.join(", ")}</dd>{/if}
                  {#if detail.countries?.length}<dt>{t("Ülke")}</dt><dd>{detail.countries.join(", ")}</dd>{/if}
                </dl>
                <div class="row">
                  <button class="cta sm wide" class:on={isMine(detail.id)} disabled={meM?.done || (!isMine(detail.id) && full)} onclick={() => togglePick(detail)}>
                    {isMine(detail.id) ? t("✓ Önerilerimde · çıkar") : t("+ Önerilerime ekle")} <span class="cnt">{mine.length}/{n.perUser}</span></button>
                </div>
                <ListButtons entry={listEntry(detail)} />
              </div>
            </div>
            {#if more.length}
              <div class="d-more"><span>{t("Bunlar da var")}</span>
                {#each more as m (m.id)}<button class="pbtn" onclick={() => openDetail(m)} title={m.title}>{@render poster(m)}</button>{/each}
              </div>
            {/if}
          </div>

        {:else if phase === "pick"}
          <div class="two-col">
            <div class="main">
              <div class="p-top">
                <h2 class="q">{t("Ne öneriyorsun?")}<span>{t("En fazla {n} {noun}. Önerilerin isimsiz havuza gider.", { n: n.perUser, noun })}</span></h2>
                <label class="search">🔍
                  <input placeholder={tv ? t("Aklındaki bir diziyi ara…") : t("Aklındaki bir filmi ara…")} bind:value={query} disabled={meM?.done} />
                  {#if results.length}
                    <div class="drop" transition:fly={{ y: -6, duration: 180 }}>
                      {#each results as m (m.id)}
                        <div class="dr">
                          <button class="dr-open" onclick={() => openDetail(m)}>{@render poster(m)}<span><b>{m.title}</b><small>{sub(m)}</small></span></button>
                          <button class="add" class:on={isMine(m.id)} disabled={!isMine(m.id) && full} onclick={() => togglePick(m)}>{isMine(m.id) ? t("✓ Önerildi") : t("+ Öner")}</button>
                        </div>
                      {/each}
                    </div>
                  {/if}
                </label>
              </div>
              {#if fromList.length}
                <div class="lab2">{t("Listelerinizden")} <span>{t("İzleyeceklerin · ORTAK olanlar önde")}</span></div>
                <div class="fl">
                  {#each fromList as e (e.ref)}
                    <button class="fli" class:on={pickedRef(e.ref)} disabled={meM?.done || pickedRef(e.ref)} title={pickedRef(e.ref) ? t("Önerildi") : t("Öner")} onclick={() => pickRef(e)}>
                      <span class="flp" style="--p:{grad(e.title)}">{#if e.poster}<img src={e.poster} alt="" />{/if}{#if e.mates?.length}<em>{t("ORTAK")}</em>{/if}</span>
                      <span class="flt">{pickedRef(e.ref) ? "✓ " : ""}{e.title}</span>
                    </button>
                  {/each}
                </div>
              {/if}
              <div class="lab2">{t("Sana göre · {x}", { x: lbl(myLabel) || t("popüler") })} <span><button class="link" onclick={suggest} disabled={fun.suggest.busy}>↻ {t("başka göster")}</button> · {t("Afişe tıkla: künye ve fragman")}</span></div>
              {#if fun.suggest.error}<div class="banner">{fun.suggest.error}</div>{/if}
              <div class="grid">
                {#if fun.suggest.busy && !fun.suggest.items.length}
                  {#each Array(10) as _}<div class="pc"><div class="poster skel"></div><span class="skel-t"></span></div>{/each}
                {/if}
                {#each fun.suggest.items as m, i (m.id)}
                  <div class="pc" class:sel={isMine(m.id)} in:fly={{ y: 16, delay: Math.min(i, 12) * 35, duration: 360 }}>
                    <button class="pbtn" onclick={() => openDetail(m)}>{@render poster(m)}</button>
                    <button class="pick-t" class:on={isMine(m.id)} title={isMine(m.id) ? t("Önerilerimden çıkar") : t("Öner")} disabled={meM?.done || (!isMine(m.id) && full)} onclick={() => togglePick(m)}>{isMine(m.id) ? "✓" : "+"}</button>
                    <span>{m.title}<small>{sub(m)}</small></span>
                  </div>
                {/each}
              </div>
            </div>
            <aside class="side">
              <h4>{t("Önerilerin · {a}/{b}", { a: mine.length, b: n.perUser })}</h4>
              <div class="slots">
                {#each mine as m (m.id)}
                  <div class="slot" in:fly={{ x: 20, duration: 300 }}>{@render poster(m)}<span><b>{m.title}</b><small>{m.year ?? ""}</small></span>
                    {#if !meM?.done}<button class="xs" title={t("Çıkar")} onclick={() => togglePick(m)}>✕</button>{/if}</div>
                {/each}
                {#each Array(n.pending) as _}<div class="slot empty">{t("Künye geliyor…")}</div>{/each}
                {#if !full}<div class="slot empty">{t("+ {k} {noun} daha seçebilirsin", { k: n.perUser - mine.length - n.pending, noun })}</div>{/if}
              </div>
              <h4>{t("Havuz")}</h4>
              <div class="pool"><div><b>{n.pool}</b> {t("{noun} toplandı", { noun })}</div>
                {#each n.members.filter((m) => m.id !== me) as m (m.id)}
                  <div class="who2">{@render av(m.name)}<em class:wait={!m.done}>{m.done ? t("önerilerini gönderdi") : t("hâlâ seçiyor")}</em></div>
                {/each}
              </div>
              <button class="cta" class:on={meM?.done} onclick={() => {
                if (!meM?.done && away && people > 1) {
                  onError(t("Odadaki diğer kişinin bağlantısı bekleniyor…"));
                  return;
                }
                send({ t: "night_done", ready: !meM?.done });
              }}>
                {meM?.done ? t("Gönderildi ✓") : t("Önerilerim hazır")} <span class="cnt">{doneN}/{n.members.length}</span></button>
              <div class="fine">{meM?.done ? t("Geri almak için tekrar bas.") + " " : mine.length ? "" : t("Öneri seçmeden de gönderebilirsin.") + " "}{t("Herkes gönderince havuz karıştırılır ve oylama açılır.")}</div>
            </aside>
          </div>

        {:else if phase === "vote" && deck}
          <div class="deck" style="--p:{card ? grad(card.title) : 'none'}">
            {#if card}
              {#key card.id}<div class="amb" in:fade={{ duration: 600 }}>{#if card.poster}<img src={card.poster} alt="" />{/if}</div>{/key}
              <div class="tstack">
                <div class="tcard">
                  {#each [...after].reverse() as m, i (m.id)}<div class="under u{after.length - i}"></div>{/each}
                  {#key card.id}
                    <div class="tv" role="group" aria-label={t("Kart")} in:fly={{ y: 30, duration: 380, easing: cubicOut }}
                      style="transform: translateX({dx}px) rotate({-1.2 + dx / 40}deg); transition: {dragFrom === null ? 'transform .25s' : 'none'}">
                      {#if card.trailer}
                        <iframe bind:this={tvFrame} title={t("Fragman")} referrerpolicy="strict-origin-when-cross-origin" allow="autoplay; encrypted-media" onload={ytListen}
                          src="https://www.youtube-nocookie.com/embed/{card.trailer}?autoplay=1&mute=1&controls=0&cc_load_policy=1&cc_lang_pref={i18n.lang}&hl={i18n.lang}&modestbranding=1&playsinline=1&rel=0&iv_load_policy=3&enablejsapi=1"></iframe>
                      {:else}
                        <div class="nf-bg">{#if card.poster}<img src={card.poster} alt="" />{/if}</div>
                        <div class="nf-poster">{@render poster(card)}</div>
                        <div class="nf"><b>{t("Fragman bulunamadı")}</b>{t("Afişe ve künyeye bakarak karar ver")}</div>
                      {/if}
                      <!-- Sürükleme katmanı: iframe imleci yutmasın, kart sağa/sola çekilebilsin -->
                      <div class="grab" role="presentation" onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up}
                        onclick={() => { if (card?.trailer && Math.abs(dx) < 4) ytToggle(); }}></div>
                      {#if card.trailer}
                        <div class="tl"><span>▶ {t("Fragman")}</span></div>
                        <div class="tr">
                          <button class="snd" class:on={!tvMuted} onclick={ytMute}>{tvMuted ? t("🔇 Sesi aç") : t("🔊 Sesli")} <kbd>M</kbd></button>
                          <button onclick={() => (trailerKey = card!.trailer!)} title={t("Büyüt")}>⤢</button>
                        </div>
                        <div class="ctrl">
                          <button class="pp" onclick={ytToggle} title={t("Durdur / oynat (Boşluk)")}>{tvPaused ? "▶" : "❚❚"}</button>
                          <span class="t">{clock(tvAt)}</span>
                          <div class="bar"><i style="width:{tvLen ? (tvAt / tvLen) * 100 : 0}%"></i></div>
                          <span class="t">{tvLen ? clock(tvLen) : "–:––"}</span>
                        </div>
                      {/if}
                      {#if dx > 50}<span class="stamp like">{t("BEĞEN")}</span>{:else if dx < -50}<span class="stamp pass">{t("GEÇ")}</span>{/if}
                    </div>
                  {/key}
                </div>
                <div class="hint">{t("Kartı sağa çek beğen, sola çek geç")} · <kbd>←</kbd> <kbd>→</kbd>{#if card.trailer} · <kbd>{t("Boşluk")}</kbd> {t("durdur")} · <kbd>M</kbd> {t("ses")}{/if}</div>
              </div>
              {#key card.id}
                <div class="info" in:fly={{ x: 30, duration: 420, easing: cubicOut }}>
                  <div class="kick"><span>{t("Havuz · kart {a} / {b}", { a: votedN + 1, b: deck.items.length })}</span><span class="prog">{t("{a}/{b} bitirdi", { a: deck.done, b: deck.of })}</span></div>
                  <h2 class:long={card.title.length > 22}>{card.title}</h2>
                  {@render meta(card)}
                  {@render ratings(card)}
                  {#if card.overview}<p class="ov">{card.overview}</p>{/if}
                  {#if card.cast?.length}<div class="cast">{t("Oyuncular:")} {#each card.cast.slice(0, 3) as c, i}{i ? ", " : ""}<b>{c}</b>{/each}{#if card.director}&nbsp;· {tv ? t("Yaratıcı") : t("Yönetmen")}: <b>{card.director}</b>{/if}</div>{/if}
                  <div class="votes">
                    <div class="vlab"><button class="vb pass" onclick={() => cast(false)} title={t("Geç")}>✕</button>{t("Geç")} <kbd>←</kbd></div>
                    <div class="vlab"><button class="vb like" onclick={() => cast(true)} title={t("Beğen")}>♥</button>{t("Beğen")} <kbd>→</kbd></div>
                    {#if card.trailer}<div class="vote-note">{t("Fragman sessiz başlar. Oy verince sıradaki kartın fragmanı açılır.")}</div>{/if}
                  </div>
                </div>
              {/key}
            {:else}
              <div class="waiting" in:scale={{ start: 0.94, duration: 400 }}>
                <div class="spin"></div>
                <h2>{t("Oyların gitti")}</h2>
                <p>{t("Diğerleri bekleniyor · {a}/{b} bitirdi", { a: deck.done, b: deck.of })}</p>
              </div>
            {/if}
            <div class="strip">
              <span class="lbl">{t("Deste")}</span>
              {#each deck.items as m (m.id)}
                <div class="sp" class:liked={deck.mine[m.id] === true} class:passed={deck.mine[m.id] === false} class:cur={m.id === card?.id} class:next={!(m.id in deck.mine) && m.id !== card?.id}>
                  {@render poster(m)}</div>
              {/each}
            </div>
          </div>

        {:else if (phase === "done" || phase === "vote") && deck?.match}
          {@const m = deck.match}
          <div class="match">
            <div class="bd">{#if m.poster}<img src={m.poster} alt="" />{/if}</div><div class="bd2"></div>
            <div class="confetti" aria-hidden="true">{#each Array(28) as _, i}<i style="--i:{i}"></i>{/each}</div>
            <div class="mp" in:scale={{ start: 0.6, duration: 800, easing: backOut }}>{@render poster(m)}</div>
            <div class="m-info" in:fly={{ x: 40, delay: 200, duration: 600, easing: cubicOut }}>
              <span class="badge">✦ {t("EŞLEŞME · {a}/{b} beğendi", { a: deck.of, b: deck.of })}</span>
              <h2 class:long={m.title.length > 20}>{m.title}</h2>
              {@render meta(m)}
              <div class="voters">{#each n.members as x (x.id)}{@render av(x.name)}{/each}<span>{t("Hepiniz beğendiniz.")} {hour < 5 ? t("Bu gece bu.") : hour < 12 ? t("Bu sabah bu.") : hour < 18 ? t("Bugün bu.") : t("Bu akşam bu.")}</span></div>
              <div class="row big-row">
                <button class="big w" onclick={() => onFind(m)}>▶ {t("Hemen aç")}</button>
                {#if m.trailer}<button class="big g" onclick={() => (trailerKey = m.trailer!)}>{t("Fragman")}</button>{/if}
                {#if canControl}<button class="big g" onclick={() => (again = true)}>{t("Yeni tur")}</button>{/if}
              </div>
              <div class="fine left">{t("\"Hemen aç\" adını panoya kopyalar ve Video ekle'yi açar (Film & Dizi'de ara).")}</div>
            </div>
          </div>

        {:else if deck?.over}
          <div class="nomatch">
            <div class="nm-head">
              <h2>{t("Ortak bir {noun} çıkmadı", { noun })}</h2>
              <p>{t("En çok beğenilenler çarka gitti. Kader karar versin.")}</p>
              {#if canControl}<button class="ghost" onclick={() => (again = true)}>{t("Yeni tur")}</button>{/if}
            </div>
            <div class="wheel-wrap"><Wheel {canControl} {me} {onError} /></div>
          </div>
        {:else}
          <div class="waiting"><div class="spin"></div><h2>{t("Havuz hazırlanıyor…")}</h2></div>
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
{/if}


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
  .sh-head { display: flex; align-items: center; gap: 14px; padding: 0 18px 0 22px; border-bottom: 1px solid var(--line); position: relative; z-index: 3; min-width: 0; }
  .sh-head h1 { margin: 0; font-size: 20px; font-weight: 800; letter-spacing: -0.02em; display: flex; align-items: center; gap: 10px; white-space: nowrap; }
  .sh-head h1 b, .hm-head > b { width: 30px; height: 30px; border-radius: 9px; display: grid; place-items: center; background: linear-gradient(135deg, #0a84ff, #5e5ce6); font-size: 15px; }
  .steps { display: flex; gap: 4px; margin-left: 10px; }
  .steps span { display: flex; align-items: center; gap: 7px; padding: 5px 12px 5px 6px; border-radius: 99px; font-size: 12px; font-weight: 600; color: var(--text-3); transition: background 0.3s, color 0.3s; }
  .steps span i { width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; font-style: normal; font-size: 11px; background: var(--glass-2); transition: background 0.3s; }
  .steps span.on { color: #fff; background: var(--glass-2); }
  .steps span.on i { background: var(--accent); }
  .steps span.done i { background: var(--green); color: #000; }
  .who { margin-left: auto; display: flex; align-items: center; color: var(--text-2); font-size: 12px; min-width: 0; padding-left: 8px; }
  .who .av { width: 26px; height: 26px; font-size: 11px; margin-left: -8px; border: 2px solid #1a1b1f; }
  .who .kl { margin-left: 8px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .x { flex: none; width: 32px; height: 32px; border-radius: 10px; border: 1px solid var(--line-2); background: var(--glass); display: grid; place-items: center; color: var(--text-2); padding: 0; }
  .x:hover { color: #fff; background: var(--glass-2); }
  kbd { font: 500 10.5px var(--font-mono); padding: 1px 5px; border-radius: 5px; border: 1px solid var(--line-2); color: var(--text-2); }
  .av { flex: none; width: 30px; height: 30px; border-radius: 50%; display: grid; place-items: center; font-weight: 700; font-size: 12px; color: #fff; background: hsl(var(--h) 55% 45%); }

  .body { min-height: 0; position: relative; display: grid; }
  .two-col { display: grid; grid-template-columns: 1fr 300px; min-height: 0; }
  .main { padding: 22px 26px; display: flex; flex-direction: column; gap: 16px; min-width: 0; overflow-y: auto; }
  .side { border-left: 1px solid var(--line); padding: 22px; display: flex; flex-direction: column; gap: 12px; background: rgba(0, 0, 0, 0.18); min-height: 0; overflow-y: auto; }
  .side h4 { margin: 0; font-size: 11.5px; color: var(--text-2); text-transform: uppercase; letter-spacing: 0.06em; }
  .q { font-size: 30px; font-weight: 850; letter-spacing: -0.03em; margin: 0; line-height: 1.15; }
  .q span { color: var(--text-2); font-weight: 600; font-size: 15px; letter-spacing: 0; margin-left: 10px; }
  .row { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .lock { display: inline-flex; align-items: center; gap: 8px; padding: 8px 14px; border-radius: 12px; background: #fff; color: #000; font-weight: 700; }
  .lock em { font-style: normal; font-weight: 600; font-size: 12px; color: #555; }
  .soft { color: var(--text-3); }

  /* Poster */
  .poster { position: relative; aspect-ratio: 2 / 3; border-radius: 12px; overflow: hidden; background: var(--p, #1a1b1f); box-shadow: 0 18px 40px rgba(0, 0, 0, 0.55), 0 0 0 1px rgba(255, 255, 255, 0.07); display: flex; flex-direction: column; justify-content: flex-end; padding: 10% 9%; }
  .poster img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .poster::after { content: ""; position: absolute; inset: 0; pointer-events: none; background: linear-gradient(160deg, rgba(255, 255, 255, 0.14), transparent 35%); }
  .poster b { position: relative; z-index: 1; font-weight: 900; letter-spacing: -0.02em; line-height: 1; font-size: 15px; }
  .pbtn { display: block; width: 100%; padding: 0; border: 0; background: none; text-align: left; }

  /* 1 · Kur */
  .moods { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
  .mood { position: relative; height: 128px; border-radius: 16px; overflow: hidden; padding: 14px 16px; display: flex; flex-direction: column; justify-content: flex-end; text-align: left; background: var(--m); border: 1px solid rgba(255, 255, 255, 0.08); transition: transform 0.25s cubic-bezier(0.34, 1.4, 0.5, 1), outline-color 0.2s; outline: 2px solid transparent; outline-offset: 2px; }
  .mood::after { content: ""; position: absolute; inset: 0; background: linear-gradient(90deg, rgba(0, 0, 0, 0.7) 30%, transparent 78%); }
  .mood:hover { transform: translateY(-2px); }
  .mood:hover .fan .poster:nth-child(1) { transform: rotate(-14deg) translateX(-6px); }
  .mood:hover .fan .poster:nth-child(3) { transform: rotate(12deg) translateX(6px); }
  .moods.locked .mood:not(.on) { opacity: 0.45; }
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
  .ready { display: flex; flex-direction: column; gap: 8px; }
  .ready > div { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: 12px; background: var(--glass); }
  .ready > div > span:not(.av) { display: flex; flex-direction: column; line-height: 1.2; min-width: 0; font-weight: 600; }
  .ready small { color: var(--text-3); font-size: 11px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ready em { margin-left: auto; font-style: normal; font-size: 11.5px; color: var(--green); font-weight: 600; white-space: nowrap; }
  .ready em.wait { color: var(--text-3); }
  .sum { margin-top: auto; padding: 14px; border-radius: 14px; background: linear-gradient(135deg, rgba(10, 132, 255, 0.18), rgba(94, 92, 230, 0.12)); border: 1px solid rgba(10, 132, 255, 0.3); font-size: 12.5px; color: rgba(255, 255, 255, 0.85); }
  .sum > b:first-child { display: block; font-size: 15px; color: #fff; margin-bottom: 4px; }
  .sum .in { color: #fff; }
  .cta { flex: none; height: 52px; border-radius: 16px; border: 0; background: #fff; color: #000; font-weight: 800; font-size: 15px; display: flex; align-items: center; justify-content: center; gap: 10px; box-shadow: 0 12px 30px -8px rgba(10, 132, 255, 0.6); transition: transform 0.15s, background 0.25s, color 0.25s; }
  .cta:hover:not(:disabled) { transform: translateY(-1px); }
  .cta:active:not(:disabled) { transform: scale(0.98); }
  .cta:disabled { opacity: 0.45; }
  .cta.on { background: var(--green); color: #00170a; box-shadow: 0 12px 30px -8px rgba(48, 209, 88, 0.6); }
  .cta .cnt { font-size: 12px; font-weight: 700; padding: 3px 9px; border-radius: 99px; background: rgba(0, 0, 0, 0.08); }
  .cta.sm { height: 38px; padding: 0 18px; border-radius: 12px; font-size: 13px; }
  .fine { color: var(--text-3); font-size: 11.5px; text-align: center; }
  .fine.left { text-align: left; }
  .ghost { height: 38px; padding: 0 16px; border-radius: 12px; background: var(--glass-2); border: 1px solid var(--line-2); display: inline-flex; align-items: center; gap: 8px; font-weight: 600; }
  .ghost:hover { background: var(--glass-3); }
  .ghost.sm { height: 32px; padding: 0 12px; font-size: 12px; }

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
  .sw { display: flex; align-items: center; gap: 12px; border: 0; background: none; padding: 0; text-align: left; }
  .sw span { flex: 1; display: flex; flex-direction: column; }
  .sw small { color: var(--text-3); font-size: 11.5px; }
  .sw i { flex: none; width: 44px; height: 26px; border-radius: 99px; background: rgba(120, 120, 128, 0.36); position: relative; transition: background 0.2s; }
  .sw i::after { content: ""; position: absolute; top: 3px; left: 3px; width: 20px; height: 20px; border-radius: 50%; background: #fff; transition: left 0.2s cubic-bezier(0.34, 1.4, 0.5, 1); }
  .sw i.on { background: var(--green); }
  .sw i.on::after { left: 21px; }
  .fp-foot { display: flex; align-items: center; gap: 10px; padding: 14px 20px; border-top: 1px solid var(--line); }
  .fp-foot > span { margin-right: auto; color: var(--text-2); }
  .fp-foot > span b { color: #fff; font-size: 16px; }

  /* 2 · Öner */
  .p-top { display: flex; flex-direction: column; gap: 12px; position: relative; z-index: 3; }
  .search { position: relative; display: flex; align-items: center; gap: 10px; height: 46px; padding: 0 16px; border-radius: 14px; background: rgba(255, 255, 255, 0.07); border: 1px solid rgba(255, 255, 255, 0.2); font-size: 14px; color: var(--text-2); }
  .search:focus-within { border-color: rgba(10, 132, 255, 0.8); box-shadow: 0 0 0 3px rgba(10, 132, 255, 0.2); }
  .search input, .search input:focus-visible { flex: 1; min-width: 0; border: 0; border-radius: 0; background: none; outline: 0; box-shadow: none; color: #fff; font: inherit; height: 100%; padding: 0; }
  .drop { position: absolute; z-index: 4; top: 52px; left: 0; right: 0; padding: 6px; border-radius: 14px; background: #202126; border: 1px solid var(--line-2); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6); display: flex; flex-direction: column; gap: 2px; max-height: 360px; overflow-y: auto; }
  .dr { display: flex; align-items: center; gap: 12px; padding: 4px 8px 4px 4px; border-radius: 10px; }
  .dr:hover { background: rgba(255, 255, 255, 0.07); }
  .dr-open { flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; border: 0; background: none; padding: 2px; text-align: left; }
  .dr .poster { width: 34px; flex: none; padding: 3px; border-radius: 5px; box-shadow: none; }
  .dr .poster b { font-size: 6px; }
  .dr-open > span { display: flex; flex-direction: column; color: #fff; min-width: 0; }
  .dr small { color: var(--text-2); font-size: 11.5px; }
  .add { flex: none; height: 30px; padding: 0 12px; border-radius: 9px; border: 0; background: #fff; color: #000; font-weight: 700; }
  .add.on { background: var(--green); }
  .add:disabled { opacity: 0.4; }
  .lab2 { display: flex; justify-content: space-between; gap: 10px; color: var(--text-2); font-size: 12px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.06em; }
  .lab2 > span { text-transform: none; letter-spacing: 0; color: var(--text-3); font-weight: 500; }
  .link { background: none; border: 0; padding: 0; color: var(--accent); font-weight: 650; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(132px, 1fr)); gap: 16px 12px; }
  .pc { position: relative; display: flex; flex-direction: column; gap: 6px; }
  .pc .poster { padding: 9%; border-radius: 10px; transition: transform 0.25s cubic-bezier(0.34, 1.4, 0.5, 1), box-shadow 0.25s; }
  .pc .pbtn:hover .poster { transform: translateY(-4px); }
  .pc > span { font-weight: 650; font-size: 12.5px; display: flex; flex-direction: column; line-height: 1.25; }
  .pc small { color: var(--text-3); font-weight: 500; font-size: 11px; }
  .pc.sel .poster { box-shadow: 0 0 0 3px #fff, 0 18px 40px rgba(0, 0, 0, 0.55); }
  .pick-t { position: absolute; top: 8px; right: 8px; width: 28px; height: 28px; border-radius: 50%; border: 0; padding: 0; background: rgba(0, 0, 0, 0.6); color: #fff; display: grid; place-items: center; font-weight: 900; font-size: 15px; z-index: 2; backdrop-filter: blur(8px); opacity: 0; transition: opacity 0.2s, transform 0.2s; }
  .pc:hover .pick-t, .pick-t.on { opacity: 1; }
  .pick-t.on { background: #fff; color: #000; }
  .pick-t:hover:not(:disabled) { transform: scale(1.12); }
  .pick-t:disabled { opacity: 0 !important; }
  .skel { background: linear-gradient(100deg, #1c1d21 30%, #26272c 50%, #1c1d21 70%) 0 0 / 300% 100%; animation: sk 1.3s linear infinite; box-shadow: none; }
  .skel-t { height: 12px; width: 70%; border-radius: 4px; background: #1c1d21; }
  @keyframes sk { to { background-position: -150% 0; } }
  .banner { padding: 10px 12px; border-radius: 10px; background: rgba(255, 69, 58, 0.12); color: #ff8a80; font-size: 12.5px; }
  .slots { display: flex; flex-direction: column; gap: 8px; }
  .slot { display: flex; align-items: center; gap: 10px; padding: 8px; border-radius: 12px; background: var(--glass); border: 1px solid var(--line); }
  .slot .poster { width: 34px; flex: none; padding: 3px; border-radius: 5px; box-shadow: none; }
  .slot .poster b { font-size: 6px; }
  .slot span { display: flex; flex-direction: column; line-height: 1.2; min-width: 0; }
  .slot span b { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .slot small { color: var(--text-3); font-size: 11px; }
  .xs { margin-left: auto; border: 0; background: none; color: var(--text-3); padding: 4px 6px; }
  .xs:hover { color: #fff; }
  .slot.empty { justify-content: center; height: 54px; border-style: dashed; color: var(--text-3); font-size: 12px; background: transparent; }
  .pool { padding: 12px 14px; border-radius: 14px; background: var(--glass); border: 1px solid var(--line); display: flex; flex-direction: column; gap: 8px; color: var(--text-2); }
  .pool b { color: #fff; font-size: 22px; margin-right: 4px; }
  .who2 { display: flex; align-items: center; gap: 4px; font-size: 12px; }
  .who2 .av { width: 22px; height: 22px; font-size: 10px; }
  .who2 em { font-style: normal; margin-left: 6px; color: var(--green); }
  .who2 em.wait { color: var(--text-3); }
  .side .cta { margin-top: auto; }

  /* Künye */
  .fl { display: flex; gap: 10px; overflow-x: auto; padding-bottom: 4px; flex: none; }
  .fli { flex: none; width: 76px; height: auto; padding: 0; background: none; border: 0; display: flex; flex-direction: column; gap: 4px; text-align: left; }
  .fli:hover { background: none; }
  .fli:disabled { opacity: 1; }
  .flp { position: relative; display: block; width: 76px; aspect-ratio: 2 / 3; border-radius: 8px; overflow: hidden; background: var(--p); border: 1px solid rgba(255, 255, 255, 0.1); }
  .flp img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .flp em { position: absolute; right: 4px; top: 4px; padding: 1px 5px; border-radius: 99px; background: #30d158; color: #00170a; font: normal 800 8.5px var(--font); }
  .fli.on .flp { box-shadow: 0 0 0 2px #30d158; }
  .flt { font-size: 11px; color: rgba(255, 255, 255, 0.75); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .detail { position: relative; display: grid; grid-template-rows: auto 1fr auto; gap: 14px; padding: 16px 26px 18px; min-height: 0; overflow-y: auto; background: radial-gradient(70% 60% at 25% 20%, rgba(10, 132, 255, 0.12), transparent 70%); }
  .d-back { justify-self: start; border: 0; background: none; padding: 4px 0; color: var(--text-2); font-weight: 600; }
  .d-back:hover { color: #fff; }
  .d-grid { display: grid; grid-template-columns: 1.15fr 1fr; gap: 28px; min-height: 0; }
  .trailer { position: relative; aspect-ratio: 16 / 9; align-self: start; border-radius: 16px; overflow: hidden; background: #000; box-shadow: 0 30px 70px rgba(0, 0, 0, 0.6), 0 0 0 1px rgba(255, 255, 255, 0.08); }
  .trailer iframe { position: absolute; inset: 0; width: 100%; height: 100%; border: 0; }
  .tr-img { position: absolute; inset: 0; background: var(--p); }
  .tr-img img { position: absolute; inset: -10%; width: 120%; height: 120%; object-fit: cover; filter: blur(24px) brightness(0.6) saturate(1.3); }
  .tr-play { position: absolute; left: 50%; top: 50%; transform: translate(-50%, -50%); width: 74px; height: 74px; border-radius: 50%; border: 0; background: rgba(255, 255, 255, 0.92); color: #000; display: grid; place-items: center; font-size: 26px; padding-left: 5px; box-shadow: 0 10px 40px rgba(0, 0, 0, 0.6); transition: transform 0.2s cubic-bezier(0.34, 1.4, 0.5, 1); }
  .tr-play:hover { transform: translate(-50%, -50%) scale(1.08); }
  .tr-cap { position: absolute; left: 16px; bottom: 12px; font-size: 12px; color: rgba(255, 255, 255, 0.85); }
  .d-info { display: flex; flex-direction: column; gap: 11px; min-width: 0; }
  .d-kick, .kick { color: var(--text-2); font-size: 12px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.08em; }
  .d-info h2 { margin: 0; font-size: 42px; font-weight: 900; letter-spacing: -0.035em; line-height: 1.08; padding-bottom: 0.06em; }
  .orig { color: var(--text-2); font-style: italic; margin-top: -8px; }
  .meta { display: flex; gap: 10px; align-items: center; color: rgba(255, 255, 255, 0.8); font-size: 14px; flex-wrap: wrap; }
  .meta i { width: 4px; height: 4px; border-radius: 50%; background: var(--text-3); }
  .genre { padding: 3px 10px; border-radius: 99px; border: 1px solid var(--line-2); font-size: 12px; color: var(--text-2); }
  .ratings { display: flex; gap: 10px; flex-wrap: wrap; }
  .rt { display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-radius: 12px; background: var(--glass); border: 1px solid var(--line); }
  .rt.none { opacity: 0.4; }
  .rt > div { display: flex; flex-direction: column; line-height: 1.1; }
  .rt b { font-size: 18px; font-weight: 800; }
  .rt small { color: var(--text-2); font-size: 11px; }
  .rt .lg { width: 30px; height: 26px; border-radius: 6px; display: grid; place-items: center; font-weight: 900; font-size: 9.5px; color: #000; }
  .ov { color: rgba(255, 255, 255, 0.78); font-size: 14px; line-height: 1.55; max-width: 620px; margin: 0; display: -webkit-box; -webkit-line-clamp: 5; line-clamp: 5; -webkit-box-orient: vertical; overflow: hidden; }
  .kunye { display: grid; grid-template-columns: 92px 1fr; gap: 6px 12px; margin: 0; font-size: 12.5px; }
  .kunye dt { color: var(--text-3); }
  .kunye dd { margin: 0; color: rgba(255, 255, 255, 0.88); }
  .cta.wide { padding: 0 18px; }
  .d-more { display: flex; align-items: center; gap: 10px; }
  .d-more > span { color: var(--text-2); font-size: 12px; font-weight: 650; margin-right: 6px; }
  .d-more .pbtn { width: 56px; }
  .d-more .poster { padding: 5px; border-radius: 7px; transition: transform 0.2s; }
  .d-more .pbtn:hover .poster { transform: translateY(-3px); }
  .d-more .poster b { font-size: 6px; }

  /* 3 · Oyla */
  .deck { position: relative; display: grid; grid-template-columns: minmax(0, 1.45fr) minmax(320px, 1fr); gap: 34px; padding: 22px 34px 100px; min-height: 0; overflow: hidden; }
  .tstack { position: relative; min-width: 0; }
  .tcard { position: relative; }
  .tcard .under { position: absolute; inset: 0; border-radius: 18px; background: #1a1d22; }
  .tcard .under.u2 { transform: translate(18px, 16px) rotate(1.2deg) scale(0.96); filter: brightness(0.55); }
  .tcard .under.u1 { transform: translate(9px, 8px) scale(0.98); filter: brightness(0.75); }
  .tv { position: relative; z-index: 2; width: 100%; max-height: calc(100vh - 330px); aspect-ratio: 16 / 9; border-radius: 18px; overflow: hidden; background: #050607;
    box-shadow: 0 30px 70px rgba(0, 0, 0, 0.65), 0 0 0 1px rgba(255, 255, 255, 0.1); }
  .tv iframe { position: absolute; inset: 0; width: 100%; height: 100%; border: 0; pointer-events: none; }
  .tv .grab { position: absolute; inset: 0; z-index: 1; touch-action: none; cursor: grab; }
  .tv .grab:active { cursor: grabbing; }
  .tv .tl, .tv .tr { position: absolute; top: 14px; z-index: 2; display: flex; gap: 6px; }
  .tv .tl { left: 14px; pointer-events: none; }
  .tv .tr { right: 14px; }
  .tv .tl span, .tv .tr button { padding: 5px 10px; border-radius: 99px; background: rgba(0, 0, 0, 0.55); backdrop-filter: blur(10px); border: 1px solid rgba(255, 255, 255, 0.14); font-size: 11.5px; font-weight: 650; color: #fff; display: flex; gap: 6px; align-items: center; }
  .tv .tr button.snd { background: #fff; color: #000; border-color: #fff; }
  .tv .tr button.snd.on { background: rgba(0, 0, 0, 0.55); color: #fff; border-color: rgba(255, 255, 255, 0.14); }
  .tv .tr button.snd kbd { color: inherit; opacity: 0.6; border-color: currentColor; }
  .tv .ctrl { position: absolute; z-index: 2; left: 0; right: 0; bottom: 0; height: 52px; padding: 0 16px; display: flex; align-items: center; gap: 12px; background: linear-gradient(0deg, rgba(0, 0, 0, 0.75), transparent); }
  .tv .ctrl .pp { width: 30px; height: 30px; border-radius: 50%; border: 0; padding: 0; background: rgba(255, 255, 255, 0.16); color: #fff; display: grid; place-items: center; font-size: 11px; }
  .tv .ctrl .t { font-family: var(--mono, monospace); font-size: 11px; color: rgba(255, 255, 255, 0.85); white-space: nowrap; }
  .tv .bar { position: relative; flex: 1; height: 4px; border-radius: 4px; background: rgba(255, 255, 255, 0.2); overflow: hidden; }
  .tv .bar i { position: absolute; left: 0; top: 0; bottom: 0; border-radius: 4px; background: #fff; transition: width 0.25s linear; }
  .tv .nf-bg { position: absolute; inset: 0; }
  .tv .nf-bg img { position: absolute; inset: -10%; width: 120%; height: 120%; object-fit: cover; filter: blur(40px) saturate(1.3) brightness(0.6); }
  .tv .nf-poster { position: absolute; left: 6%; top: 10%; bottom: 10%; aspect-ratio: 2 / 3; }
  .tv .nf-poster .poster { height: 100%; }
  .tv .nf { position: absolute; right: 8%; top: 50%; transform: translateY(-50%); max-width: 44%; padding: 12px 16px; border-radius: 14px; background: rgba(0, 0, 0, 0.55); backdrop-filter: blur(10px); border: 1px solid rgba(255, 255, 255, 0.14); font-size: 12.5px; color: rgba(255, 255, 255, 0.85); }
  .tv .nf b { display: block; font-size: 14px; color: #fff; margin-bottom: 2px; }
  .tv .stamp { z-index: 3; }
  .tstack .hint { margin-top: 22px; text-align: center; color: var(--text-3); font-size: 12px; }
  .vote-note { color: var(--text-3); font-size: 11.5px; max-width: 200px; line-height: 1.45; }
  .amb { position: absolute; inset: 0; pointer-events: none; background: var(--p); }
  .amb img { position: absolute; inset: -10%; width: 120%; height: 120%; object-fit: cover; filter: blur(80px) saturate(1.5); opacity: 0.4; }
  .amb::after { content: ""; position: absolute; inset: 0; background: linear-gradient(90deg, rgba(16, 17, 20, 0.2), rgba(16, 17, 20, 0.9) 70%); }
  .stamp { position: absolute; z-index: 5; left: 26px; top: 40px; transform: rotate(-16deg); border: 3px solid; font-weight: 900; font-size: 26px; padding: 2px 12px; border-radius: 8px; letter-spacing: 0.06em; background: rgba(0, 0, 0, 0.35); }
  .stamp.like { border-color: var(--green); color: var(--green); }
  .stamp.pass { border-color: #ff6961; color: #ff6961; left: auto; right: 26px; transform: rotate(16deg); }
  .info { position: relative; display: flex; flex-direction: column; gap: 12px; padding-top: 6px; min-width: 0; }
  .info .kick { display: flex; gap: 14px; align-items: center; }
  .prog { color: var(--green); }
  .info h2 { margin: 0; font-size: 50px; font-weight: 900; letter-spacing: -0.035em; line-height: 1.06; padding-bottom: 0.06em; }
  .info h2.long, .m-info h2.long { font-size: 36px; }
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
  .waiting { grid-column: 1 / -1; align-self: center; justify-self: center; display: flex; flex-direction: column; align-items: center; gap: 8px; text-align: center; position: relative; }
  .waiting h2 { margin: 6px 0 0; font-size: 30px; font-weight: 850; }
  .waiting p { margin: 0; color: var(--text-2); }
  .spin { width: 46px; height: 46px; border-radius: 50%; border: 3px solid rgba(255, 255, 255, 0.12); border-top-color: var(--accent); animation: rot 0.9s linear infinite; }
  @keyframes rot { to { transform: rotate(360deg); } }

  /* 4 · Eşleşme */
  .match { position: relative; display: grid; grid-template-columns: 330px 1fr; gap: 48px; align-items: center; padding: 0 70px; overflow: hidden; }
  .match .bd { position: absolute; inset: -40px; overflow: hidden; }
  .match .bd img { width: 100%; height: 100%; object-fit: cover; filter: blur(60px) saturate(1.4); opacity: 0.55; }
  .match .bd2 { position: absolute; inset: 0; background: linear-gradient(90deg, rgba(10, 10, 12, 0.2), rgba(10, 10, 12, 0.85) 60%); }
  .mp { position: relative; width: 330px; transform: rotate(-3deg); }
  .mp .poster { box-shadow: 0 40px 90px rgba(0, 0, 0, 0.7), 0 0 0 1px rgba(255, 255, 255, 0.1), 0 0 80px -10px rgba(255, 140, 60, 0.6); }
  .mp .poster b { font-size: 40px; }
  .m-info { position: relative; display: flex; flex-direction: column; gap: 14px; min-width: 0; }
  .badge { align-self: flex-start; padding: 6px 14px; border-radius: 99px; background: linear-gradient(135deg, #30d158, #0fb5ae); color: #00170a; font-weight: 800; font-size: 12.5px; letter-spacing: 0.02em; }
  .m-info h2 { margin: 0; font-size: 60px; font-weight: 900; letter-spacing: -0.04em; line-height: 1.04; padding-bottom: 0.06em; }
  .voters { display: flex; align-items: center; gap: 10px; color: rgba(255, 255, 255, 0.8); font-size: 14px; }
  .voters .av { margin-left: -8px; border: 2px solid #15161a; }
  .voters .av:first-child { margin-left: 0; }
  .big-row { margin-top: 8px; }
  .big { height: 54px; padding: 0 24px; white-space: nowrap; border-radius: 16px; border: 0; font-weight: 800; font-size: 15px; display: flex; align-items: center; gap: 10px; transition: transform 0.15s; }
  .big:hover { transform: translateY(-1px); }
  .big.w { background: #fff; color: #000; box-shadow: 0 14px 34px -10px rgba(255, 255, 255, 0.4); }
  .big.g { background: var(--glass-2); border: 1px solid var(--line-2); }
  .confetti { position: absolute; inset: 0; pointer-events: none; overflow: hidden; z-index: 3; }
  .confetti i {
    position: absolute; top: -20px; left: calc(var(--i) * 3.6%); width: 8px; height: 14px; border-radius: 2px;
    background: hsl(calc(var(--i) * 47) 90% 60%); animation: fall 2.6s cubic-bezier(0.3, 0.6, 0.5, 1) calc(var(--i) * 45ms) 1 forwards;
  }
  @keyframes fall { to { transform: translateY(760px) rotate(720deg); opacity: 0; } }
  .nomatch { display: grid; grid-template-columns: 1fr 1fr; gap: 40px; align-items: center; padding: 30px 60px; }
  .nm-head h2 { margin: 0; font-size: 42px; font-weight: 900; letter-spacing: -0.03em; }
  .nm-head p { color: var(--text-2); font-size: 15px; }
  .wheel-wrap { max-width: 360px; }

  /* Fragman katmanı */
  .tr-over { position: absolute; inset: 64px 0 0; z-index: 8; background: rgba(0, 0, 0, 0.75); display: grid; place-items: center; backdrop-filter: blur(6px); }
  .tr-box { width: min(86%, 1000px); aspect-ratio: 16 / 9; border-radius: 16px; overflow: hidden; box-shadow: 0 40px 120px rgba(0, 0, 0, 0.8); }
  .tr-box iframe { width: 100%; height: 100%; border: 0; }

  /* Host başlatma */
  .hostm { position: fixed; z-index: 42; inset: var(--top-bar) 64px 0 0; display: grid; place-items: center; }
  .hm-card { width: 460px; padding: 24px; border-radius: 22px; background: #1a1b20; border: 1px solid var(--line-2); box-shadow: 0 40px 120px rgba(0, 0, 0, 0.7); display: flex; flex-direction: column; gap: 12px; }
  .hm-head { display: flex; gap: 12px; align-items: center; margin-bottom: 6px; }
  .hm-head > b { width: 40px; height: 40px; border-radius: 12px; font-size: 18px; }
  .hm-head > div { flex: 1; }
  .hm-head h3 { margin: 0; font-size: 19px; font-weight: 800; }
  .hm-head small { color: var(--text-2); }
  .hm-l { color: var(--text-2); font-size: 11.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; margin-top: 4px; }
  .kinds { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .k { padding: 16px; border-radius: 16px; background: rgba(255, 255, 255, 0.05); border: 1px solid var(--line); display: flex; flex-direction: column; gap: 2px; text-align: left; transition: background 0.2s, border-color 0.2s, box-shadow 0.2s; }
  .k span { font-size: 26px; }
  .k b { font-size: 16px; }
  .k small { color: var(--text-2); }
  .k.on { background: rgba(10, 132, 255, 0.16); border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent), 0 12px 30px -10px rgba(10, 132, 255, 0.6); }
  .nums { display: flex; gap: 8px; }
  .nums button { flex: 1; height: 44px; border-radius: 12px; display: grid; place-items: center; background: rgba(255, 255, 255, 0.05); border: 1px solid var(--line); font-weight: 800; font-size: 16px; color: var(--text-2); transition: background 0.2s, color 0.2s; }
  .nums button.on { background: #fff; color: #000; }
  .hm-note { color: var(--text-2); font-size: 12.5px; margin: 0; }
  .hm-note b { color: #fff; }
  .hm-card .cta { margin-top: 6px; }

  /* Açılış */
  .intro { position: fixed; z-index: 42; inset: var(--top-bar) 64px 0 0; display: grid; place-items: center; overflow: hidden; background: radial-gradient(60% 60% at 50% 50%, #10234a, #050608 75%); }
  .rays { position: absolute; inset: -50%; background: repeating-conic-gradient(from 0deg, rgba(10, 132, 255, 0.1) 0deg 6deg, transparent 6deg 18deg); animation: rot 30s linear infinite; mask-image: radial-gradient(closest-side, #000, transparent); }
  .ib { position: relative; display: flex; flex-direction: column; align-items: center; gap: 14px; text-align: center; }
  .i-k { padding: 6px 14px; border-radius: 99px; background: rgba(255, 255, 255, 0.08); border: 1px solid var(--line-2); color: rgba(255, 255, 255, 0.85); font-size: 12.5px; font-weight: 600; }
  .ib h2 { margin: 0; font-size: 72px; font-weight: 900; letter-spacing: -0.045em; background: linear-gradient(180deg, #fff, #9cc8ff); -webkit-background-clip: text; background-clip: text; color: transparent; padding-bottom: 0.08em; }
  .count { position: relative; width: 150px; height: 150px; display: grid; place-items: center; }
  .count svg { position: absolute; inset: 0; transform: rotate(-90deg); }
  .count circle { fill: none; stroke-width: 6; }
  .count .bg { stroke: rgba(255, 255, 255, 0.1); }
  .count .fg { stroke: #fff; stroke-linecap: round; stroke-dasharray: 339; transition: stroke-dashoffset 0.9s linear; filter: drop-shadow(0 0 10px #0a84ff); }
  .count b { font-size: 84px; font-weight: 900; letter-spacing: -0.05em; text-shadow: 0 0 40px rgba(10, 132, 255, 0.8); }
  .i-av { display: flex; gap: 8px; }
  .i-av .av { width: 40px; height: 40px; font-size: 15px; box-shadow: 0 0 0 3px #050608, 0 0 20px rgba(10, 132, 255, 0.6); }
  .i-sub { color: var(--text-2); font-size: 14px; }
</style>
