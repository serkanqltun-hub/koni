<script lang="ts">
  // Seçilen dizi bölümü (`series:tt…:s:e`) ya da film (`movie:tt…`) üzerinden Türkçe /
  // İngilizce altyazı araması. Önce doğrudan bölüm/film kimliğiyle addon'lardan (ve
  // Stremio OpenSubtitles v3'ten) aranır; bulunamazsa odada Santral OpenSubtitles
  // aramasına temizlenmiş başlık ve IMDb kimliğiyle düşer.
  import { fun } from "./fun.svelte";
  import { t, tx } from "./i18n.svelte";
  import { subsApplyUrl, subsHash, subsSearchEpisode, syncFun, type SubResult } from "./player";

  let {
    inRoom = true,
    canControl,
    title,
    file = "",
    onAdded,
    onLog,
    onError,
  }: {
    inRoom?: boolean;
    canControl: boolean;
    title: string;
    file?: string;
    onAdded?: () => void;
    onLog?: (msg: string) => void;
    onError: (msg: string) => void;
  } = $props();

  /** Sonuç gelince en iyisini otomatik seç (tek tık). */
  let auto = $state<string | null>(null);

  /** Torrent/dosya adından temiz dizi+bölüm ("Orphan Black S01E01") ya da film adını ve IMDb kimliğini çıkarır. */
  function parseQuery(rawTitle: string, rawFile: string): { query: string; imdb?: string } {
    let imdb: string | undefined;
    let epFromMeta = "";

    const metaPart = rawFile.includes("|") ? rawFile.split("|")[0] : "";
    if (metaPart.startsWith("movie:tt")) {
      imdb = metaPart.slice("movie:".length).trim();
    } else if (metaPart.startsWith("series:tt")) {
      const parts = metaPart.slice("series:".length).split(":");
      if (parts.length === 3) {
        const s = Number(parts[1]);
        const e = Number(parts[2]);
        if (s >= 0 && e > 0) {
          epFromMeta = `S${String(s).padStart(2, "0")}E${String(e).padStart(2, "0")}`;
        }
      }
    }

    let s = rawTitle
      .replace(/\.[a-z0-9]{2,4}$/i, "")
      .replace(/[._[\]()]+/g, " ")
      .replace(/\s+/g, " ")
      .trim();

    const epMatch = s.match(/^(.*?)\b(?:S(\d{1,2})E(\d{1,2})|(\d{1,2})x(\d{1,2}))\b/i);
    if (epMatch) {
      let show = epMatch[1].replace(/\b(19|20)\d{2}\s*$/, "").trim();
      const sn = Number(epMatch[2] ?? epMatch[4]);
      const en = Number(epMatch[3] ?? epMatch[5]);
      const code = `S${String(sn).padStart(2, "0")}E${String(en).padStart(2, "0")}`;
      if (show) return { query: `${show} ${code}`.slice(0, 100) };
    }

    s = s.replace(/\b(2160p|1440p|1080p|720p|480p|4k|uhd|web-?dl|webrip|bluray|bdrip|brrip|hdrip|dvdrip|hdtv|x264|x265|h264|h265|hevc|aac|ddp5|dd5|ac3|remux|proper|repack)\b.*$/i, "").trim();
    const y = s.match(/\b(19|20)\d{2}\b/);
    if (y?.index) {
      s = epFromMeta ? s.slice(0, y.index).trim() : s.slice(0, y.index + 4).trim();
    }
    const finalQuery = epFromMeta && !s.toUpperCase().includes(epFromMeta) ? `${s} ${epFromMeta}` : s;
    return { query: finalQuery.replace(/\s+/g, " ").trim().slice(0, 100), ...(imdb ? { imdb } : {}) };
  }

  async function find(lang: string) {
    fun.subs = { lang, busy: true, items: [], error: "" };
    auto = lang;
    try {
      const epItems = await subsSearchEpisode(file || null, title, lang);
      if (epItems.length > 0) {
        fun.subs = { lang, busy: false, items: epItems, error: "" };
        return;
      }
    } catch {
      // Yedek aramaya geç
    }

    if (!inRoom) {
      fun.subs = { lang, busy: false, items: [], error: "Bu bölüm/film için altyazı bulunamadı" };
      return;
    }

    auto = canControl ? lang : null;
    const hash = await subsHash().catch(() => null);
    const { query, imdb } = parseQuery(title, file);
    syncFun({
      t: "subs_search",
      lang,
      query,
      ...(imdb ? { imdb } : {}),
      ...(hash ? { hash } : {}),
    }).catch((e) => {
      fun.subs.busy = false;
      onError(tx(String(e)));
    });
  }

  async function pick(s: SubResult) {
    if (s.url) {
      try {
        const added = await subsApplyUrl(s.url, s.name);
        onAdded?.();
        onLog?.(t("Altyazı yüklendi: {name}", { name: added }));
      } catch (e) {
        onError(tx(String(e)));
      }
      return;
    }
    if (s.fileId && inRoom && canControl) {
      syncFun({ t: "subs_pick", fileId: s.fileId, lang: s.lang }).catch((e) => onError(tx(String(e))));
    }
  }

  $effect(() => {
    const s = fun.subs;
    if (auto && !s.busy && s.lang === auto && s.items.length) {
      auto = null;
      pick(s.items[0]);
    }
  });
</script>

<div class="subs">
  <span class="label">{t("İnternetten bul")}</span>
  <button class="small" disabled={fun.subs.busy || !title} onclick={() => find("tr")}>TR</button>
  <button class="small" disabled={fun.subs.busy || !title} onclick={() => find("en")}>EN</button>
  {#if fun.subs.busy}<span class="muted">{t("aranıyor…")}</span>{/if}
</div>
{#if fun.subs.error}
  <div class="muted err">{tx(fun.subs.error)}</div>
{:else if fun.subs.items.length}
  <ul class="list">
    {#each fun.subs.items.slice(0, 10) as s, i (s.url ?? s.fileId ?? i)}
      {@const selectable = !!s.url || (inRoom && canControl)}
      <li>
        <button class="item" disabled={!selectable} title={selectable ? t("Altyazıyı yükle ve seç") : t("Odaya altyazıyı host ekler")} onclick={() => pick(s)}>
          <span class="name">{s.name}</span>
          <small>{s.lang.toUpperCase()}{#if s.downloads > 0} · {t("{n} indirme", { n: s.downloads })}{/if}{#if s.hashMatch} · <b class="ok">{t("sürüme uygun")}</b>{/if}{#if s.machine} · {t("makine çevirisi")}{/if}{#if s.hi} · {t("işitme engelli")}{/if}</small>
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .subs { display: flex; align-items: center; gap: 6px; margin-top: 8px; }
  .label { font-size: 12px; color: var(--text-2); margin-right: auto; }
  .muted { color: var(--text-2); font-size: 12px; }
  .err { color: #ff6961; margin-top: 6px; }
  .list { list-style: none; padding: 0; margin: 8px 0 0; display: flex; flex-direction: column; gap: 4px; max-height: 240px; overflow-y: auto; }
  .item { width: 100%; text-align: left; display: flex; flex-direction: column; gap: 2px; padding: 7px 9px; border-radius: 10px; background: var(--glass); border: 1px solid var(--line); }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
  .item small { color: var(--text-3); font-size: 11px; }
  .ok { color: var(--green); font-weight: 600; }
</style>
