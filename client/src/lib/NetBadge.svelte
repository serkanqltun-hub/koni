<script lang="ts">
  import { t } from "./i18n.svelte";
  // Odadakiler çekmecesinde kişi başına bağlantı göstergesi: ping ve tampon.
  // `state`: üyenin `member_status.state`'i (playing, stalled, refilling…).
  let { rttMs, bufferPct, state }: { rttMs?: number | null; bufferPct?: number; state?: string } = $props();

  const ping = $derived(rttMs == null ? null : Math.round(rttMs));
  const tone = $derived(ping == null ? "" : ping < 100 ? "good" : ping < 250 ? "mid" : "bad");
  const buf = $derived.by(() => {
    if (state === "stalled") return { tone: "bad", text: t("takılıyor") };
    if (state === "refilling") return { tone: "mid", text: t("dolduruyor") };
    if (bufferPct != null && bufferPct < 100) return { tone: "mid", text: t("tampon %{n}", { n: bufferPct }) };
    return null;
  });
</script>

<span class="net" title={t("Ping · tampon")}>
  {#if buf}<span class="buf {buf.tone}">{buf.text}</span>{/if}
  <span class="ping {tone}"><i></i>{ping == null ? "–" : `${ping} ms`}</span>
</span>

<style>
  .net { display: inline-flex; align-items: center; gap: 6px; font-size: 11px; white-space: nowrap; font-variant-numeric: tabular-nums; flex: none; }
  .ping { display: inline-flex; align-items: center; gap: 4px; color: var(--text-3); }
  .ping i { width: 6px; height: 6px; border-radius: 50%; background: var(--text-3); }
  .ping.good i { background: var(--green); }
  .ping.mid i { background: var(--yellow); }
  .ping.bad i { background: var(--red); }
  .ping.bad { color: var(--red); }
  .buf { padding: 0 6px; border-radius: 5px; font-weight: 600; }
  .buf.mid { color: var(--yellow); background: rgba(255, 214, 10, 0.1); }
  .buf.bad { color: var(--red); background: rgba(255, 69, 58, 0.12); }
</style>
