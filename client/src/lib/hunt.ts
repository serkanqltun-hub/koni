// Otomatik kaynak avı: sıradaki kaynakları Real-Debrid'de dener, açılan ilkini bulur.
// SourceHunt (arayüzlü) ve solo'da sıradaki bölüm (sessiz av) aynı döngüyü kullanır.
import { debridCheck } from "./player";
import { LADDER, failKind, huntOrder, type Rung, type SourceInfo } from "./sources";

/** SourceHunt'taki "En iyi kalite" seçimi (localStorage). */
export const TOP_KEY = "koni.topQuality";
export function loadTop(): Rung {
  try { const v = localStorage.getItem(TOP_KEY); return LADDER.includes(v as Rung) ? (v as Rung) : "4K"; } catch { return "4K"; }
}
/** Real-Debrid istek sınırı: beklenip kaldığı yerden devam edilir (sn). */
export const SLOW_WAIT = 30;

export type HuntStep =
  | { at: "ok"; i: number }
  /** İstek sınırı: `i` yeniden denenecek. */
  | { at: "slow"; i: number }
  /** Anahtar/premium/addon hatası: av durur. */
  | { at: "stop"; i: number; err: string }
  | { at: "end" }
  /** İptal edildi (`alive` false döndü). */
  | { at: "gone" };

/** `from`'dan başlayıp sırayla dener; ilk sonuçta (bulundu, yavaşla, dur, bitti) döner. */
export async function huntFrom(order: SourceInfo[], from: number, o: {
  alive: () => boolean;
  onTry?: (i: number) => void;
  onFail?: (i: number, kind: "cache" | "bad" | "stop") => void;
}): Promise<HuntStep> {
  for (let i = from; i < order.length; i++) {
    o.onTry?.(i);
    try {
      await debridCheck(order[i].src);
      if (!o.alive()) return { at: "gone" };
      return { at: "ok", i };
    } catch (e) {
      if (!o.alive()) return { at: "gone" };
      const k = failKind(String(e));
      if (k === "slow") return { at: "slow", i };
      o.onFail?.(i, k);
      if (k === "stop") return { at: "stop", i, err: String(e) };
    }
  }
  return { at: "end" };
}

/** Arayüzsüz av: `top`'tan aşağı inerek açılan ilk kaynak; istek sınırında bekler. */
export async function huntSilent(list: SourceInfo[], alive: () => boolean, top: Rung = loadTop()): Promise<SourceInfo | null> {
  const order = huntOrder(list, top);
  let from = 0;
  for (;;) {
    const r = await huntFrom(order, from, { alive });
    if (r.at === "ok") return order[r.i];
    if (r.at !== "slow") return null;
    await new Promise((res) => setTimeout(res, SLOW_WAIT * 1000));
    if (!alive()) return null;
    from = r.i;
  }
}
