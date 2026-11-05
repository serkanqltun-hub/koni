<script lang="ts">
  // Ne izlesek / Ne izlesem ortak kartı: fragman (sessiz başlar) ya da afiş, sağa/sola
  // sürükleyerek oy. Oyu üst bileşen işler (`onCast`); Boşluk/M tuşları `toggle`/`mute`.
  import { fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import type { Movie } from "../player";
  import { t, i18n } from "../i18n.svelte";

  let { card, under = 0, grad, onCast, onExpand }: {
    card: Movie;
    /** Arkada görünen kart sayısı (en fazla 2). */
    under?: number;
    /** Afiş yoksa arka plan. */
    grad: (title: string) => string;
    onCast: (like: boolean) => void;
    /** Fragmanı büyüt. */
    onExpand: (trailer: string) => void;
  } = $props();

  let dx = $state(0);
  let dragFrom = $state<number | null>(null);
  function down(e: PointerEvent) { dragFrom = e.clientX; (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); }
  function move(e: PointerEvent) { if (dragFrom !== null) dx = e.clientX - dragFrom; }
  function up() {
    if (dragFrom === null) return;
    const d = dx;
    dragFrom = null; dx = 0;
    if (Math.abs(d) > 90) onCast(d > 0);
  }

  // Oylama kartındaki fragman: YouTube gömmesini postMessage ile yönetiriz (ayrı betik yüklemeden).
  let tvFrame = $state<HTMLIFrameElement | null>(null);
  let tvMuted = $state(true);
  let tvPaused = $state(false);
  let tvAt = $state(0);
  let tvLen = $state(0);
  $effect(() => {
    void card.id;
    tvMuted = true; tvPaused = false; tvAt = 0; tvLen = 0;
  });
  function yt(func: string) {
    tvFrame?.contentWindow?.postMessage(JSON.stringify({ event: "command", func, args: [] }), "*");
  }
  function ytListen() {
    tvFrame?.contentWindow?.postMessage(JSON.stringify({ event: "listening", id: 1, channel: "widget" }), "*");
  }
  /** Klavye (M / Boşluk) üst bileşenden gelir. */
  export function mute() { ytMute(); }
  export function toggle() { ytToggle(); }
  function ytMute() { tvMuted = !tvMuted; yt(tvMuted ? "mute" : "unMute"); }
  function ytToggle() { tvPaused = !tvPaused; yt(tvPaused ? "pauseVideo" : "playVideo"); }
  function ytInfo(e: MessageEvent) {
    if (!tvFrame || e.source !== tvFrame.contentWindow || typeof e.data !== "string") return;
    try {
      const info = JSON.parse(e.data)?.info;
      if (!info) return;
      if (typeof info.currentTime === "number") tvAt = info.currentTime;
      if (typeof info.duration === "number" && info.duration > 0) tvLen = info.duration;
      if (info.playerState === 1) tvPaused = false;
      else if (info.playerState === 2 || info.playerState === 0) tvPaused = true;
    } catch { /* YouTube dışı ileti */ }
  }
  const clock = (sec: number) => `${Math.floor(sec / 60)}:${String(Math.floor(sec % 60)).padStart(2, "0")}`;
</script>

<svelte:window onmessage={ytInfo} />

<div class="tcard">
  {#each Array(Math.min(under, 2)) as _, i}<div class="under u{Math.min(under, 2) - i}"></div>{/each}
  {#key card.id}
    <div class="tv" role="group" aria-label={t("Kart")} in:fly={{ y: 30, duration: 380, easing: cubicOut }}
      style="transform: translateX({dx}px) rotate({-1.2 + dx / 40}deg); transition: {dragFrom === null ? 'transform .25s' : 'none'}">
      {#if card.trailer}
        <iframe bind:this={tvFrame} title={t("Fragman")} referrerpolicy="strict-origin-when-cross-origin" allow="autoplay; encrypted-media" onload={ytListen}
          src="https://www.youtube-nocookie.com/embed/{card.trailer}?autoplay=1&mute=1&controls=0&cc_load_policy=1&cc_lang_pref={i18n.lang}&hl={i18n.lang}&modestbranding=1&playsinline=1&rel=0&iv_load_policy=3&enablejsapi=1"></iframe>
      {:else}
        <div class="nf-bg">{#if card.poster}<img src={card.poster} alt="" />{/if}</div>
        <div class="nf-poster"><div class="poster" style="--p:{grad(card.title)}">{#if card.poster}<img src={card.poster} alt="" draggable="false" onerror={(e) => { (e.currentTarget as HTMLElement).style.display = "none"; }} />{:else}<b>{card.title}</b>{/if}</div></div>
        <div class="nf"><b>{t("Fragman bulunamadı")}</b>{t("Afişe ve künyeye bakarak karar ver")}</div>
      {/if}
      <!-- Sürükleme katmanı: iframe imleci yutmasın, kart sağa/sola çekilebilsin -->
      <div class="grab" role="presentation" onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up}
        onclick={() => { if (card.trailer && Math.abs(dx) < 4) ytToggle(); }}></div>
      {#if card.trailer}
        <div class="tl"><span>▶ {t("Fragman")}</span></div>
        <div class="tr">
          <button class="snd" class:on={!tvMuted} onclick={ytMute}>{tvMuted ? t("🔇 Sesi aç") : t("🔊 Sesli")} <kbd>M</kbd></button>
          <button onclick={() => onExpand(card.trailer!)} title={t("Büyüt")}>⤢</button>
        </div>
        <div class="ctrl">
          <button class="pp" onclick={ytToggle} title={t("Durdur / oynat (Boşluk)")}>{tvPaused ? "▶" : "❚❚"}</button>
          <span class="t">{clock(tvAt)}</span>
          <div class="bar"><i style="width:{tvLen ? (tvAt / tvLen) * 100 : 0}%"></i></div>
          <span class="t">{tvLen ? clock(tvLen) : "–:––"}</span>
        </div>
      {/if}
      {#if dx > 50}<span class="stamp like">{t("BEĞEN")}</span>{:else if dx < -50}<span class="stamp pass">{t("GEÇ")}</span>{/if}
    </div>
  {/key}
</div>

<style>
  button { font: inherit; color: inherit; cursor: pointer; }
  kbd { font: 500 10.5px var(--font-mono); padding: 1px 5px; border-radius: 5px; border: 1px solid var(--line-2); color: var(--text-2); }
  .tcard { position: relative; }
  .tcard .under { position: absolute; inset: 0; border-radius: 18px; background: #1a1d22; }
  .tcard .under.u2 { transform: translate(18px, 16px) rotate(1.2deg) scale(0.96); filter: brightness(0.55); }
  .tcard .under.u1 { transform: translate(9px, 8px) scale(0.98); filter: brightness(0.75); }
  .tv { position: relative; z-index: 2; width: 100%; max-height: calc(100vh - 330px); aspect-ratio: 16 / 9; border-radius: 18px; overflow: hidden; background: #050607;
    box-shadow: 0 30px 70px rgba(0, 0, 0, 0.65), 0 0 0 1px rgba(255, 255, 255, 0.1); }
  .tv iframe { position: absolute; inset: 0; width: 100%; height: 100%; border: 0; pointer-events: none; }
  .tv .grab { position: absolute; inset: 0; z-index: 1; touch-action: none; cursor: grab; }
  .tv .grab:active { cursor: grabbing; }
  .tv .tl, .tv .tr { position: absolute; top: 14px; z-index: 2; display: flex; gap: 6px; }
  .tv .tl { left: 14px; pointer-events: none; }
  .tv .tr { right: 14px; }
  .tv .tl span, .tv .tr button { padding: 5px 10px; border-radius: 99px; background: rgba(0, 0, 0, 0.55); backdrop-filter: blur(10px); border: 1px solid rgba(255, 255, 255, 0.14); font-size: 11.5px; font-weight: 650; color: #fff; display: flex; gap: 6px; align-items: center; }
  .tv .tr button.snd { background: #fff; color: #000; border-color: #fff; }
  .tv .tr button.snd.on { background: rgba(0, 0, 0, 0.55); color: #fff; border-color: rgba(255, 255, 255, 0.14); }
  .tv .tr button.snd kbd { color: inherit; opacity: 0.6; border-color: currentColor; }
  .tv .ctrl { position: absolute; z-index: 2; left: 0; right: 0; bottom: 0; height: 52px; padding: 0 16px; display: flex; align-items: center; gap: 12px; background: linear-gradient(0deg, rgba(0, 0, 0, 0.75), transparent); }
  .tv .ctrl .pp { width: 30px; height: 30px; border-radius: 50%; border: 0; padding: 0; background: rgba(255, 255, 255, 0.16); color: #fff; display: grid; place-items: center; font-size: 11px; }
  .tv .ctrl .t { font-family: var(--mono, monospace); font-size: 11px; color: rgba(255, 255, 255, 0.85); white-space: nowrap; }
  .tv .bar { position: relative; flex: 1; height: 4px; border-radius: 4px; background: rgba(255, 255, 255, 0.2); overflow: hidden; }
  .tv .bar i { position: absolute; left: 0; top: 0; bottom: 0; border-radius: 4px; background: #fff; transition: width 0.25s linear; }
  .tv .nf-bg { position: absolute; inset: 0; }
  .tv .nf-bg img { position: absolute; inset: -10%; width: 120%; height: 120%; object-fit: cover; filter: blur(40px) saturate(1.3) brightness(0.6); }
  .tv .nf-poster { position: absolute; left: 6%; top: 10%; bottom: 10%; aspect-ratio: 2 / 3; }
  .tv .nf-poster .poster { height: 100%; }
  .tv .nf { position: absolute; right: 8%; top: 50%; transform: translateY(-50%); max-width: 44%; padding: 12px 16px; border-radius: 14px; background: rgba(0, 0, 0, 0.55); backdrop-filter: blur(10px); border: 1px solid rgba(255, 255, 255, 0.14); font-size: 12.5px; color: rgba(255, 255, 255, 0.85); }
  .tv .nf b { display: block; font-size: 14px; color: #fff; margin-bottom: 2px; }
  .tv .stamp { z-index: 3; }
  .stamp { position: absolute; z-index: 5; left: 26px; top: 40px; transform: rotate(-16deg); border: 3px solid; font-weight: 900; font-size: 26px; padding: 2px 12px; border-radius: 8px; letter-spacing: 0.06em; background: rgba(0, 0, 0, 0.35); }
  .stamp.like { border-color: var(--green); color: var(--green); }
  .stamp.pass { border-color: #ff6961; color: #ff6961; left: auto; right: 26px; transform: rotate(16deg); }
  .poster { position: relative; aspect-ratio: 2 / 3; border-radius: 12px; overflow: hidden; background: var(--p, #1a1b1f); box-shadow: 0 18px 40px rgba(0, 0, 0, 0.55), 0 0 0 1px rgba(255, 255, 255, 0.07); display: flex; flex-direction: column; justify-content: flex-end; padding: 10% 9%; }
  .poster img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .poster b { position: relative; z-index: 1; font-weight: 900; letter-spacing: -0.02em; line-height: 1; font-size: 15px; }
</style>
