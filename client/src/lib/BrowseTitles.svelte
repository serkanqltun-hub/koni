<script lang="ts" module>
  import type { BrowseQuery, CatalogTitle } from "./player";
  type Kind = "movie" | "series";
  type Sort = NonNullable<BrowseQuery["sort"]>;
  const YEAR = new Date().getFullYear();
  const FIRST = 1950;
  /** Detaydan dönünce filtreler ve sonuçlar yerinde kalsın. */
  const s = $state({
    kind: "movie" as Kind, genre: 0, lo: FIRST, hi: YEAR, minRating: 0, maxMin: 0, lang: "", sort: "popular" as Sort,
    grid: false, items: [] as CatalogTitle[], page: 0, total: 0, key: "",
  });
  /** Raflar (tür/sıra/dil başına ilk sayfa), aynı oturumda bir kez. */
  const shelfCache = new Map<string, CatalogTitle[]>();
</script>

<script lang="ts">
  // Film & Dizi → "Göz at" (p0.26 `browse`): tür, yıl, puan, süre ve dile göre başlıklar.
  // Filtre yokken raflar, filtre seçilince 30'luk ızgara ve "Daha fazla". Kart detay sayfasını açar.
  import { untrack, type Snippet } from "svelte";
  import { browseGet, type Movie } from "./player";
  import { social } from "./social.svelte";
  import DualRange from "./DualRange.svelte";
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";

  let { card, onError }: {
    /** TitlesPane'in afiş kartı (tıklayınca detay). */
    card: Snippet<[CatalogTitle]>;
    onError?: (msg: string) => void;
  } = $props();

  const GENRES: [number, string][] = [
    [28, "Aksiyon"], [35, "Komedi"], [18, "Dram"], [53, "Gerilim"], [80, "Suç"], [878, "Bilimkurgu"], [27, "Korku"], [10749, "Romantik"],
    [9648, "Gizem"], [16, "Animasyon"], [12, "Macera"], [14, "Fantastik"], [10751, "Aile"], [99, "Belgesel"], [10752, "Savaş"], [37, "Western"],
  ];
  const LANGS: [string, string][] = [["tr", "Türkçe"], ["en", "İngilizce"], ["ko", "Korece"], ["ja", "Japonca"], ["fr", "Fransızca"], ["es", "İspanyolca"], ["it", "İtalyanca"], ["de", "Almanca"], ["hi", "Hintçe"]];
  const SORTS: [Sort, string][] = [["popular", "Popüler"], ["rating", "En yüksek puan"], ["new", "En yeni"]];
  /** Raflar: ilk sayfanın ilk 15'i; "Tümü" o sırayla ızgarayı açar. */
  const SHELVES: { id: string; title: string; sort: Sort; lang?: string }[] = [
    { id: "pop", title: "Bu hafta popüler", sort: "popular" },
    { id: "new", title: "Yeni çıkanlar", sort: "new" },
    { id: "tr", title: "Türk yapımları", sort: "popular", lang: "tr" },
  ];

  const toTitle = (m: Movie, kind: Kind): CatalogTitle => ({
    id: m.imdb || `tmdb:${kind === "series" ? "tv" : "movie"}:${m.id}`, type: kind, name: m.title, poster: m.poster,
    releaseInfo: m.year ? String(m.year) : undefined, imdbRating: m.ratings?.imdb ?? null,
  });
  const query = (sort: Sort, page: number, lang = s.lang): BrowseQuery => ({
    genre: s.genre || undefined, yearFrom: s.lo > FIRST ? s.lo : undefined, yearTo: s.hi < YEAR ? s.hi : undefined,
    minRating: s.minRating || undefined, maxMin: s.maxMin || undefined, lang: lang || undefined, sort, page,
  });
  let err = $state("");
  async function fetchPage(sort: Sort, page: number, lang?: string) {
    const url = social.server;
    if (!url) throw new Error(t("Göz at için bir sunucuya giriş yap"));
    const r = await browseGet(url, s.kind, query(sort, page, lang)).catch(() => {
      throw new Error(t("Bu sunucu Göz at'ı desteklemiyor ya da cevap vermedi"));
    });
    if (!r) throw new Error(t("Göz at için bir sunucuya giriş yap"));
    if (r.error) throw new Error(r.error);
    return r;
  }

  // Raflar: filtre yokken; tür/dizi değişince yeniden.
  let shelves = $state<Record<string, CatalogTitle[]>>({});
  $effect(() => {
    if (s.grid) return;
    const kind = s.kind;
    let live = true;
    err = "";
    for (const sh of SHELVES) {
      const k = `${kind}:${sh.id}`;
      const hit = shelfCache.get(k);
      if (hit) { shelves[sh.id] = hit; continue; }
      delete shelves[sh.id];
      fetchPage(sh.sort, 1, sh.lang).then((r) => {
        const list = r.items.slice(0, 15).map((m) => toTitle(m, kind));
        shelfCache.set(k, list);
        if (live) shelves[sh.id] = list;
      }, (e) => { if (live) err = String(e.message ?? e); });
    }
    return () => { live = false; };
  });

  // Izgara: filtre ya da sıra değişince ilk sayfa (kaydırıcı için kısa bekleme).
  let loading = $state(false);
  const key = $derived(JSON.stringify([s.kind, query(s.sort, 1)]));
  $effect(() => {
    if (!s.grid) return;
    const k = key;
    if (k === untrack(() => s.key)) return; // detaydan dönüş: elde olan kalsın
    let live = true;
    const timer = setTimeout(() => {
      loading = true;
      err = "";
      fetchPage(s.sort, 1).then((r) => {
        if (!live) return;
        s.items = r.items.map((m) => toTitle(m, s.kind));
        s.page = 1; s.total = r.total; s.key = k;
      }, (e) => { if (live) err = String(e.message ?? e); }).finally(() => { if (live) loading = false; });
    }, 300);
    return () => { live = false; clearTimeout(timer); };
  });
  async function more() {
    if (loading) return;
    loading = true;
    try {
      const r = await fetchPage(s.sort, s.page + 1);
      const have = new Set(s.items.map((x) => x.id));
      s.items = [...s.items, ...r.items.map((m) => toTitle(m, s.kind)).filter((x) => !have.has(x.id))];
      s.page += 1; s.total = r.total;
    } catch (e) {
      onError?.(String((e as Error).message ?? e));
    } finally {
      loading = false;
    }
  }
  const canMore = $derived(s.grid && s.page > 0 && s.page < 16 && s.items.length < s.total);

  /** Bir filtre dokunuşu ızgarayı açar. */
  function set(fn: () => void) { fn(); s.grid = true; }
  function openShelf(sh: (typeof SHELVES)[number]) { s.sort = sh.sort; s.lang = sh.lang ?? ""; s.grid = true; }
  function reset() {
    Object.assign(s, { genre: 0, lo: FIRST, hi: YEAR, minRating: 0, maxMin: 0, lang: "", sort: "popular", grid: false, items: [], page: 0, total: 0, key: "" });
  }
  function setKind(k: Kind) {
    if (s.kind === k) return;
    s.kind = k;
    s.key = "";
  }
