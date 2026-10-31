<script lang="ts">
  // "Kaldığın yerden devam": oynarken konumu ~5 sn'de bir kaydeder; kayıt ilerideyse tek tıkla atlatır.
  import { saveResume, loadResume, type ResumeMedia } from "./resume";
  import { t } from "./i18n.svelte";

  let {
    media,
    pos,
    duration,
    roomPos,
    room,
    mates = [],
    canSeek,
    onSeek,
  }: {
    media: ResumeMedia | null;
    pos: number | null;
    duration: number | null;
    /** Odanın konumu (tek başınaysa null). */
    roomPos: number | null;
    room?: string;
    /** Odadaki diğer üyeler: yarım kalanlarda "aynı ekiple devam" için. */
    mates?: string[];
    canSeek: boolean;
    onSeek: (s: number) => void;
  } = $props();

  let saved = $state<number | null>(null);
  let closedFor = $state("");

  function fmt(s: number): string {
    const t = Math.floor(s), h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), sec = t % 60;
    const p = (n: number) => String(n).padStart(2, "0");
    return h > 0 ? `${h}:${p(m)}:${p(sec)}` : `${p(m)}:${p(sec)}`;
  }

  // Medya açıldığında kayıtlı konumu bir kez oku.
  $effect(() => {
    const src = media?.src;
    saved = src ? (loadResume().find((r) => r.src === src)?.pos ?? null) : null;
  });

  const show = $derived(
    canSeek && media != null && saved != null && closedFor !== media.src && duration != null &&
      (roomPos ?? pos ?? 0) <= saved - 60,
  );

  // ~5 sn'de bir kaydet. Düğme görünürken kaydı ezme (başlangıçta konum <30 sn kaydı silerdi).
  let latest: { media: typeof media; pos: number | null; duration: number | null; room?: string; mates: string[]; show: boolean } | null = null;
  $effect(() => { latest = { media, pos, duration, room, mates, show }; });
  $effect(() => {
    const id = setInterval(() => {
      if (!latest) return;
      const { media: m, pos: p, duration: d, room: r, mates: w, show: s } = latest;
      if (!m || p == null || !d || s) return;
      saveResume({ src: m.src, kind: m.kind, title: m.title || m.src, file: m.file, idx: m.idx, room: r, with: w.length ? w : undefined, pos: p, duration: d });
    }, 5000);
    return () => clearInterval(id);
  });
</script>

{#if show && saved != null && media}
  <span class="capsule warn">
    <button class="white small" onclick={() => { onSeek(saved!); closedFor = media!.src; }}>{t("Kaldığın yerden devam")} · {fmt(saved)}</button>
    <button class="small" title={t("Kapat")} aria-label={t("Kapat")} onclick={() => (closedFor = media!.src)}>✕</button>
  </span>
{/if}

<style>
  .capsule {
    display: inline-flex; align-items: center; gap: 8px; height: 34px; padding: 0 6px 0 12px; border-radius: 10px; font-size: 12px; white-space: nowrap; flex: none;
    background: rgba(255, 214, 10, 0.1); border: 1px solid rgba(255, 214, 10, 0.25); color: var(--yellow);
  }
  .capsule button { margin: 0; }
</style>
