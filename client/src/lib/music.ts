// Müzik ekranı yardımcıları: kapak, sanatçı/şarkı ayrımı, kapaktan baskın renk.
import { cleanTitle } from "./lyrics";

/** YouTube linkinden video kimliği. */
function ytId(src: string): string | null {
  const m = src.match(/(?:youtube\.com\/(?:watch\?(?:.*&)?v=|shorts\/|embed\/|live\/)|youtu\.be\/|music\.youtube\.com\/watch\?(?:.*&)?v=)([\w-]{11})/i);
  return m ? m[1] : null;
}

/** Linkten bilinen kapak (şimdilik YouTube); yoksa `null`. */
export function artFor(src: string | null | undefined): string | null {
  const id = src ? ytId(src) : null;
  return id ? `https://i.ytimg.com/vi/${id}/hqdefault.jpg` : null;
}

/** "Sanatçı - Şarkı (Official Video)" → { song, artist }. */
export function splitTitle(title: string): { song: string; artist: string } {
  const t = cleanTitle(title) || title;
  const m = t.match(/^(.+?)\s+[-–—]\s+(.+)$/);
  return m ? { artist: m[1].trim(), song: m[2].trim() } : { artist: "", song: t };
}

/** Başlıktan sabit renk tonu (kapak yoksa ya da okunamazsa). */
export function hueOf(s: string): number {
  let h = 0;
  for (const c of s) h = (h * 31 + c.codePointAt(0)!) % 360;
  return h;
}

/** Kapağın baskın, doygun rengi ("r, g, b"); okunamazsa `null`. */
export function dominant(url: string): Promise<string | null> {
  return new Promise((resolve) => {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onerror = () => resolve(null);
    img.onload = () => {
      try {
        const c = document.createElement("canvas");
        c.width = c.height = 24;
        const g = c.getContext("2d")!;
        g.drawImage(img, 0, 0, 24, 24);
        const d = g.getImageData(0, 0, 24, 24).data;
        // Doygun ve çok karanlık olmayan piksellerin ortalaması; yoksa genel ortalama.
        const px: { c: number[]; s: number }[] = [];
        for (let i = 0; i < d.length; i += 4) {
          const c = [d[i], d[i + 1], d[i + 2]];
          const max = Math.max(...c), min = Math.min(...c);
          px.push({ c, s: (max - min) * (max > 50 ? 1 : 0.2) });
        }
        const best = Math.max(...px.map((p) => p.s));
        const pick = best >= 40 ? px.filter((p) => p.s >= best * 0.6) : px;
        const avg = [0, 1, 2].map((k) => Math.round(pick.reduce((n, p) => n + p.c[k], 0) / pick.length));
        resolve(avg.join(", "));
      } catch {
        resolve(null); // CORS yok: tuval okunamaz
      }
    };
    img.src = url;
  });
}
