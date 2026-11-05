// Mola (p0.28): sunucudaki `break` mesajlarından beslenir; mola sahnesi ve düğme aynı nesneyi okur.
import type { BreakMsg } from "./player";

export interface BreakState {
  by: string; min: number; until: number; ready: string[]; waiting: string[];
  /** Herkes hazır oldu; oda geri sayıma geçti, sahne sayım bitene kadar kalır. */
  ending: boolean;
  /** Bitiş mesajından sonra geri sayımın başlamasını bekleme payı. */
  grace: boolean;
}

export const brk = $state<{ cur: BreakState | null }>({ cur: null });

let graceTimer: ReturnType<typeof setTimeout> | undefined;

export function onBreak(m: BreakMsg) {
  clearTimeout(graceTimer);
  if (m.end) {
    const cur = brk.cur;
    if (cur && m.cause === "ready") {
      brk.cur = { ...cur, ending: true, grace: true };
      graceTimer = setTimeout(() => { if (brk.cur) brk.cur = { ...brk.cur, grace: false }; }, 2000);
    } else brk.cur = null;
    return;
  }
  brk.cur = { by: m.by, min: m.min, until: m.until, ready: m.ready ?? [], waiting: m.waiting ?? [], ending: false, grace: false };
}

export function resetBreak() {
  clearTimeout(graceTimer);
  brk.cur = null;
}

/** Sahne görünsün mü: mola sürerken, ya da "hazırız" sonrası 3-2-1 bitene kadar. */
export const breakVisible = (b: BreakState | null, countdown: number) => !!b && (!b.ending || countdown > 0 || b.grace);
