/** Medya başına son seçilen altyazı ve gecikmesi: aynı film/bölüm yeniden açılınca geri yüklenir.
 *  Yalnızca bu bilgisayarda (dosya yolları burada geçerli); hem tek başına hem odada çalışır. */
import { refOf } from "./resume";
import type { SubPick } from "./player";

const KEY = "koni.subs";
const MAX = 100;

type Store = Record<string, SubPick & { at: number }>;

/** Film/bölüm kimliği varsa o (kaynak değişse de bulunur), yoksa adres + dosya ipucu. */
export function subKey(m: { src: string; file?: string; idx?: number }): string {
  const r = refOf(m.file);
  if (r) return `${r.ref}:${r.season ?? 0}:${r.episode ?? 0}`;
  return `${m.src}#${m.file ?? m.idx ?? ""}`;
}

function load(): Store {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    return v && typeof v === "object" && !Array.isArray(v) ? v : {};
  } catch {
    return {};
  }
}

export function findSub(key: string): SubPick | undefined {
  const v = load()[key];
  return v && typeof v.key === "string" && typeof v.delay === "number" ? v : undefined;
}

/** Değiştiyse kaydeder. Hiç seçilmemiş ve gecikmesiz durum yeni kayıt açmaz (eski kaydı günceller). */
export function saveSub(key: string, p: SubPick) {
  const all = load(), old = all[key];
  if (old && old.key === p.key && (old.file ?? null) === (p.file ?? null) && Math.abs(old.delay - p.delay) < 0.001) return;
  if (!old && p.key === "no" && Math.abs(p.delay) < 0.001) return;
  all[key] = { file: p.file ?? null, key: p.key, delay: p.delay, at: Date.now() };
  const keep = Object.entries(all).sort((a, b) => b[1].at - a[1].at).slice(0, MAX);
  try { localStorage.setItem(KEY, JSON.stringify(Object.fromEntries(keep))); } catch { /* depolama kapalı */ }
}
