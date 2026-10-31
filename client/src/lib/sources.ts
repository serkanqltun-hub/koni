// Film & Dizi kaynak satırı: addon'un serbest metninden kalite, boyut, kaynak sayısı ve
// Türkçe altyazı/dublaj ipuçlarını okur (yaygın addon biçimi; tutmazsa alanlar boş kalır).
import type { Source } from "./player";

export type Quality = "4K" | "2K" | "1080p" | "720p" | "SD";
export interface SourceInfo {
  src: Source;
  /** İlk satır: sürüm adı. */
  release: string;
  quality: Quality;
  size: string | null;
  seeds: number | null;
  /** BluRay, WEB-DL, HDR … */
  tags: string[];
  trSub: boolean;
  trDub: boolean;
  /** Oynatılabilir mi (infoHash ya da hazır debrid URL var mı). */
  ok: boolean;
}

const QSCORE: Record<Quality, number> = { "1080p": 4, "4K": 3, "2K": 2, "720p": 1, SD: 0 };

const isCachedUrl = (s: Source): boolean =>
  Boolean(s.url) && !/\bdownload\]|⏳|⬇️|not.?cached/i.test(`${s.name}\n${s.title}`);

export function parseSource(s: Source): SourceInfo {
  const all = `${s.name}\n${s.title}`;
  const release = s.title.split("\n").map((l) => l.trim()).find(Boolean) ?? s.name;
  const quality: Quality = /2160p|\b4k\b|\buhd\b/i.test(all) ? "4K" : /1440p|\b2k\b/i.test(all) ? "2K" : /1080p/i.test(all) ? "1080p" : /720p/i.test(all) ? "720p" : "SD";
  const size = all.match(/(\d+(?:[.,]\d+)?)\s*(GB|MB)/i);
  const seeds = all.match(/👤\s*(\d+)/);
  const tags = [...new Set((all.match(/\b(REMUX|BluRay|BDRip|BRRip|WEB-?DL|WEBRip|HDTV|HDR10\+?|HDR|DV|Atmos)\b/gi) ?? []).map((t) => t.replace(/^webdl$/i, "WEB-DL")))].slice(0, 3);
  const trSub = /altyaz|tr[ ._-]?sub|turkish[ ._-]?sub/i.test(all);
  const trDub = /🇹🇷|dublaj|tr[ ._-]?dub|\bturkish\b|türkçe/i.test(all.replace(/turkish[ ._-]?sub|türkçe[ ._-]?altyaz\S*/gi, ""));
  return {
    src: s, release, quality, tags, trSub, trDub, ok: Boolean(s.hash || s.url),
    size: size ? `${size[1].replace(".", ",")} ${size[2].toUpperCase()}` : null,
    seeds: seeds ? Number(seeds[1]) : null,
  };
}

/** Önerilen sıra: oynatılabilir olanlar, hazır debrid URL'leri, sonra 1080p > 4K > 720p,
 *  sonra en çok eş (önbellekte olma ihtimali en yüksek olan). */
export function rankSources(list: Source[]): SourceInfo[] {
  return list.map(parseSource).sort((a, b) =>
    Number(b.ok) - Number(a.ok)
    || Number(isCachedUrl(b.src)) - Number(isCachedUrl(a.src))
    || QSCORE[b.quality] - QSCORE[a.quality]
    || (b.seeds ?? 0) - (a.seeds ?? 0));
}

/** Otomatik kaynak seçiminin basamakları: en yüksek kaliteden aşağı. */
export const LADDER = ["4K", "2K", "1080p", "720p"] as const;
export type Rung = (typeof LADDER)[number];

/** Denenecek sıra: oynatılabilir kaynaklar, `top` kalitesinden aşağı inerek, her
 *  basamakta hazır debrid URL'li ve en çok eşli olanlar önce. */
export function huntOrder(list: SourceInfo[], top: Rung): SourceInfo[] {
  const rungs = [...LADDER.slice(LADDER.indexOf(top)), "SD"] as readonly Quality[];
  return list
    .filter((s) => s.ok && rungs.includes(s.quality))
    .sort((a, b) =>
      rungs.indexOf(a.quality) - rungs.indexOf(b.quality)
      || Number(isCachedUrl(b.src)) - Number(isCachedUrl(a.src))
      || (b.seeds ?? 0) - (a.seeds ?? 0));
}

/** Real-Debrid hatasını sayaca çevirir; `slow`: istek sınırı (bekleyip devam), `stop`: anahtar/premium, aramayı keser. */
export function failKind(err: string): "cache" | "bad" | "slow" | "stop" {
  const m = err.toLowerCase();
  if (m.includes("çok fazla istek") || m.includes("too many requests")) return "slow";
  if (m.includes("anahtar") || m.includes("api key") || m.includes("premium")) return "stop";
  if (m.includes("addon'unu ekle") || m.includes("add your own addon")) return "stop";
  if (m.includes("önbelle") || m.includes("not cached")) return "cache";
  return "bad";
}