</script>

<div class="bt">
  <div class="row">
    <div class="seg">
      <button class:on={s.kind === "movie"} onclick={() => setKind("movie")}>{t("Filmler")}</button>
      <button class:on={s.kind === "series"} onclick={() => setKind("series")}>{t("Diziler")}</button>
    </div>
    <div class="chips">
      <button class:on={!s.genre} onclick={() => set(() => (s.genre = 0))}>{t("Tümü")}</button>
      {#each GENRES as [id, name] (id)}
        <button class:on={s.genre === id} onclick={() => set(() => (s.genre = s.genre === id ? 0 : id))}>{t(name)}</button>
      {/each}
    </div>
  </div>

  <div class="filters">
    <label class="yr">
      <span>{t("Yıl")} <b>{s.lo}–{s.hi}</b></span>
      <DualRange min={FIRST} max={YEAR} bind:lo={() => s.lo, (v) => set(() => (s.lo = v))} bind:hi={() => s.hi, (v) => set(() => (s.hi = v))} />
    </label>
    <label>
      <span>{t("En az puan")}</span>
      <select value={s.minRating} onchange={(e) => set(() => (s.minRating = Number(e.currentTarget.value)))}>
        <option value={0}>{t("Hepsi")}</option>
        {#each [6, 7, 8] as n (n)}<option value={n}>{n}+</option>{/each}
      </select>
    </label>
    <label>
      <span>{s.kind === "series" ? t("Bölüm süresi") : t("Süre")}</span>
      <select value={s.maxMin} onchange={(e) => set(() => (s.maxMin = Number(e.currentTarget.value)))}>
        <option value={0}>{t("Hepsi")}</option>
        {#each (s.kind === "series" ? [30, 45, 60] : [90, 120, 150]) as n (n)}<option value={n}>{t("en çok {n} dk", { n })}</option>{/each}
      </select>
    </label>
    <label>
      <span>{t("Özgün dil")}</span>
      <select value={s.lang} onchange={(e) => set(() => (s.lang = e.currentTarget.value))}>
        <option value="">{t("Hepsi")}</option>
        {#each LANGS as [id, name] (id)}<option value={id}>{t(name)}</option>{/each}
      </select>
    </label>
    <label>
      <span>{t("Sırala")}</span>
      <select value={s.sort} onchange={(e) => set(() => (s.sort = e.currentTarget.value as Sort))}>
        {#each SORTS as [id, name] (id)}<option value={id}>{t(name)}</option>{/each}
      </select>
    </label>
    {#if s.grid}<button class="clr" onclick={reset}><Icon name="x" size={13} /> {t("Filtreleri temizle")}</button>{/if}
  </div>

  {#if err}
    <div class="intro"><span>{err}</span></div>
  {:else if !s.grid}
    {#each SHELVES as sh (sh.id)}
      {@const list = shelves[sh.id]}
      {#if !list || list.length}
        <section>
          <div class="lab"><b>{t(sh.title)}</b><button class="all" onclick={() => openShelf(sh)}>{t("Tümü")} <Icon name="fwd" size={12} /></button></div>
          {#if list}
            <div class="shelf">{#each list as ti (ti.id)}{@render card(ti)}{/each}</div>
          {:else}
            <div class="shelf wait">{#each Array(7) as _, i (i)}<span class="sk"></span>{/each}</div>
          {/if}
        </section>
      {/if}
    {/each}
  {:else if loading && !s.items.length}
    <div class="intro"><span>{t("Yükleniyor…")}</span></div>
  {:else if s.items.length === 0 && s.key}
    <div class="intro"><span>{t("Bu filtrelere uyan başlık yok.")}</span></div>
  {:else}
    <div class="lab"><b>{t("{n} başlık", { n: s.total.toLocaleString() })}</b></div>
    <div class="pgrid">{#each s.items as ti (ti.id)}{@render card(ti)}{/each}</div>
    {#if canMore}<button class="moreb" disabled={loading} onclick={more}>{loading ? t("Yükleniyor…") : t("Daha fazla")}</button>{/if}
  {/if}
</div>

<style>
  .bt { display: flex; flex-direction: column; gap: 16px; }
  .row { display: flex; gap: 12px; align-items: flex-start; }
  .seg { display: flex; flex: none; padding: 3px; border-radius: 10px; background: rgba(255, 255, 255, 0.06); border: 1px solid var(--line); }
  .seg button { height: 26px; padding: 0 12px; border: 0; border-radius: 8px; font-size: 12px; font-weight: 650; color: var(--text-2); background: none; }
  .seg button.on { background: #fff; color: #000; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chips button { height: 30px; padding: 0 12px; border-radius: 99px; font-size: 12px; font-weight: 600; color: var(--text-2); background: rgba(255, 255, 255, 0.05); border: 1px solid var(--line); }
  .chips button:hover { color: var(--text); background: rgba(255, 255, 255, 0.1); }
  .chips button.on { background: #fff; color: #000; border-color: #fff; }
  .filters { display: flex; flex-wrap: wrap; gap: 10px 16px; align-items: flex-end; }
  .filters label { display: flex; flex-direction: column; gap: 4px; font-size: 11.5px; color: var(--text-3); font-weight: 600; }
  .filters label.yr { width: 220px; }
  .filters b { color: var(--text-2); font-weight: 650; }
  .filters select { height: 32px; font-size: 12.5px; }
  .clr { height: 32px; padding: 0 12px; border-radius: 10px; font-size: 12px; display: inline-flex; align-items: center; gap: 5px; color: var(--text-2); }
  section { display: flex; flex-direction: column; gap: 10px; }
  .lab { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .lab b { font-size: 15px; font-weight: 750; letter-spacing: -0.01em; }
  .all { height: 28px; padding: 0 10px; border-radius: 8px; font-size: 12px; display: inline-flex; align-items: center; gap: 4px; color: var(--text-2); }
  .shelf { display: grid; grid-auto-flow: column; grid-auto-columns: 140px; gap: 14px; overflow-x: auto; padding-bottom: 6px; }
  .sk { aspect-ratio: 2 / 3; border-radius: 11px; background: rgba(255, 255, 255, 0.05); }
  .pgrid { display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 18px 14px; }
  .moreb { align-self: center; height: 36px; padding: 0 22px; border-radius: 11px; font-weight: 650; }
  .intro { text-align: center; margin: 6vh auto 0; max-width: 440px; color: var(--text-2); }
</style>
