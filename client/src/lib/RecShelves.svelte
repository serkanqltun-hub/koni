<script lang="ts">
  // Öneri rafları (p0.27 `recs`): "Çünkü X'i izledin", "Arkadaşların beğendi", "Ayşe de izleyecek".
  // Hesap yoksa ya da raf boşsa hiçbir şey çizmez. Afişe tıklayınca künye (MovieDetail) açılır.
  import { recsGet, type DetailRef, type Movie, type RecEntry, type Recs } from "./player";
  import { social, splitHandle } from "./social.svelte";
  import { t } from "./i18n.svelte";

  let { onOpen, onCall = null }: {
    onOpen: (d: DetailRef) => void;
    /** "Çağır": o arkadaşı odaya çağırır (tek başınaysa oda kurulur). Verilmezse düğme yok. */
    onCall?: ((name: string) => void) | null;
  } = $props();

  let recs = $state<Recs | null>(null);
  $effect(() => {
    const server = social.server;
    if (!server) return;
    recsGet(server).then((r) => (recs = r), () => {});
  });

  const fromMovie = (m: Movie): DetailRef => ({
    ref: m.imdb || `tmdb:${m.kind === "tv" ? "tv" : "movie"}:${m.id}`,
    kind: m.kind === "tv" ? "series" : "movie", title: m.title, poster: m.poster, year: m.year,
  });
  const fromEntry = (e: RecEntry): DetailRef => ({ ref: e.ref, kind: e.kind, title: e.title, poster: e.poster, year: e.year });
  const bare = (n: string) => splitHandle(n)[0];
  const names = (l: { name: string }[]) => l.slice(0, 2).map((h) => bare(h.name)).join(", ") + (l.length > 2 ? ` +${l.length - 2}` : "");
  const shelves = $derived(recs ? (recs.because.some((b) => b.items.length) || recs.friends.length || recs.together.length) : false);
</script>

{#snippet tile(d: DetailRef, badge: string, call?: string)}
  <div class="tile">
    <button type="button" class="open" onclick={() => onOpen(d)} title={d.title}>
      <span class="img">
        {#if d.poster}<img src={d.poster} alt="" loading="lazy" referrerpolicy="no-referrer" onerror={(e) => { (e.currentTarget as HTMLElement).style.display = "none"; }} />{/if}
        <i class="ph">{d.title}</i>
        {#if badge}<u>{badge}</u>{/if}
      </span>
      <b>{d.title}</b>
      <small>{[d.year, d.kind === "series" ? t("DİZİ") : t("FİLM")].filter(Boolean).join(" · ")}</small>
    </button>
    {#if call && onCall}<button type="button" class="call" onclick={() => onCall!(call)}>{t("Birlikte aç · Çağır")}</button>{/if}
  </div>
{/snippet}

{#if recs && shelves}
  <div class="recs">
    {#each recs.because.filter((b) => b.items.length) as b (b.ref)}
      <section>
        <h4>{t("Çünkü izlediğin: {title}", { title: b.title })}</h4>
        <div class="row">{#each b.items as m (m.id)}{@render tile(fromMovie(m), "")}{/each}</div>
      </section>
    {/each}
    {#if recs.friends.length}
      <section>
        <h4>{t("Arkadaşların beğendi")}</h4>
        <div class="row">{#each recs.friends as e (e.ref)}{@render tile(fromEntry(e), e.by?.length ? `★ ${names(e.by)}` : "")}{/each}</div>
      </section>
    {/if}
    {#if recs.together.length}
      <section>
        <h4>{t("Arkadaşların da izleyecek")}</h4>
        <div class="row">{#each recs.together as e (e.ref)}{@render tile(fromEntry(e), e.mates?.length ? t("{names} de istiyor", { names: names(e.mates) }) : "", e.mates?.[0]?.name)}{/each}</div>
      </section>
    {/if}
  </div>
{/if}

<style>
  .recs { display: flex; flex-direction: column; gap: 18px; min-width: 0; }
  section { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  h4 { margin: 0; font-size: 11.5px; color: var(--text-2); text-transform: uppercase; letter-spacing: 0.06em; }
  .row { display: flex; gap: 12px; overflow-x: auto; padding-bottom: 6px; scrollbar-width: thin; }
  .tile { flex: none; width: 120px; display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .open { all: unset; cursor: pointer; display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .img {
    position: relative; width: 120px; aspect-ratio: 2 / 3; border-radius: 10px; overflow: hidden; margin-bottom: 4px;
    background: rgba(255, 255, 255, 0.06); border: 1px solid var(--line); display: grid; place-items: center;
  }
  .open:hover .img, .open:focus-visible .img { border-color: var(--line-2); }
  .img img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; z-index: 1; }
  .ph { font-style: normal; font-size: 11.5px; padding: 8px; text-align: center; color: var(--text-3); }
  u {
    position: absolute; left: 6px; right: 6px; bottom: 6px; z-index: 2; text-decoration: none; font-size: 10.5px; font-weight: 700; line-height: 1.25;
    padding: 3px 7px; border-radius: 7px; background: rgba(0, 0, 0, 0.72); color: #ffd60a; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  b { font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  small { color: var(--text-3); font-size: 11.5px; }
  .call { height: 28px; border-radius: 8px; font-size: 11.5px; font-weight: 650; padding: 0 6px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
