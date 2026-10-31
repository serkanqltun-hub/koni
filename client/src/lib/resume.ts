/** Yarım kalan filmler: medya adresine göre son konum (yalnızca yerel; sunucuya gitmez). */
/** Kaydedilen medya. `debrid`'de `src` infoHash, `file`/`idx` torrentteki dosya ipucu. */
export interface ResumeMedia {
  src: string;
  kind: "url" | "file" | "download" | "debrid";
  title: string;
  file?: string;
  idx?: number;
}
export interface ResumeEntry extends ResumeMedia {
  /** Kaydın yapıldığı oda adı (tek başınaysa yok). */
  room?: string;
  /** Birlikte izlediklerin (hesap adları, etiketsiz): "aynı ekiple devam". */
  with?: string[];
  pos: number;
  duration: number;
  at: number;
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
  const rest = loadResume().filter((x) => x.src !== e.src);
  if (!(e.duration > 0) || e.pos < 30 || e.duration - e.pos < 90) return store(rest);
  return store([{ ...e, at: Date.now() }, ...rest].slice(0, MAX));
}

export function removeResume(src: string): ResumeEntry[] {
  return store(loadResume().filter((x) => x.src !== src));
}
