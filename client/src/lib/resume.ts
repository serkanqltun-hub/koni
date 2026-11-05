/** Yarım kalan filmler: medya adresine göre son konum. Kimliği bilinen film/dizi (`file` alanı
 *  `movie:tt…` / `series:tt…:s:e`) hesaplı sunucuya da yazılır (p0.25 `progress`); yerel dosya ve linkler
 *  yalnızca bu bilgisayarda kalır. */
import { listsGet, listsProgress, listsRemove, type Progress } from "./player";
import { markFileSeen } from "./library.svelte";

/** Kaydedilen medya. `debrid`'de `src` infoHash, `file`/`idx` dosya ipucu. */
export interface ResumeMedia {
  src: string;
  kind: "url" | "file" | "download" | "debrid";
  title: string;
  file?: string;
  idx?: number;
}
/** `kind: "ref"`: başka bilgisayardan gelen kayıt; kaynağı yok, başlık aranıp açılır. */
export interface ResumeEntry extends Omit<ResumeMedia, "kind"> {
  kind: ResumeMedia["kind"] | "ref";
  /** Kaydın yapıldığı oda adı (tek başınaysa yok). */
  room?: string;
  /** Birlikte izlediklerin (hesap adları, etiketsiz): "aynı ekiple devam". */
  with?: string[];
  pos: number;
  duration: number;
  at: number;
  poster?: string;
}

const KEY = "koni.resume";
const MAX = 20;

export function loadResume(): ResumeEntry[] {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(v) ? v.filter((r) => r && typeof r.src === "string" && typeof r.pos === "number" && typeof r.duration === "number") : [];
  } catch {
    return [];
  }
}

function store(list: ResumeEntry[]): ResumeEntry[] {
  try { localStorage.setItem(KEY, JSON.stringify(list)); } catch { /* depolama kapalı */ }
  return list;
}

/** Konumu kaydeder; başlamamış (<30 sn) ya da bitmiş (son 90 sn) sayılırsa kaydı siler. */
export function saveResume(e: Omit<ResumeEntry, "at">): ResumeEntry[] {
  const rest = loadResume().filter((x) => x.src !== e.src && !sameTitle(x, e));
  if (!(e.duration > 0) || e.pos < 30 || e.duration - e.pos < 90) {
    if (e.duration > 0 && e.pos >= 30) {
      pushRemove(e); // bitti
      if (e.kind === "file") markFileSeen(e.src);
    }
    return store(rest);
  }
  pushProgress(e);
  return store([{ ...e, at: Date.now() }, ...rest].slice(0, MAX));
}

export function removeResume(src: string): ResumeEntry[] {
  const all = loadResume();
  const gone = all.find((x) => x.src === src);
  if (gone) pushRemove(gone);
  return store(all.filter((x) => x.src !== src));
}

/** Oynayan medyanın kaydı: aynı adres ya da aynı başlık/bölüm (başka bilgisayardan gelen kayıt). */
export function findResume(m: { src: string; file?: string }): ResumeEntry | undefined {
  return loadResume().find((x) => x.src === m.src || sameTitle(x, m));
}

// ---- Sunucu senkronu (p0.25, PROTOCOL §3.0g `progress`) ---------------------------------

interface Ref { ref: string; kind: "movie" | "series"; season?: number; episode?: number }
/** `file` → başlık kimliği; kimliksiz medyada null. */
export function refOf(file: string | undefined): Ref | null {
  const s = file?.match(/^series:(tt\d+):(\d+):(\d+)/);
  if (s) return { ref: s[1], kind: "series", season: +s[2], episode: +s[3] };
  const m = file?.match(/^movie:(tt\d+)/);
  return m ? { ref: m[1], kind: "movie" } : null;
}
const epKey = (r: Ref | null) => (r ? `${r.ref}:${r.season ?? 0}:${r.episode ?? 0}` : "");
function sameTitle(a: { file?: string }, b: { file?: string }): boolean {
  const k = epKey(refOf(a.file));
  return k !== "" && k === epKey(refOf(b.file));
}
const code = (s: number, e: number) => `S${String(s).padStart(2, "0")}E${String(e).padStart(2, "0")}`;

