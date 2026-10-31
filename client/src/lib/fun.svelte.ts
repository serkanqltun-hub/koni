// Odanın "Ne izlesek?" ve altyazı durumu: sunucunun `sync://fun` mesajlarından
// beslenir, kokpit ve paneller aynı nesneyi okur.
import type { FunMsg, Mood, Movie, Night, NightPreview, Proposal, QueueItem, SubResult, WheelResult } from "./player";

export interface Deck {
  id: string; mood: string; by: string; items: Movie[];
  /** Benim oylarım (TMDB id → beğendim mi). */
  mine: Record<number, boolean>;
  /** Sunucuya iletilmiş oylarım (TMDB id → true). */
  sent: Record<number, boolean>;
  done: number; of: number;
  match: Movie | null;
  /** Herkes oyladı, eşleşme yok: en çok beğenilenler çarka gitti. */
  over: boolean;
}

export const fun = $state({
  moods: [] as Mood[],
  loading: null as string | null,
  deck: null as Deck | null,
  proposals: [] as Proposal[],
  result: null as WheelResult | null,
  subs: { lang: "", busy: false, items: [] as SubResult[], error: "" },
  queue: [] as QueueItem[],
  /** "Ne izlesek?" oturumu (p0.17); `null` = açık oturum yok. */
  night: null as Night | null,
  /** Yalnızca bana gelen cevaplar: önizlemeler (etiket → sonuç), öneriler, arama, künye. */
  previews: {} as Record<string, NightPreview>,
  suggest: { busy: false, items: [] as Movie[], total: 0, error: "" },
  results: { query: "", items: [] as Movie[] },
  detail: null as Movie | null,
  /** Ekrandaki tepki baloncukları (kısa ömürlü). */
  reactions: [] as Bubble[],
});

/** Tepki baloncuğu; `x`: yatay konum (%). */
export interface Bubble { id: number; emoji: string; name: string; x: number }
const BUBBLE_MS = 1800;
let bubbleId = 0;

export function resetFun() {
  fun.moods = [];
  fun.loading = null;
  fun.deck = null;
  fun.proposals = [];
  fun.result = null;
  fun.subs = { lang: "", busy: false, items: [], error: "" };
  fun.queue = [];
  fun.reactions = [];
  fun.night = null;
  fun.previews = {};
  fun.suggest = { busy: false, items: [], total: 0, error: "" };
  fun.results = { query: "", items: [] };
  fun.detail = null;
}

/** Bir sonraki oylanmamış kart (yoksa `null`). */
export function nextCard(d: Deck | null): Movie | null {
  return d?.items.find((m) => !(m.id in d.mine)) ?? null;
}

/** Mesajı duruma uygular. Kullanıcıya gösterilecek bir satır varsa döndürür. */
export function onFun(m: FunMsg): { text: string; err?: boolean } | null {
  switch (m.t) {
    case "moods":
      fun.moods = m.moods;
      return null;
    case "deck_loading":
      fun.loading = m.mood;
      return null;
    case "deck": {
      fun.loading = null;
      const prev = fun.deck?.id === m.deckId ? fun.deck : null;
      const mine: Record<number, boolean> = { ...(prev?.mine ?? {}) };
      const sent: Record<number, boolean> = { ...(prev?.sent ?? {}) };
      for (const [k, v] of Object.entries(m.mine ?? {})) {
        const id = Number(k);
        mine[id] = v;
        sent[id] = true;
      }
      fun.deck = {
        id: m.deckId, mood: m.mood, by: m.by, items: m.items, mine, sent,
        done: m.progress.done, of: m.progress.of, over: m.over,
        match: m.items.find((i) => i.id === m.match) ?? null,
      };
      return null;
    }
    case "deck_progress":
      if (fun.deck?.id === m.deckId) { fun.deck.done = m.done; fun.deck.of = m.of; fun.deck.over = m.over; }
      return null;
    case "match":
      if (fun.deck?.id === m.deckId) fun.deck.match = m.item;
      return null;
    case "discover_failed":
      fun.loading = null;
      return { text: "Keşif: " + m.message, err: true };
    case "wheel":
      fun.proposals = m.proposals;
      fun.result = m.result ?? null;
      return null;
    case "fun_error":
      return { text: m.message, err: true };
    case "subs_results":
      fun.subs = { lang: m.lang, busy: false, items: m.items ?? [], error: m.error ?? "" };
      return null;
    case "subtitle":
      return null; // App yükler
    case "queue":
      fun.queue = m.items ?? [];
      return null;
    case "reaction": {
      const b = { id: ++bubbleId, emoji: m.emoji, name: m.name, x: 8 + Math.random() * 84 };
      fun.reactions = [...fun.reactions.slice(-11), b];
      setTimeout(() => (fun.reactions = fun.reactions.filter((r) => r.id !== b.id)), BUBBLE_MS);
      return null;
    }
    case "view": // App.svelte işler
      return null;
    case "night":
      fun.night = m.closed ? null : m;
      return null;
    case "night_previews":
      for (const p of m.items) fun.previews[p.tag] = p;
      return null;
    case "night_suggestions":
      fun.suggest = { busy: false, items: m.items ?? [], total: m.total, error: m.error ?? "" };
      return null;
    case "night_results":
      fun.results = { query: m.query, items: m.items ?? [] };
      return null;
    case "night_detail":
      if (!m.error) fun.detail = m.item;
      return null;
    case "called":
      return null;
  }
}
