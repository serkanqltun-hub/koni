<script lang="ts">
  // Lobide "Kütüphanem" rafı: Ayarlar'da eklenen klasörlerdeki filmler ve diziler (afişli).
  // Film afişi dosyayı açar; dizi afişi bölüm listesini açar. Kaldığın yer (resume.ts) ve
  // izlendi bilgisi (bu bilgisayar + hesaptaki İzlediklerim / bölüm izledim) birlikte kullanılır.
  // Klasör yoksa hiçbir şey çizmez.
  import { library, initLibrary, shelves, type Shelf } from "./library.svelte";
  import { loadResume } from "./resume";
  import { listsGet, type LibraryItem } from "./player";
  import { social } from "./social.svelte";
  import { t } from "./i18n.svelte";
  import Icon from "./Icon.svelte";

  let { busy, onOpen }: { busy: boolean; onOpen: (path: string) => void } = $props();

  initLibrary();
  const list = $derived(shelves(library.data?.items ?? []));
  const resume = loadResume().filter((r) => r.kind === "file");
  const posOf = (path: string) => {
    const r = resume.find((x) => x.src === path);
    return r && r.duration > 0 ? r.pos / r.duration : 0;
  };

  // Hesaptaki izlenenler: film kimlikleri ve dizi başına "s:e".
  let seenRefs = $state<string[]>([]);
  let seenEps = $state<Record<string, string[]>>({});
  $effect(() => {
    const server = social.server;
    if (!server || !list.length) return;
    listsGet(server).then((l) => {
      if (!l) return;
      seenRefs = l.seen.map((e) => e.ref);
      seenEps = l.episodes ?? {};
    }, () => {});
  });

  function seen(s: Shelf, i: LibraryItem): boolean {
    if (library.seen.includes(i.path)) return true;
    if (!s.ref) return false;
    return s.kind === "movie" ? seenRefs.includes(s.ref) : (seenEps[s.ref] ?? []).includes(`${i.season}:${i.episode}`);
  }
  const unseen = (s: Shelf) => s.items.filter((i) => !seen(s, i)).length;
  /** Sıradaki bölüm: son izlenenden sonraki ilk izlenmemiş; hiçbiri izlenmediyse ilki. */
  function next(s: Shelf): LibraryItem | undefined {
    let last = -1;
    s.items.forEach((i, n) => { if (seen(s, i)) last = n; });
    return s.items.slice(last + 1).find((i) => !seen(s, i)) ?? s.items.find((i) => !seen(s, i));
  }
  const code = (i: LibraryItem) => `S${String(i.season ?? 0).padStart(2, "0")}E${String(i.episode ?? 0).padStart(2, "0")}`;
  const file = (p: string) => p.split(/[\\/]/).pop() ?? p;

  let open = $state<string | null>(null);
  const sel = $derived(list.find((s) => s.key === open) ?? null);
  function pick(s: Shelf) {
    if (s.kind === "movie") return onOpen(s.items[0].path);
    open = open === s.key ? null : s.key;
  }
</script>

