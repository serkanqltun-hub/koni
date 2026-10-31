/** Bu bilgisayarda girilen son odalar (yalnızca yerel; sunucuya gitmez). */
export interface RecentRoom { invite: string; name: string; at: number; members: number }

const KEY = "koni.recent";

export function loadRecent(): RecentRoom[] {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(v) ? v.filter((r) => r && typeof r.invite === "string" && typeof r.name === "string") : [];
  } catch {
    return [];
  }
}

/** Odayı listenin başına koyar, en fazla 5 oda tutar. */
export function rememberRoom(r: RecentRoom): RecentRoom[] {
  const list = [r, ...loadRecent().filter((x) => x.invite !== r.invite)].slice(0, 5);
  try { localStorage.setItem(KEY, JSON.stringify(list)); } catch { /* depolama kapalı */ }
  return list;
}

/** Kapanmış odayı listeden siler. */
export function forgetRoom(invite: string): RecentRoom[] {
  const list = loadRecent().filter((x) => x.invite !== invite);
  try { localStorage.setItem(KEY, JSON.stringify(list)); } catch { /* depolama kapalı */ }
  return list;
}
