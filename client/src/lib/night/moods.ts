// Ne izlesek / Ne izlesem hava kutuları (TMDB tür kimlikleri). Başlıklar t() ile çevrilir.
import type { NightFilter } from "../player";

export interface MoodTile { id: string; title: string; sub: string; grad: string; f: NightFilter }
export const MOODS: MoodTile[] = [
  { id: "gerim", title: "Gerim gerim", sub: "Gerilim, gizem, son sahnede ağzın açık kalsın", grad: "linear-gradient(135deg,#3a1414,#7a1f1f)", f: { genres: [53, 9648, 80], voteMin: 6.8 } },
  { id: "hafif", title: "Kafa dağıtmalık", sub: "Hafif komedi, kafa yormaz", grad: "linear-gradient(135deg,#3a2e0e,#8a6a12)", f: { genres: [35], without: [27, 53], voteMin: 6.3 } },
  { id: "bilim", title: "Akıl yakan bilimkurgu", sub: "Büyük fikirler, büyük ekran", grad: "linear-gradient(135deg,#0e2a3a,#14526f)", f: { genres: [878], voteMin: 7 } },
  { id: "kalp", title: "Kalbe dokunsun", sub: "Romantik, biraz da hüzün", grad: "linear-gradient(135deg,#2a0e2a,#6a1a5a)", f: { genres: [10749, 18], voteMin: 7 } },
  { id: "korku", title: "Işıkları kapat", sub: "Korku; yalnız izlenmez", grad: "linear-gradient(135deg,#1a1a1a,#4a4a4a)", f: { genres: [27], voteMin: 6.2 } },
  { id: "herkes", title: "Hepimize uyar", sub: "Animasyon, her yaşa", grad: "linear-gradient(135deg,#0e3a20,#1f7a3f)", f: { genres: [16, 10751], voteMin: 7 } },
];

// Kur adımının ortak sabitleri (Ne izlesek + Ne izlesem). Etiketler t() ile çevrilir.
export const YEAR = new Date().getFullYear();
export const ERAS = [
  { id: "80", label: "80'ler", from: 1980, to: 1989 },
  { id: "90", label: "90'lar", from: 1990, to: 1999 },
  { id: "00", label: "2000'ler", from: 2000, to: 2014 },
  { id: "new", label: "Yeni", from: 2015, to: YEAR },
];
export const GENRES: [number, string][] = [
  [53, "Gerilim"], [80, "Suç"], [9648, "Gizem"], [18, "Dram"], [28, "Aksiyon"], [878, "Bilimkurgu"], [27, "Korku"], [35, "Komedi"],
  [10749, "Romantik"], [16, "Animasyon"], [99, "Belgesel"], [10752, "Savaş"], [37, "Western"], [14, "Fantastik"], [12, "Macera"], [10751, "Aile"],
];
export const LANGS: [string, string][] = [["en", "İngilizce"], ["tr", "Türkçe"], ["ko", "Korece"], ["fr", "Fransızca"], ["es", "İspanyolca"], ["ja", "Japonca"], ["it", "İtalyanca"], ["de", "Almanca"], ["hi", "Hintçe"]];
export const COUNTS = [0, 100, 500, 1000, 5000, 20000, 50000, 100000];
/** Özel filtre taslağı; `draftFilter` ile NightFilter'a çevrilir. */
export type Draft = { inc: number[]; exc: number[]; y0: number; y1: number; r0: number; r1: number; vote: number; count: number; langs: string[]; hideSeen: boolean };
export const runtimeRange = (tv: boolean) => (tv ? { min: 15, max: 90 } : { min: 60, max: 200 });
export const blankDraft = (tv: boolean): Draft => { const rt = runtimeRange(tv); return { inc: [], exc: [], y0: 1960, y1: YEAR, r0: rt.min, r1: rt.max, vote: 0, count: 0, langs: [], hideSeen: true }; };
export function draftFilter(d: Draft, tv: boolean): NightFilter {
  const rt = runtimeRange(tv);
  return {
    genres: d.inc, without: d.exc,
    yearFrom: d.y0 > 1960 ? d.y0 : undefined, yearTo: d.y1 < YEAR ? d.y1 : undefined,
    runtimeMin: d.r0 > rt.min ? d.r0 : undefined, runtimeMax: d.r1 < rt.max ? d.r1 : undefined,
    voteMin: d.vote || undefined, voteCountMin: d.count || undefined, languages: d.langs, hideSeen: d.hideSeen,
  };
}
