// İndir, sonra izle (tek başına): indirmeler bu bilgisayarda kalır, ağ olmadan açılır.
// İlerleme lobi kapalıyken de izlensin diye durum burada tutulur.
import { listen } from "@tauri-apps/api/event";
import { offlineCancel, offlineDelete, offlineDownload, offlineList, type OfflineProgress, type SavedDownload } from "./player";

export const offline = $state<{ saved: SavedDownload[]; active: OfflineProgress[] }>({ saved: [], active: [] });

let started = false;
/** İlk çağrıda olayı dinler ve listeyi çeker. */
export function initOffline() {
  if (started) return;
  started = true;
  void listen<OfflineProgress>("offline://progress", (e) => {
    const p = e.payload;
    const rest = offline.active.filter((a) => a.url !== p.url);
    offline.active = p.state === "ready" ? rest : [...rest, p];
    if (p.state === "ready") void refreshOffline();
  });
  void refreshOffline();
}

export async function refreshOffline() {
  offline.saved = await offlineList().catch(() => offline.saved);
}

export async function startOffline(url: string) {
  if (offline.active.some((a) => a.url === url && a.state !== "failed")) return;
  offline.active = [...offline.active.filter((a) => a.url !== url), { url, state: "queued", pct: 0, speed: 0, eta: 0, error: "" }];
  try {
    await offlineDownload(url);
  } catch (e) {
    offline.active = offline.active.filter((a) => a.url !== url);
    throw e;
  }
}

/** Süren indirmeyi durdurur ya da başarısız olanı listeden kaldırır. */
export async function cancelOffline(url: string) {
  offline.active = offline.active.filter((a) => a.url !== url);
  await offlineCancel(url);
}

export async function deleteOffline(key: string) {
  await offlineDelete(key);
  offline.saved = offline.saved.filter((s) => s.key !== key);
}
