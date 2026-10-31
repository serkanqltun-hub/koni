<script lang="ts">
  import { t } from "./i18n.svelte";
  // Müzik sahnesi "Şimdi çalıyor" (Spotify tarzı): büyük kapak, şarkı/sanatçı ve
  // kapağın renginden arka plan. mpv penceresi bu sahnede gizlidir.
  import { fade, fly, scale } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";
  import Logo from "./Logo.svelte";
  import { splitTitle, hueOf, dominant } from "./music";

  let { title, src, cover, playing, source, upNext, empty, controls }: {
    /** Çalan parçanın başlığı (yoksa boş). */
    title: string;
    src: string;
    cover: string | null;
    playing: boolean;
    /** "Lounge" ya da çalan listenin adı. */
    source: string;
    upNext: string | null;
    /** Boşken gösterilecek açıklama. */
    empty: string;
    /** Oynatıcı (App'ten: oynat, atla, dalga formlu süre çubuğu, ses). */
    controls?: Snippet;
  } = $props();

  const meta = $derived(splitTitle(title));

  // Kapaktan baskın renk; okunamazsa başlıktan sabit ton.
  let rgb = $state<string | null>(null);
  $effect(() => {
    const url = cover;
    rgb = null;
    if (!url) return;
    let live = true;
    dominant(url).then((c) => { if (live) rgb = c; });
    return () => { live = false; };
  });
  const tint = $derived(rgb ?? (title ? `hsl(${hueOf(title)} 55% 42%)` : "hsl(268 45% 40%)"));
  const tintCss = $derived(rgb ? `rgb(${rgb})` : tint);

</script>

