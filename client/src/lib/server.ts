// Santral adresi. Herkes kendi sunucusunu kurabilir (docs/SELF-HOSTING.md); varsayılan
// derlemede VITE_KONI_SERVER ile verilebilir. Resmi derlemede boştur: adres davet
// linkinden (`?s=`) gelir ya da giriş ekranında yazılır.
export const DEFAULT_SERVER: string = import.meta.env.VITE_KONI_SERVER || "";

/** "koni.ornek.com", "https://koni.ornek.com", "localhost:8080" → `wss://…/ws` / `ws://…/ws`. */
export function normServer(input: string): string {
  let t = input.trim().replace(/\/+$/, "");
  if (!t) return "";
  if (/^https?:\/\//i.test(t)) t = t.replace(/^http/i, "ws");
  if (!/^wss?:\/\//i.test(t)) {
    const local = /^(localhost|127\.|10\.|192\.168\.|\[?::1)/i.test(t);
    t = `${local ? "ws" : "wss"}://${t}`;
  }
  return /^wss?:\/\/[^/]+$/i.test(t) ? `${t}/ws` : t;
}

/** Ekranda gösterilen kısa ad: `wss://koni.ornek.com/ws` → `koni.ornek.com`. */
export function serverLabel(url: string): string {
  return url.replace(/^wss?:\/\//i, "").replace(/\/ws$/, "");
}

/** Üye davet linki; sunucu adresini de taşır, linke tıklayan hiçbir şey yazmaz. */
export function inviteLink(code: string, server: string): string {
  return `koni://invite/${code}?s=${encodeURIComponent(serverLabel(server))}`;
}

/** Sunucuya ulaşılıyor mu? `/healthz`'e bakar (yanıt gövdesi okunmaz, yalnızca bağlantı). */
export async function probe(url: string): Promise<boolean> {
  const http = url.replace(/^ws/i, "http").replace(/\/ws$/, "");
  try {
    await fetch(`${http}/healthz`, { mode: "no-cors", cache: "no-store", signal: AbortSignal.timeout(5000) });
    return true;
  } catch {
    return false;
  }
}
