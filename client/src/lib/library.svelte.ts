// Yerel kütüphane: klasörler ve taranan videolar (Rust library.rs), bitirilen dosyalar.
// İzlendi bilgisi bu bilgisayarda tutulur; dosya Cinemeta'da eşlendiyse hesaba da yazılır
// (film İzlediklerim'e, bölüm `seen_ep`). Kaldığın yer resume.ts'in yerel dosya kaydıdır.
import { libraryAdd, libraryGet, libraryRemove, libraryScan, listsAdd, listsSeenEp, type LibraryData, type LibraryItem } from "./player";
import { social } from "./social.svelte";

const SEEN_KEY = "koni.librarySeen";
const SEEN_MAX = 3000;

export const library = $state({
  data: null as LibraryData | null,
  busy: false,
  error: null as string | null,
  /** Bitirilen dosya yolları. */
  seen: (() => {
    try {
      const v = JSON.parse(localStorage.getItem(SEEN_KEY) ?? "[]");
      return Array.isArray(v) ? v.filter((x) => typeof x === "string") : [];
    } catch { return []; }
  })() as string[],
});

let loaded = false;
export function initLibrary() {
  if (loaded) return;
  loaded = true;
  libraryGet().then((d) => (library.data = d)).catch(() => (loaded = false));
}

async function run(job: () => Promise<LibraryData | null>) {
  if (library.busy) return;
  library.busy = true;
  library.error = null;
  try {
    const d = await job();
    if (d) library.data = d;
  } catch (e) {
    library.error = String(e);
  } finally {
    library.busy = false;
  }
}
export const addFolder = () => run(libraryAdd);
export const removeFolder = (f: string) => run(() => libraryRemove(f));
export const rescan = () => run(libraryScan);

/** resume.ts: yerel dosya sonuna kadar izlendi. */
export function markFileSeen(path: string) {
  if (library.seen.includes(path)) return;
  library.seen = [path, ...library.seen].slice(0, SEEN_MAX);
  try { localStorage.setItem(SEEN_KEY, JSON.stringify(library.seen)); } catch { /* depolama kapalı */ }
  const it = library.data?.items.find((i) => i.path === path);
  const m = it?.m;
  if (!it || !m || !social.server) return;
  const done = m.kind === "series"
    ? it.season != null && it.episode != null ? listsSeenEp(social.server, m.id, it.season, it.episode) : null
    : listsAdd(social.server, "seen", { ref: m.id, kind: "movie", title: m.name, year: m.year ? parseInt(m.year) || undefined : undefined, poster: m.poster ?? undefined });
  done?.catch(() => {});
}

/** Rafta bir kart: film ya da dizinin bölümleri. */
export interface Shelf { key: string; title: string; poster?: string | null; kind: "movie" | "series"; ref?: string; items: LibraryItem[]; added: number }
export function shelves(items: LibraryItem[]): Shelf[] {
  const map = new Map<string, Shelf>();
  for (const i of items) {
    const series = i.season != null;
    const key = series ? `s:${i.m?.id ?? i.name.toLowerCase()}` : `f:${i.path}`;
    let s = map.get(key);
    if (!s) {
      s = { key, title: i.m?.name ?? i.name, poster: i.m?.poster, kind: series ? "series" : "movie", ref: i.m?.id, items: [], added: 0 };
      map.set(key, s);
    }
    s.items.push(i);
    s.added = Math.max(s.added, i.added);
  }
  for (const s of map.values()) s.items.sort((a, b) => (a.season ?? 0) - (b.season ?? 0) || (a.episode ?? 0) - (b.episode ?? 0));
  return [...map.values()].sort((a, b) => b.added - a.added);
}
