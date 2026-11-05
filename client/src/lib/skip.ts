// İntro / özet atla (T28): dizi başına paylaşılan işaretler (PROTOCOL §3.0n).
// Bir kişi "intro başı / sonu" işaretler, sunucudaki herkes sonraki bölümlerde "İntro'yu atla" görür.
import { skipClear, skipGet, skipSet, type SkipMark } from "./player";

const AUTO_KEY = "koni.autoSkip";

/** Ayar: intro / özet gelince sormadan atla. */
export function autoSkip(): boolean {
  try { return localStorage.getItem(AUTO_KEY) === "1"; } catch { return false; }
}
export function setAutoSkip(v: boolean) {
  try { localStorage.setItem(AUTO_KEY, v ? "1" : "0"); } catch { /* depolama kapalı */ }
}

/** Dizi (`tt…`) → işaretler. Uygulama açıkken bir kez çekilir, yazınca tazelenir. */
const cache = new Map<string, SkipMark[]>();

export async function loadMarks(url: string, show: string): Promise<SkipMark[]> {
  const hit = cache.get(show);
  if (hit) return hit;
  const r = await skipGet(url, show).catch(() => null);
  const marks = r?.marks ?? [];
  if (r) cache.set(show, marks);
  return marks;
}

/** Önce bölümün sezonu, yoksa tüm sezonlar (`season: 0`). */
export function markFor(marks: SkipMark[], season: number): SkipMark | null {
  return marks.find((m) => m.season === season) ?? marks.find((m) => m.season === 0) ?? null;
}

/** Atlanacak aralık: `pos` özetin ya da intro'nun içindeyse nereye atlanacağı. */
export interface SkipSpan { kind: "recap" | "intro"; from: number; to: number }
export function spanAt(m: SkipMark | null, pos: number): SkipSpan | null {
  if (!m) return null;
  // Son 1 sn'de düğme çıkmasın: zaten bitiyor.
  if (m.recapEnd && pos < m.recapEnd - 1) return { kind: "recap", from: 0, to: m.recapEnd };
  if (m.introEnd > m.introStart && pos >= m.introStart && pos < m.introEnd - 1) return { kind: "intro", from: m.introStart, to: m.introEnd };
  return null;
}

/** İşareti yazar; `season` 0 = tüm sezonlar. Mevcut işaretin diğer yarısı korunur. */
export async function saveMark(
  url: string, show: string, season: number,
  part: { introStart: number; introEnd: number } | { recapEnd: number },
): Promise<SkipMark[]> {
  const old = (cache.get(show) ?? []).find((m) => m.season === season);
  const intro = "introStart" in part ? part : { introStart: old?.introStart ?? 0, introEnd: old?.introEnd ?? 0 };
  const recap = "recapEnd" in part ? part.recapEnd : old?.recapEnd;
  const r = await skipSet(url, show, season, round(intro.introStart), round(intro.introEnd), recap ? round(recap) : undefined);
  const marks = r?.marks ?? [];
  cache.set(show, marks);
  return marks;
}

export async function clearMark(url: string, show: string, season: number): Promise<SkipMark[]> {
  const r = await skipClear(url, show, season);
  const marks = r?.marks ?? [];
  cache.set(show, marks);
  return marks;
}

const round = (s: number) => Math.round(Math.max(0, Math.min(3600, s)) * 10) / 10;
