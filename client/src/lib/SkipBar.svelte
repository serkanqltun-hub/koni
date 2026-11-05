<script lang="ts">
  // İntro / özet atla (T28): kontrol şeridinde, videonun üstünde değil.
  // İşaretli dizide intro'ya girince "İntro'yu atla" 6 sn görünür; ayar açıksa kendiliğinden atlar.
  import type { EpRef } from "./nextep.svelte";
  import type { SkipMark } from "./player";
  import { autoSkip, clearMark, loadMarks, markFor, saveMark, spanAt } from "./skip";
  import { t } from "./i18n.svelte";

  let {
    url,
    ep,
    pos,
    canSeek,
    autoOk,
    onSeek,
    onNote,
  }: {
    url: string;
    /** Oynayan dizi bölümü; değilse null. */
    ep: EpRef | null;
    pos: number | null;
    /** Atlama düğmesi: solo'da hep, odada yetkisi olana. */
    canSeek: boolean;
    /** Otomatik atlama: solo'da ya da odada yalnızca host (herkes aynı anda göndermesin). */
    autoOk: boolean;
    onSeek: (s: number) => void;
    onNote: (key: string) => void;
  } = $props();

  // `ep` her karede yeni nesne gelebilir: etkiler yalnızca değer değişince çalışsın.
  const show = $derived(ep?.show ?? "");
  const epKey = $derived(ep?.key ?? "");
  let marks = $state<SkipMark[]>([]);
  $effect(() => {
    marks = [];
    if (!show) return;
    let alive = true;
    loadMarks(url, show).then((m) => { if (alive) marks = m; });
    return () => { alive = false; };
  });

  const mark = $derived(ep ? markFor(marks, ep.season) : null);
  const span = $derived(pos != null ? spanAt(mark, pos) : null);

  // Her bölümde her aralık bir kez: ya otomatik atla ya da düğmeyi 6 sn göster.
  let seen = "";
  let visible = $state("");
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const key = epKey && span ? `${epKey}|${span.kind}` : "";
    if (!key || key === seen || !span) return;
    seen = key;
    if (canSeek && autoOk && autoSkip()) {
      onSeek(span.to);
      onNote(span.kind === "intro" ? "İntro atlandı" : "Özet atlandı");
      return;
    }
    visible = key;
    clearTimeout(timer);
    timer = setTimeout(() => (visible = ""), 6000);
  });
  $effect(() => () => clearTimeout(timer));
  const showSkip = $derived(canSeek && !!ep && !!span && visible === `${epKey}|${span.kind}`);

  // İşaretleme: önce "başı", sonra "sonu". Kapsam: bu sezon ya da tüm sezonlar.
  let marking = $state(false);
  let start = $state<number | null>(null);
  let all = $state(false);
  let busy = $state(false);
  $effect(() => { void epKey; marking = false; start = null; });
  const season = $derived(all ? 0 : (ep?.season ?? 0));
  const own = $derived(marks.some((m) => m.season === season));

  async function save(p: Parameters<typeof saveMark>[3], done: string) {
    if (!ep || busy) return;
    busy = true;
    try {
      marks = await saveMark(url, ep.show, season, p);
      onNote(done);
      marking = false;
      start = null;
    } catch {
      onNote("İşaret kaydedilemedi");
    } finally {
      busy = false;
    }
  }
  function markEnd() {
    if (start == null || pos == null) return;
    if (pos <= start + 1) return onNote("İntro'nun sonu başından sonra olmalı");
    save({ introStart: start, introEnd: pos }, "İntro işaretlendi");
  }
  async function clear() {
    if (!ep || busy) return;
    busy = true;
    try {
      marks = await clearMark(url, ep.show, season);
      onNote("İşaret silindi");
      marking = false;
    } catch {
      onNote("İşaret silinemedi");
    } finally {
      busy = false;
    }
  }
</script>

{#if ep}
  {#if showSkip && span}
    <button class="white small skip" onclick={() => { onSeek(span.to); visible = ""; }}>
      {span.kind === "intro" ? t("İntro'yu atla") : t("Özeti atla")}
    </button>
  {/if}
  {#if marking}
    <span class="capsule">
      <select class="small" bind:value={all} title={t("İşaret kapsamı")}>
        <option value={false}>{t("Bu sezon")}</option>
        <option value={true}>{t("Tüm sezonlar")}</option>
      </select>
      {#if start == null}
        <button class="white small" disabled={pos == null} onclick={() => (start = pos)}>{t("Burası intro'nun başı")}</button>
        <button class="small" disabled={busy || pos == null || pos < 5} title={t("Bölümün başından buraya kadar özet")}
          onclick={() => save({ recapEnd: pos! }, "Özet işaretlendi")}>{t("Özet burada bitti")}</button>
        {#if own}<button class="small" disabled={busy} onclick={clear}>{t("Sil")}</button>{/if}
      {:else}
        <button class="white small" disabled={busy} onclick={markEnd}>{t("Burası intro'nun sonu")}</button>
      {/if}
      <button class="small" title={t("Kapat")} aria-label={t("Kapat")} onclick={() => { marking = false; start = null; }}>✕</button>
    </span>
  {:else}
    <button class="cb mk" class:on={!!mark} title={mark ? t("İntro işaretli · düzenle") : t("İntro'yu işaretle")} aria-label={t("İntro'yu işaretle")}
      onclick={() => (marking = true)}>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 4v16" /><path d="M5 5h11l-2 4 2 4H5" /></svg>
    </button>
  {/if}
{/if}

<style>
  .capsule {
    display: inline-flex; align-items: center; gap: 6px; height: 34px; padding: 0 6px; border-radius: 10px; font-size: 12px; white-space: nowrap; flex: none;
    background: rgba(255, 255, 255, 0.05); border: 1px solid rgba(255, 255, 255, 0.12);
  }
  .capsule button, .capsule select { margin: 0; }
  .skip { flex: none; white-space: nowrap; }
  .mk { width: 34px; height: 34px; padding: 0; border: 0; border-radius: 10px; background: none; display: grid; place-items: center; color: var(--text-2); flex: none; }
  .mk.on { color: var(--accent, #0a84ff); }
</style>