{#if library.data?.folders.length}
  <section class="lib">
    <h4>{t("Kütüphanem")} <span>{library.busy ? t("Taranıyor…") : t("{n} başlık", { n: list.length })}</span></h4>
    {#if list.length}
      <div class="row">
        {#each list as s (s.key)}
          {@const n = s.kind === "series" ? unseen(s) : 0}
          {@const p = s.kind === "movie" ? posOf(s.items[0].path) : 0}
          {@const done = s.kind === "movie" ? seen(s, s.items[0]) : n === 0}
          <button type="button" class="tile" class:on={open === s.key} disabled={busy && s.kind === "movie"} onclick={() => pick(s)} title={s.kind === "movie" ? file(s.items[0].path) : s.title}>
            <span class="img" style="--p:{p * 100}%">
              {#if s.poster}<img src={s.poster} alt="" loading="lazy" referrerpolicy="no-referrer" onerror={(e) => { (e.currentTarget as HTMLElement).style.display = "none"; }} />{/if}
              <i class="ph">{s.title}</i>
              {#if n > 0}<u class="new">{n === s.items.length ? t("{n} bölüm", { n }) : t("{n} yeni", { n })}</u>{:else if done}<u class="ok">✓</u>{/if}
            </span>
            <b>{s.title}</b>
            <small>{s.kind === "series" ? t("DİZİ") : t("FİLM")}{s.kind === "series" ? ` · ${t("{n} bölüm", { n: s.items.length })}` : ""}</small>
          </button>
        {/each}
      </div>
      {#if sel}
        {@const nx = next(sel)}
        <div class="eps">
          <div class="eps-h">
            <b>{sel.title}</b>
            {#if nx}<button class="white" disabled={busy} onclick={() => onOpen(nx.path)}><Icon name="play" size={12} /> {t("Sıradaki: {code}", { code: code(nx) })}</button>{/if}
            <button class="x" title={t("Kapat")} onclick={() => (open = null)}><Icon name="x" size={12} /></button>
          </div>
          {#each sel.items as i (i.path)}
            {@const p = posOf(i.path)}
            <button type="button" class="ep" class:seen={seen(sel, i)} disabled={busy} onclick={() => onOpen(i.path)} title={file(i.path)}>
              <span class="code">{code(i)}</span>
              <span class="fn">{file(i.path)}</span>
              {#if seen(sel, i)}<span class="st">✓ {t("izlendi")}</span>{:else if p > 0}<span class="bar" style="--p:{p * 100}%"></span>{/if}
              <Icon name="play" size={12} />
            </button>
          {/each}
        </div>
      {/if}
    {:else}
      <div class="none">{library.busy ? t("Klasörler taranıyor…") : t("Eklediğin klasörlerde video bulunamadı.")}</div>
    {/if}
  </section>
{/if}

<style>
  .lib { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  h4 { margin: 0; font-size: 11.5px; color: var(--text-2); text-transform: uppercase; letter-spacing: 0.06em; display: flex; gap: 10px; align-items: baseline; }
  h4 span { text-transform: none; letter-spacing: 0; color: var(--text-3); font-weight: 500; }
  .row { display: flex; gap: 12px; overflow-x: auto; padding-bottom: 6px; scrollbar-width: thin; }
  .tile { all: unset; cursor: pointer; flex: none; width: 120px; display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .tile:disabled { cursor: default; opacity: 0.6; }
  .img {
    position: relative; width: 120px; aspect-ratio: 2 / 3; border-radius: 10px; overflow: hidden; margin-bottom: 4px;
    background: rgba(255, 255, 255, 0.06); border: 1px solid var(--line); display: grid; place-items: center;
  }
  .img::after { content: ""; position: absolute; left: 0; bottom: 0; height: 3px; width: var(--p); background: #ff375f; z-index: 3; }
  .tile:hover .img, .tile:focus-visible .img { border-color: var(--line-2); }
  .tile.on .img { border-color: rgba(10, 132, 255, 0.8); }
  .img img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; z-index: 1; }
  .ph { font-style: normal; font-size: 11.5px; padding: 8px; text-align: center; color: var(--text-3); }
  u { position: absolute; top: 6px; right: 6px; z-index: 2; text-decoration: none; font-size: 10.5px; font-weight: 750; padding: 3px 7px; border-radius: 7px; background: rgba(0, 0, 0, 0.72); }
  u.new { background: #0a84ff; color: #fff; }
  u.ok { color: var(--green); }
  b { font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  small { color: var(--text-3); font-size: 11.5px; }
  .none { padding: 16px; border-radius: 14px; border: 1px solid var(--line); color: var(--text-3); font-size: 12.5px; }
  .eps { border-radius: 14px; border: 1px solid var(--line); background: rgba(255, 255, 255, 0.045); padding: 8px; display: flex; flex-direction: column; gap: 2px; max-height: 320px; overflow-y: auto; }
  .eps-h { display: flex; align-items: center; gap: 10px; padding: 4px 6px 8px; }
  .eps-h b { flex: 1; font-size: 13.5px; }
  .eps-h .white { height: 30px; padding: 0 12px; border-radius: 9px; font-size: 12px; font-weight: 700; display: inline-flex; gap: 5px; align-items: center; }
  .x { width: 26px; height: 26px; padding: 0; border-radius: 7px; display: grid; place-items: center; color: var(--text-3); background: transparent; border: 0; }
  .ep { all: unset; cursor: pointer; display: flex; align-items: center; gap: 12px; padding: 7px 8px; border-radius: 9px; font-size: 12.5px; color: var(--text-2); }
  .ep:hover, .ep:focus-visible { background: rgba(255, 255, 255, 0.06); color: var(--text); }
  .ep:disabled { cursor: default; opacity: 0.6; }
  .ep.seen .code, .ep.seen .fn { color: var(--text-3); }
  .code { font: 600 12px var(--font-mono); color: var(--text); }
  .fn { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .st { color: var(--green); font-size: 11.5px; white-space: nowrap; }
  .bar { width: 56px; height: 3px; border-radius: 2px; background: linear-gradient(90deg, #ff375f var(--p), rgba(255, 255, 255, 0.12) var(--p)); }
</style>
