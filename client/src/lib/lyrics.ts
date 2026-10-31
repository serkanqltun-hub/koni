// LRCLIB senkron sözleri (docs/design/media.md §6.3). Açık API, anahtar yok.
// Arama başlıktan yapılır: YouTube başlıklarındaki "(Official Video)" vb. atılır.

export interface LyricLine { t: number; text: string }
export interface Lyrics { synced: LyricLine[]; plain: string; name: string }

/** "Sanatçı - Şarkı (Official Video) [4K]" → "Sanatçı - Şarkı" */
export function cleanTitle(title: string): string {
  return title
    .replace(/\.[a-z0-9]{2,4}$/i, "")
    .replace(/[([{【][^)\]}】]*[)\]}】]/g, " ")
    .replace(/\b(official|music|video|audio|lyrics?|hd|4k|mv|clip|klip)\b/gi, " ")
    .replace(/[_|]+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

/** `[mm:ss.xx]` satırlarını ayrıştırır; bir satırda birden çok zaman olabilir. */
export function parseLrc(lrc: string): LyricLine[] {
  const out: LyricLine[] = [];
  for (const raw of lrc.split(/\r?\n/)) {
    const stamps = [...raw.matchAll(/\[(\d+):(\d+(?:\.\d+)?)\]/g)];
    if (!stamps.length) continue;
    const text = raw.replace(/\[[^\]]*\]/g, "").trim();
    for (const s of stamps) out.push({ t: Number(s[1]) * 60 + Number(s[2]), text });
  }
  return out.sort((a, b) => a.t - b.t);
}

/** Şu anki satırın indeksi (`pos` saniye); ilk satırdan önce -1. */
export function lineAt(lines: LyricLine[], pos: number): number {
  let lo = 0, hi = lines.length - 1, ans = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (lines[mid].t <= pos) { ans = mid; lo = mid + 1; } else hi = mid - 1;
  }
  return ans;
}

interface LrcLibHit { trackName: string; artistName: string; syncedLyrics: string | null; plainLyrics: string | null; instrumental: boolean }

/** Başlıkla arar; senkron sözü olan ilk sonucu, yoksa düz sözü döndürür. */
export async function fetchLyrics(title: string, signal?: AbortSignal): Promise<Lyrics | null> {
  const q = cleanTitle(title);
  if (!q) return null;
  const res = await fetch(`https://lrclib.net/api/search?q=${encodeURIComponent(q)}`, { signal });
  if (!res.ok) throw new Error(`LRCLIB: HTTP ${res.status}`);
  const hits = (await res.json()) as LrcLibHit[];
  const hit = hits.find((h) => h.syncedLyrics) ?? hits.find((h) => h.plainLyrics);
  if (!hit) return null;
  return {
    synced: hit.syncedLyrics ? parseLrc(hit.syncedLyrics) : [],
    plain: hit.plainLyrics ?? "",
    name: `${hit.artistName} – ${hit.trackName}`,
  };
}
