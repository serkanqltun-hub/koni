// Çalma listeleri (Müzik modu, docs/design/media.md §6.3): bu bilgisayarda localStorage'da
// durur, sunucuya gitmez. Çalan liste tek başına modda sırayla yerelde açılır; odada
// Lounge kuyruğuna azar azar eklenir (Santral kişi başı 10 şarkı alır), App.svelte besler.

export interface Track { src: string; title: string; dur: number | null }
export interface Playlist { id: string; name: string; tracks: Track[] }

const KEY = "koni.playlists";
/** Kalple eklenenler; ilk beğenide oluşur. */
export const LIKED_ID = "liked";
const LIKED_NAME = "Beğendiklerim";

function load(): Playlist[] {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    if (!Array.isArray(v)) return [];
    return v
      .filter((p) => p && typeof p.id === "string" && typeof p.name === "string" && Array.isArray(p.tracks))
      .map((p) => ({
        id: p.id,
        name: p.name,
        tracks: (p.tracks as Partial<Track>[])
          .filter((t) => t && typeof t.src === "string")
          .map((t) => ({ src: t.src!, title: String(t.title || t.src), dur: typeof t.dur === "number" ? t.dur : null })),
      }));
  } catch {
    return [];
  }
}

export const lists = $state<{ all: Playlist[] }>({ all: load() });
const save = () => localStorage.setItem(KEY, JSON.stringify(lists.all));
export const findList = (id: string) => lists.all.find((p) => p.id === id);

export function createList(name: string): string {
  const id = crypto.randomUUID();
  lists.all.push({ id, name: name.trim().slice(0, 80) || "Yeni liste", tracks: [] });
  save();
  return id;
}

export function renameList(id: string, name: string) {
  const p = findList(id);
  const n = name.trim().slice(0, 80);
  if (!p || !n) return;
  p.name = n;
  save();
}

export function deleteList(id: string) {
  lists.all = lists.all.filter((p) => p.id !== id);
  if (playing.id === id) stopList();
  save();
}

/** Listede olmayanları sona ekler; eklenen sayısını döner. */
export function addTracks(id: string, tracks: Track[]): number {
  const p = findList(id);
  if (!p) return 0;
  const have = new Set(p.tracks.map((t) => t.src));
  let n = 0;
  for (const t of tracks) {
    if (!t.src || have.has(t.src)) continue;
    have.add(t.src);
    p.tracks.push({ src: t.src, title: t.title || t.src, dur: t.dur ?? null });
    n++;
  }
  if (n) save();
  return n;
}

export function removeTrack(id: string, src: string) {
  const p = findList(id);
  if (!p) return;
  p.tracks = p.tracks.filter((t) => t.src !== src);
  save();
}

/** `i`. şarkıyı bir yukarı (-1) ya da aşağı (+1) taşır. */
export function moveTrack(id: string, i: number, d: -1 | 1) {
  const p = findList(id);
  const j = i + d;
  if (!p || i < 0 || j < 0 || i >= p.tracks.length || j >= p.tracks.length) return;
  [p.tracks[i], p.tracks[j]] = [p.tracks[j], p.tracks[i]];
  save();
}

export const isLiked = (src: string) => findList(LIKED_ID)?.tracks.some((t) => t.src === src) ?? false;

/** Kalp: Beğendiklerim'e ekler ya da çıkarır; yeni durumu döner. */
export function toggleLike(t: Track): boolean {
  if (isLiked(t.src)) {
    removeTrack(LIKED_ID, t.src);
    return false;
  }
  if (!findList(LIKED_ID)) {
    lists.all.unshift({ id: LIKED_ID, name: LIKED_NAME, tracks: [] });
  }
  addTracks(LIKED_ID, [t]);
  return true;
}

/** Çalan liste. `gen` her şarkı değişiminde artar (tek başına mod açar); `fed`: odada
 *  Lounge'a eklenen şarkı sayısı. */
export const playing = $state({ id: "", name: "", queue: [] as Track[], pos: -1, fed: 0, gen: 0 });

export function playList(id: string, from = 0, shuffle = false) {
  const p = findList(id);
  if (!p?.tracks.length) return;
  let q = $state.snapshot(p.tracks).slice(Math.max(0, from));
  if (shuffle) {
    q = $state.snapshot(p.tracks);
    for (let i = q.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [q[i], q[j]] = [q[j], q[i]];
    }
  }
  Object.assign(playing, { id, name: p.name, queue: q, pos: 0, fed: 0, gen: playing.gen + 1 });
}

export function stopList() {
  Object.assign(playing, { id: "", name: "", queue: [], pos: -1, fed: 0 });
}

/** Tek başına: sıradaki/önceki şarkı; liste bitince durur. */
export function stepList(d: -1 | 1) {
  const pos = playing.pos + d;
  if (!playing.id || pos < 0) return;
  if (pos >= playing.queue.length) return stopList();
  playing.pos = pos;
  playing.gen++;
}

export const nowTrack = () => (playing.id ? (playing.queue[playing.pos] ?? null) : null);

export function fmtDur(s: number | null | undefined): string {
  if (s == null || !isFinite(s)) return "";
  const t = Math.round(s);
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const sec = String(t % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}
