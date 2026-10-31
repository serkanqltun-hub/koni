<script lang="ts">
  import { t } from "./i18n.svelte";
  // İki tutamaklı aralık seçici (Ne izlesek? özel filtre).
  let { min, max, step = 1, lo = $bindable(), hi = $bindable(), disabled = false }: {
    min: number; max: number; step?: number; lo: number; hi: number; disabled?: boolean;
  } = $props();
  const pct = (v: number) => ((v - min) / (max - min)) * 100;
</script>

<div class="dr" class:disabled style="--a:{pct(lo)}%; --b:{pct(hi)}%">
  <div class="track"></div>
  <div class="fill"></div>
  <input type="range" {min} {max} {step} {disabled} value={lo} aria-label={t("En az")}
    oninput={(e) => (lo = Math.min(Number(e.currentTarget.value), hi))} />
  <input type="range" {min} {max} {step} {disabled} value={hi} aria-label={t("En çok")}
    oninput={(e) => (hi = Math.max(Number(e.currentTarget.value), lo))} />
</div>

<style>
  .dr { position: relative; height: 26px; margin-top: 4px; }
  .dr.disabled { opacity: 0.5; }
  .track, .fill { position: absolute; top: 11px; height: 4px; border-radius: 4px; }
  .track { left: 0; right: 0; background: rgba(255, 255, 255, 0.14); }
  .fill { left: var(--a); right: calc(100% - var(--b)); background: #fff; }
  input { position: absolute; inset: 0; width: 100%; margin: 0; background: none; pointer-events: none; -webkit-appearance: none; appearance: none; height: 26px; }
  input::-webkit-slider-runnable-track { background: none; height: 26px; }
  input::-webkit-slider-thumb {
    -webkit-appearance: none; pointer-events: auto; width: 18px; height: 18px; margin-top: 4px; border-radius: 50%;
    background: #fff; box-shadow: 0 2px 8px rgba(0, 0, 0, 0.5); cursor: grab; border: 0;
  }
  input::-moz-range-thumb { pointer-events: auto; width: 18px; height: 18px; border-radius: 50%; background: #fff; border: 0; cursor: grab; }
  input::-moz-range-track { background: none; }
</style>
