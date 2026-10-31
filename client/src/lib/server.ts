// Santral adresi. Herkes kendi sunucusunu kurabilir (docs/SELF-HOSTING.md); varsayılan
// derlemede VITE_KONI_SERVER ile gelir (CI: repo değişkeni KONI_SERVER). Boşsa giriş
// ekranı sunucu adresini sorar.
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

/** Üye davet linki; varsayılan sunucu dışındaysa adres de linke eklenir. */
export function inviteLink(code: string, server: string): string {
  return server === DEFAULT_SERVER ? `koni://invite/${code}` : `koni://invite/${code}?s=${encodeURIComponent(serverLabel(server))}`;
}
