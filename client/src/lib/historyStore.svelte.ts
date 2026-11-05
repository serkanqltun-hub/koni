// Odanın ortak geçmişi (p0.28): sunucu odaya girince `snapshot.history`, sonra her değişiklikte
// `history` mesajı yollar (en yeni önce, en fazla 20). Oda kapanınca/ayrılınca silinir.
import { listen } from "@tauri-apps/api/event";
import { syncReopen, type ServerMsg, type HistoryItem } from "./player";

export const roomHistory = $state<{ items: HistoryItem[] }>({ items: [] });

void listen<ServerMsg>("sync://server", (e) => {
  const m = e.payload;
  if (m.t === "joined") roomHistory.items = m.snapshot.history ?? [];
  else if (m.t === "history") roomHistory.items = m.items ?? [];
});
void listen<string | null>("sync://closed", () => (roomHistory.items = []));

export const reopen = (h: HistoryItem) => syncReopen(h.media);
