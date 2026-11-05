// Tek başına video sırası (Keşfet → "Sıraya ekle"). Odada sıra Santral'daki ortak kuyruktur
// (fun.queue); burada yalnızca solo için yerel liste tutulur. Bu bilgisayarda saklanır.
export type VideoItem = { src: string; title: string };

const KEY = "koni.videoQueue";
const MAX = 50;

const load = (): VideoItem[] => {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(v) ? v.filter((x) => x && typeof x.src === "string").slice(0, MAX) : [];
  } catch {
    return [];
  }
};

export const vq = $state({ items: load() });

function save() {
  try { localStorage.setItem(KEY, JSON.stringify(vq.items)); } catch {}
}

/** false: zaten sırada ya da sıra dolu. */
export function vqAdd(src: string, title: string): boolean {
  if (vq.items.length >= MAX || vq.items.some((x) => x.src === src)) return false;
  vq.items = [...vq.items, { src, title: title || src }];
  save();
  return true;
}

export function vqRemove(src: string) {
  vq.items = vq.items.filter((x) => x.src !== src);
  save();
}

export function vqShift(): VideoItem | undefined {
  const [first, ...rest] = vq.items;
  vq.items = rest;
  save();
  return first;
}
