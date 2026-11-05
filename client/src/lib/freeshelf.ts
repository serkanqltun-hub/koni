// "Ücretsiz & klasik" rafı: Internet Archive'daki kamu malı filmler (istemciden, CORS açık)
// ve ücretsiz, resmi film kanalları. Film doğrudan MP4 olarak oynatıcıda açılır.

export type FreeFilm = { id: string; title: string; year: number | null; mins: number | null; thumb: string };

/** Seçilmiş kamu malı klasikler (Internet Archive kimlikleri; raf bu sırayla dizilir). */
const IDS = [
  "his_girl_friday",
  "Night.Of.The.Living.Dead_1080p",
  "The_General_Buster_Keaton",
  "Nosferatu_most_complete_version_93_mins.",
  "TheStranger_0",
  "Detour",
  "MyManGodfrey1936",
  "CarnivalofSouls",
  "ThePhantomoftheOpera",
  "royal_wedding",
  "The_House_On_Haunted_Hill",
  "DasKabinettdesDoktorCaligariTheCabinetofDrCaligari",
  "charlie_chaplin_film_fest",
  "TheLittleShopOfHorrors1960_765",
];

/** Ücretsiz, resmi film kanalları (Keşfet'te sitenin içinde açılır). */
export const FREE_CHANNELS = [
  { name: "FilmRise Movies", url: "https://www.youtube.com/@FilmRiseMovies/videos" },
  { name: "Popcornflix", url: "https://www.youtube.com/@Popcornflix/videos" },
];

/** Daha fazlası: arşivin uzun metraj koleksiyonu. */
export const ARCHIVE_MORE = "https://archive.org/details/feature_films";

const API = "https://archive.org";

/** "1:31:44", "1:33.13", "67 min" → dakika. */
function minutes(r: unknown): number | null {
  const s = String(r ?? "").trim();
  const min = s.match(/^(\d+)\s*min/i);
  if (min) return +min[1];
  const p = s.match(/^(\d+):(\d+)(?:[:.](\d+))?$/);
  if (!p) return null;
  return p[3] != null ? +p[1] * 60 + +p[2] : +p[1];
}

let cache: Promise<FreeFilm[]> | null = null;

/** Raf listesi (bir kez çekilir; hata olursa sonraki çağrı yeniden dener). */
export function freeFilms(): Promise<FreeFilm[]> {
  cache ??= (async () => {
    const q = `identifier:(${IDS.map((i) => `"${i}"`).join(" OR ")})`;
    const url = `${API}/advancedsearch.php?q=${encodeURIComponent(q)}&fl[]=identifier&fl[]=title&fl[]=year&fl[]=runtime&rows=${IDS.length}&output=json`;
    const res = await fetch(url, { signal: AbortSignal.timeout(12000) });
    if (!res.ok) throw new Error(`Internet Archive ${res.status}`);
    const docs: { identifier: string; title?: string; year?: number | string; runtime?: string }[] = (await res.json()).response?.docs ?? [];
    return IDS.flatMap((id) => {
      const d = docs.find((x) => x.identifier === id);
      if (!d) return [];
      const raw = d.title ?? id;
      const year = +(d.year ?? 0) || +(raw.match(/\b(1[89]\d\d|20\d\d)\b/)?.[1] ?? 0) || null;
      const title = raw.replace(/\s*[-(]\s*\d{4}\s*\)?\s*$/, "").replace(/\s*\(.*?\)\s*$/, "").trim();
      return [{ id, title, year, mins: minutes(d.runtime), thumb: `${API}/services/img/${encodeURIComponent(id)}` }];
    });
  })().catch((e) => { cache = null; throw e; });
  return cache;
}

/** Filmin doğrudan MP4 adresi: önce H.264, yoksa herhangi bir MP4 (en büyüğü). */
export async function freeFilmUrl(id: string): Promise<string> {
  const res = await fetch(`${API}/metadata/${encodeURIComponent(id)}/files`, { signal: AbortSignal.timeout(12000) });
  if (!res.ok) throw new Error(`Internet Archive ${res.status}`);
  const files: { name: string; format?: string; size?: string }[] = (await res.json()).result ?? [];
  const mp4 = files.filter((f) => /\.mp4$/i.test(f.name));
  const best = (l: typeof mp4) => l.sort((a, b) => +(b.size ?? 0) - +(a.size ?? 0))[0];
  const f = best(mp4.filter((x) => x.format === "h.264")) ?? best(mp4);
  if (!f) throw new Error("Bu filmin oynatılabilir dosyası yok");
  return `${API}/download/${encodeURIComponent(id)}/${f.name.split("/").map(encodeURIComponent).join("/")}`;
}
