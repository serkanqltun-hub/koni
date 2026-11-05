// Solo'da sıradaki bölüm: dizi bölümü biterken sonraki bölüm ve kaynakları önceden bulunur,
// bölüm bitince arayüzsüz avla (lib/hunt.ts) açılır. Odada kullanılmaz.
import { addonStreams, catalogEpisodes, type Source } from "./player";
import { rankSources, type SourceInfo } from "./sources";
import { huntSilent } from "./hunt";

const STOP_KEY = "koni.nextEpStop";

/** Oynayan bölüm: medya `file` alanı `series:tt…:s:e|…`. `key`: sezon paketinde bölümler aynı
 *  `src`'yi paylaşır, o yüzden `src` + `file`. */
export interface EpRef { key: string; show: string; season: number; episode: number }
export function epRef(src: string, file: string | undefined): EpRef | null {
  const m = file?.match(/^series:(tt\d+):(\d+):(\d+)/);
  return m ? { key: `${src}|${file}`, show: m[1], season: +m[2], episode: +m[3] } : null;
}
const code = (s: number, e: number) => `S${String(s).padStart(2, "0")}E${String(e).padStart(2, "0")}`;

export const nextEp = $state({
  /** Hangi bölüm için hazırlandı (`EpRef.key`). */
  for: "",
  /** Sonraki bölüm: başlık ("Ad S01E05"), kod, Cinemeta kimliği; `last`: sezon/dizi bitti. */
  title: "",
  code: "",
  id: "",
  last: false,
  sources: null as SourceInfo[] | null,
  /** Bu bölüm için iptal edildi (ya da kaynak bulunamadı): kendiliğinden açılmaz. */
  cancelled: "",
  busy: false,
  /** Ayar: "Bölüm bitince dur". */
  stop: (() => { try { return localStorage.getItem(STOP_KEY) === "1"; } catch { return false; } })(),
});
let token = 0;

export function setStop(v: boolean) {
  nextEp.stop = v;
  try { localStorage.setItem(STOP_KEY, v ? "1" : "0"); } catch { /* depolama kapalı */ }
}

/** Son 60 sn: sonraki bölümü ve kaynak listesini çek. `title`: oynayan "Ad S01E04". */
export async function prefetchNext(ep: EpRef, title: string) {
  const my = ++token;
  Object.assign(nextEp, { for: ep.key, title: "", code: "", id: "", last: false, sources: null, busy: false });
  const show = title.match(/^(.*?)\s+S\d{2}E\d{2}$/)?.[1] ?? "";
  try {
    const list = await catalogEpisodes(ep.show);
    if (my !== token) return;
    const n = list
      .filter((e) => e.season > 0)
      .sort((a, b) => a.season - b.season || a.episode - b.episode)
      .find((e) => e.season > ep.season || (e.season === ep.season && e.episode > ep.episode));
    if (!n) { nextEp.last = true; return; }
    const c = code(n.season, n.episode);
    Object.assign(nextEp, { code: c, title: show ? `${show} ${c}` : c, id: n.id });
    const r = await addonStreams("series", n.id);
    if (my === token) nextEp.sources = rankSources(r.sources);
  } catch {
    // Bitişte yeniden denenir.
  }
}

/** Sessiz av + aç. `alive`: oynayan bölüm hâlâ aynı mı. Bulamazsa `false`. */
export async function playNext(alive: () => boolean, open: (s: Source, title: string) => Promise<void>): Promise<boolean> {
  if (!nextEp.id || nextEp.busy) return false;
  const my = token;
  const ok = () => my === token && alive();
  const src = nextEp.for;
  nextEp.busy = true;
  try {
    let list = nextEp.sources;
    if (!list) {
      list = rankSources((await addonStreams("series", nextEp.id)).sources);
      if (ok()) nextEp.sources = list;
    }
    const s = ok() ? await huntSilent(list, ok) : null;
    if (!ok()) return true;
    if (!s) { nextEp.cancelled = src; return false; }
    await open(s.src, nextEp.title);
    // Açılamadıysa (hata etkinlikte) aynı bölüm sürüyor: tekrar denenmez.
    if (alive()) { nextEp.cancelled = src; return false; }
    return true;
  } catch {
    if (ok()) nextEp.cancelled = src;
    return !ok();
  } finally {
    if (my === token) nextEp.busy = false;
  }
}

export function cancelNext() {
  token++;
  nextEp.cancelled = nextEp.for;
  nextEp.busy = false;
}
