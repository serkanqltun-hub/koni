import { invoke } from "@tauri-apps/api/core";
import { EN, PATTERNS } from "./i18n/index";

// İki dil: Türkçe (kaynak) ve İngilizce. Anahtar Türkçe metnin kendisidir:
// t("Odadan ayrıl") İngilizcede sözlükten gelir, bulunamazsa Türkçe kalır.
// Metni gösterirken çevir (şablonda ya da $derived içinde); modül sabitlerinde
// Türkçe tut, ekranda t(...) ile çevir ki dil değişince anında güncellensin.
export type Lang = "tr" | "en";

const KEY = "koni.lang";

function initial(): Lang {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "tr" || v === "en") return v;
  } catch { /* depolama kapalı */ }
  return "en"; // varsayılan İngilizce; Türkçe Ayarlar'dan ya da giriş ekranından seçilir
}

export const i18n = $state<{ lang: Lang }>({ lang: initial() });

function apply(l: Lang) {
  document.documentElement.lang = l;
  invoke("set_lang", { en: l === "en" }).catch(() => {});
}
apply(i18n.lang);

export function setLang(l: Lang) {
  i18n.lang = l;
  try { localStorage.setItem(KEY, l); } catch { /* depolama kapalı */ }
  apply(l);
}

/** Tarih/sayı biçimi ve büyük harf için yerel ayar. */
export function locale(): string {
  return i18n.lang === "en" ? "en-US" : "tr-TR";
}

/** Arayüz metni. `{ad}` yer tutucuları vars ile doldurulur: t("{n} kişi", { n: 3 }). */
export function t(tr: string, vars?: Record<string, string | number>): string {
  let s = i18n.lang === "en" ? (EN[tr] ?? tr) : tr;
  if (vars) {
    s = s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
    // İngilizce tekil/çoğul: "{n} {person|people}" → n 1 ise ilk biçim.
    s = s.replace(/\{([^{}|]*)\|([^{}|]*)\}/g, (_, one, many) => (Number(vars.n) === 1 ? one : many));
  }
  return s;
}

/** Sunucudan ya da Rust'tan gelen hazır Türkçe metin (etkinlik, hata): önce tam
 *  eşleşme, sonra kalıplar (i18n/patterns.ts). Bilinmeyen metin olduğu gibi kalır. */
export function tx(text: string): string {
  if (i18n.lang !== "en" || !text) return text;
  const hit = EN[text];
  if (hit) return hit;
  for (const [re, en] of PATTERNS) {
    // Yakalanan parçalar da sözlükte varsa çevrilir (ör. deste adı, mod adı).
    const m = text.match(re);
    if (m) return en.replace(/\$(\d)/g, (_, i) => { const g = m[Number(i)] ?? ""; return EN[g] ?? g; });
  }
  return text;
}