let server: string | null = null;
/** Hesaplı sunucu adresi; hesap yoksa null (senkron kapalı). */
export function setResumeServer(url: string | null) {
  server = url;
  pulledAt = 0;
}

const PUSH_MS = 30_000;
/** Başlık başına son gönderim; `last` duraklatmayı fark etmek için son görülen konum. */
const sent = new Map<string, { at: number; pos: number; last: number }>();
let pending: Omit<ResumeEntry, "at"> | null = null;

/** 30 sn'de bir; konum değişmediyse (duraklatıldı) hemen. */
function pushProgress(e: Omit<ResumeEntry, "at">) {
  const r = refOf(e.file);
  if (!server || !r) return;
  const now = Date.now(), s = sent.get(r.ref);
  const paused = s != null && Math.abs(e.pos - s.last) < 0.5 && Math.abs(e.pos - s.pos) >= 1;
  if (s) s.last = e.pos;
  pending = e;
  if (s && now - s.at < PUSH_MS && !paused) return;
  send(e, r);
}
function send(e: Omit<ResumeEntry, "at">, r: Ref) {
  pending = null;
  sent.set(r.ref, { at: Date.now(), pos: e.pos, last: e.pos });
  const title = r.kind === "series" ? e.title.replace(/\s+S\d+E\d+$/, "") : e.title;
  listsProgress(server!, { ...r, pos: Math.round(e.pos * 10) / 10, duration: Math.round(e.duration), title, poster: e.poster }).catch(() => {});
}
/** Medya kapanınca son konumu hemen gönderir. */
export function flushResume() {
  const e = pending, r = refOf(e?.file);
  if (e && r && server) send(e, r);
  pending = null;
}
function pushRemove(e: { file?: string }) {
  const r = refOf(e.file);
  if (!server || !r || !sent.has(r.ref) && !loadResume().some((x) => refOf(x.file)?.ref === r.ref)) return;
  sent.delete(r.ref);
  if (pending && refOf(pending.file)?.ref === r.ref) pending = null;
  listsRemove(server, "progress", r.ref).catch(() => {});
}

let pulledAt = 0;
/** Sunucudaki kayıtları yerel listeyle birleştirir (yeni olan kazanır); en çok 30 sn'de bir sorar. */
export async function pullResume(): Promise<ResumeEntry[]> {
  if (!server || Date.now() - pulledAt < PUSH_MS) return loadResume();
  pulledAt = Date.now();
  let rows: Progress[];
  try { rows = (await listsGet(server))?.progress ?? []; } catch { return loadResume(); }
  return store(mergeResume(loadResume(), rows));
}

/** Saf birleştirme: başlık başına en yeni kayıt kalır; yerelde kaynağı olan bölüm aynıysa o korunur. */
export function mergeResume(local: ResumeEntry[], rows: Progress[]): ResumeEntry[] {
  let out = [...local];
  for (const p of rows) {
    if (!p.ref || !(p.duration && p.duration > 0)) continue;
    const mine = out.filter((x) => refOf(x.file)?.ref === p.ref);
    if (mine.some((x) => x.at >= p.at)) continue;
    const r: Ref = { ref: p.ref, kind: p.kind, season: p.season, episode: p.episode };
    const file = p.kind === "series" ? `series:${p.ref}:${p.season ?? 0}:${p.episode ?? 0}` : `movie:${p.ref}`;
    const same = mine.find((x) => epKey(refOf(x.file)) === epKey(r));
    out = out.filter((x) => x !== same);
    const name = p.title || p.ref;
    out.push(same
      ? { ...same, pos: p.pos, duration: p.duration, at: p.at }
      : { src: `ref:${p.ref}`, kind: "ref", file, pos: p.pos, duration: p.duration, at: p.at, poster: p.poster,
          title: p.kind === "series" ? `${name} ${code(p.season ?? 0, p.episode ?? 0)}` : name });
  }
  return out.sort((a, b) => b.at - a.at).slice(0, MAX);
}