<div class="music" style="--tint:{tintCss}">
  {#key cover ?? title}
    <div class="bg" in:fade={{ duration: 900 }} out:fade={{ duration: 900 }}>
      {#if cover}<img src={cover} alt="" />{/if}
    </div>
  {/key}
  <div class="veil"></div>

  {#if title}
    <div class="stage">
      {#key title}
        <div class="hero" in:fly={{ y: 24, duration: 600, easing: cubicOut }}>
          <div class="art" class:playing in:scale={{ start: 0.85, duration: 650, easing: cubicOut }}>
            {#if cover}
              <img src={cover} alt="" />
            {:else}
              <div class="gen"><Icon name="music" size={64} /></div>
            {/if}
          </div>
          <div class="meta">
            <div class="kicker">
              {#if playing}<span class="eq" aria-hidden="true"><i></i><i></i><i></i><i></i></span>{/if}
              {t("Şimdi çalıyor")} · {t(source)}
            </div>
            <h1 title={title} class:long={meta.song.length > 18} class:xlong={meta.song.length > 34}>{meta.song}</h1>
            {#if meta.artist}<div class="artist">{meta.artist}</div>{/if}
            {@render controls?.()}
            {#if upNext}<div class="actions"><span class="next"><Icon name="next" size={12} /> {t("Sıradaki:")} <b>{upNext}</b></span></div>{/if}
          </div>
        </div>
      {/key}

    </div>
  {:else}
    <div class="idle" in:fade={{ duration: 400 }}>
      <div class="art big"><div class="gen"><Logo size={56} /></div></div>
      <h1>{t("Sessizlik")}</h1>
      <p>{t(empty)}</p>
    </div>
  {/if}
</div>

<style>
  /* Zemin çekmecenin ve alt şeridin altına da uzanır: Lounge aynı sahnenin parçası gibi. */
  .music { grid-row: 2 / 4; grid-column: 1 / 3; position: relative; overflow: hidden; min-width: 0; min-height: 0; background: #070708; isolation: isolate; }
  .bg, .veil { position: absolute; inset: 0; }
  .bg { background: radial-gradient(150% 110% at 35% 0%, var(--tint), transparent 75%); transition: background 1s; }
  .bg img { position: absolute; inset: -10%; width: 120%; height: 120%; object-fit: cover; filter: blur(70px) saturate(1.6); opacity: 0.55; }
  .veil { background: linear-gradient(180deg, rgba(0, 0, 0, 0.1) 0%, rgba(7, 7, 8, 0.65) 55%, #070708 100%); }

  .stage {
    position: relative; height: 100%; display: flex; align-items: center;
    padding: 40px calc(var(--drawer, 0px) + 56px) calc(var(--bottom-bar, 0px) + 32px) 56px;
    transition: padding var(--drawer-ms, 0.3s) cubic-bezier(0.32, 0.72, 0, 1);
  }
  .hero { display: flex; align-items: center; gap: 40px; min-width: 0; width: 100%; }
  .art {
    flex: none; width: clamp(180px, 42vh, 340px); aspect-ratio: 1; border-radius: 16px; overflow: hidden; position: relative;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6), 0 0 0 1px rgba(255, 255, 255, 0.08);
    transition: transform 0.6s cubic-bezier(0.34, 1.3, 0.5, 1), box-shadow 0.6s;
    transform: scale(0.94);
  }
  .art.playing { transform: none; box-shadow: 0 30px 80px rgba(0, 0, 0, 0.65), 0 0 60px -10px var(--tint); }
  .art img { width: 100%; height: 100%; object-fit: cover; display: block; transform: scale(1.34); } /* hqdefault'un siyah şeritleri */
  .gen {
    width: 100%; height: 100%; display: grid; place-items: center; color: rgba(255, 255, 255, 0.85);
    background: linear-gradient(135deg, var(--tint), #1a1a1f);
  }
  .meta { flex: 1; min-width: 0; max-width: 620px; display: flex; flex-direction: column; gap: 2px; }
  /* Kapağın üst kenarında ince cam parlaması. */
  .art::after { content: ""; position: absolute; inset: 0; border-radius: inherit; pointer-events: none; background: linear-gradient(160deg, rgba(255, 255, 255, 0.14), transparent 40%); }
  .kicker { display: flex; align-items: center; gap: 8px; font-size: 12px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.08em; color: rgba(255, 255, 255, 0.75); }
  /* İki satıra sığar; alt boşluk g/y/ş kuyruklarının kesilmesini önler. */
  h1 {
    margin: 2px 0 0; padding-bottom: 0.14em; font-size: clamp(32px, 4.6vw, 60px); line-height: 1.08; font-weight: 800;
    letter-spacing: -0.03em; text-wrap: balance; text-shadow: 0 2px 24px rgba(0, 0, 0, 0.25);
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }
  h1.long { font-size: clamp(28px, 3.6vw, 46px); }
  h1.xlong { font-size: clamp(24px, 2.8vw, 36px); }
  .kicker { margin-bottom: 6px; }
  .artist { font-size: 18px; font-weight: 600; color: rgba(255, 255, 255, 0.86); letter-spacing: -0.01em; }
  .actions { display: flex; align-items: center; gap: 16px; margin-top: 10px; min-width: 0; }
  .next { display: inline-flex; align-items: center; gap: 6px; font-size: 12.5px; color: rgba(255, 255, 255, 0.6); min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .next b { color: rgba(255, 255, 255, 0.9); font-weight: 600; overflow: hidden; text-overflow: ellipsis; }

  .eq { display: inline-flex; align-items: flex-end; gap: 2px; height: 12px; }
  .eq i { width: 3px; background: #1ed760; border-radius: 1px; animation: eq 0.9s ease-in-out infinite; }
  .eq i:nth-child(2) { animation-delay: -0.3s; }
  .eq i:nth-child(3) { animation-delay: -0.6s; }
  .eq i:nth-child(4) { animation-delay: -0.15s; }
  @keyframes eq { 0%, 100% { height: 3px; } 50% { height: 12px; } }

  .idle { position: relative; height: 100%; padding-right: var(--drawer, 0px) !important; padding-bottom: var(--bottom-bar, 0px) !important; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; text-align: center; padding: 24px; }
  .idle h1 { font-size: 40px; }
  .idle p { margin: 0; max-width: 420px; color: rgba(255, 255, 255, 0.65); font-size: 14px; }
  .art.big { width: 180px; transform: none; margin-bottom: 14px; animation: float 5s ease-in-out infinite; }
  @keyframes float { 50% { transform: translateY(-8px); } }
</style>
