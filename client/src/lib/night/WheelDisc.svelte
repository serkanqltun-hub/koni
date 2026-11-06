<script lang="ts">
  // Çark çizimi (odadaki ortak çark ve Ne izlesem). Dilim i, tepedeki işaretten saat
  // yönünde [i/n, (i+1)/n) turu kaplar; `rot` derece, `dur` ms dönüş süresi.
  // Afişi olan dilim afişle dolar; göbek `onSpin` verilirse düğmedir.
  import { t } from "../i18n.svelte";

  let { items, rot, dur, size = 260, onSpin, spinning = false, hue }: {
    items: { id: string | number; title: string; poster?: string }[];
    rot: number; dur: number; size?: number;
    /** Göbeğe basınca çevir (yoksa göbek yalnız süs). */
    onSpin?: () => void;
    spinning?: boolean;
    /** Afişsiz dilimin rengi için başlık tonu. */
    hue?: (name: string) => number;
  } = $props();

  const C = $derived(size / 2);
  const R = $derived(C - 14);
  const big = $derived(size >= 340);
  function pt(deg: number, rad = R) {
    const a = (deg * Math.PI) / 180;
    return `${(C + rad * Math.sin(a)).toFixed(2)} ${(C - rad * Math.cos(a)).toFixed(2)}`;
  }
  const slices = $derived.by(() => {
    const n = items.length;
    const a = 360 / Math.max(n, 1);
    return items.map((p, i) => {
      const from = i * a, to = (i + 1) * a, mid = from + a / 2;
      const title = p.title.length > 16 ? p.title.slice(0, 15) + "…" : p.title;
      const h = hue ? hue(p.title) : (i * 360) / n + 210;
      // Yazı dilim boyunca (merkezden dışa); alt yarıda ters dönmesin diye çevrilir.
      const rad = (mid * Math.PI) / 180, tr = R * 0.6;
      const flip = mid > 90 && mid < 270;
      return {
        id: p.id, title, poster: p.poster, mid,
        tx: +(C + tr * Math.sin(rad)).toFixed(2), ty: +(C - tr * Math.cos(rad)).toFixed(2), ta: flip ? mid + 90 : mid - 90,
        d: n === 1 ? `M${C} ${C} m-${R} 0 a${R} ${R} 0 1 0 ${R * 2} 0 a${R} ${R} 0 1 0 -${R * 2} 0` : `M${C} ${C} L${pt(from)} A${R} ${R} 0 ${a > 180 ? 1 : 0} 1 ${pt(to)} Z`,
        fill: `hsl(${h} 45% ${i % 2 ? 30 : 38}%)`,
        // Afiş: dilim ortası boyunca, dışa bakan dikdörtgen (dilime kırpılır).
        img: { w: Math.max(R * 1.1, (2 * Math.PI * R * a) / 360 + 40), h: R + 30 },
      };
    });
  });
  const bulbs = $derived(big ? Array.from({ length: 36 }, (_, i) => pt(i * 10, R + 8).split(" ")) : []);
  const uid = Math.random().toString(36).slice(2, 8);
</script>

<div class="disc" style="width:{size}px;height:{size}px" class:big>
  <svg width={size} height={size} viewBox="0 0 {size} {size}" aria-label={t("Çark")}>
    <defs>
      {#each slices as s (s.id)}<clipPath id="c{uid}{s.id}"><path d={s.d} /></clipPath>{/each}
      <linearGradient id="sh{uid}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="rgba(0,0,0,.55)" /><stop offset="0.45" stop-color="rgba(0,0,0,0)" /></linearGradient>
    </defs>
    <circle cx={C} cy={C} r={R + 13} fill="#121317" stroke="rgba(255,255,255,.12)" stroke-width="2" />
    <g style="transform: rotate({rot}deg); transform-origin: {C}px {C}px; transition: transform {dur}ms cubic-bezier(0.12, 0.6, 0.08, 1)">
      {#each slices as s (s.id)}
        <path d={s.d} fill={s.fill} />
        {#if s.poster}
          <g clip-path="url(#c{uid}{s.id})">
            <g transform="rotate({s.mid} {C} {C})">
              <image href={s.poster} x={C - s.img.w / 2} y={C - R - 14} width={s.img.w} height={s.img.h} preserveAspectRatio="xMidYMid slice" />
              <rect x={C - s.img.w / 2} y={C - R - 14} width={s.img.w} height={s.img.h} fill="url(#sh{uid})" />
            </g>
          </g>
        {/if}
        <path d={s.d} fill="none" stroke="rgba(10,11,14,.9)" stroke-width="3" />
        <text x={s.tx} y={s.ty} transform="rotate({s.ta} {s.tx} {s.ty})" text-anchor="middle" dominant-baseline="middle" fill="#fff" font-size={big ? 15 : 11} font-weight="800" style="paint-order: stroke; stroke: rgba(0,0,0,.7); stroke-width: 3px; letter-spacing: -.01em">{s.title}</text>
      {/each}
    </g>
    {#each bulbs as [x, y], i (i)}<circle cx={x} cy={y} r="3" fill={i % 2 ? "#ffd60a" : "#fff3b0"} opacity="0.9" />{/each}
    <circle cx={C} cy={C} r={R} fill="none" stroke="rgba(255,255,255,.18)" stroke-width="2" />
  </svg>
  <svg class="ptr" viewBox="0 0 34 44" style="width:{big ? 34 : 22}px"><path d="M17 44 L2 10 Q17 -6 32 10 Z" fill="#ffd60a" stroke="#7a5a00" stroke-width="1.5" /></svg>
  {#if onSpin}
    <button class="hub" onclick={onSpin} disabled={spinning} style="width:{big ? 110 : 56}px;height:{big ? 110 : 56}px">
      {spinning ? t("Dönüyor…") : t("ÇEVİR")}{#if big && !spinning}<small>{t("ya da afişe dokun")}</small>{/if}
    </button>
  {:else}
    <span class="hub dot" style="width:{big ? 44 : 28}px;height:{big ? 44 : 28}px"></span>
  {/if}
</div>

<style>
  .disc { position: relative; display: grid; place-items: center; }
  .disc svg:first-child { filter: drop-shadow(0 20px 40px rgba(0, 0, 0, 0.6)); }
  .ptr { position: absolute; top: -2px; left: 50%; transform: translateX(-50%); filter: drop-shadow(0 6px 10px rgba(0, 0, 0, 0.7)); }
  .hub { position: absolute; border-radius: 50%; border: 0; padding: 0; background: #fff; color: #000; display: grid; place-items: center; align-content: center; font: inherit; font-weight: 900; font-size: 14px; letter-spacing: 0.04em; cursor: pointer; box-shadow: 0 0 0 6px #121317, 0 0 0 7px rgba(255, 255, 255, 0.18), 0 14px 40px rgba(0, 0, 0, 0.7); transition: transform 0.15s; }
  .hub:hover:not(:disabled) { transform: scale(1.04); }
  .hub:disabled { cursor: default; opacity: 1; color: #777; font-size: 12px; }
  .hub small { font-weight: 600; font-size: 9.5px; color: #666; letter-spacing: 0.01em; }
  .hub.dot { background: #1c1c1e; box-shadow: 0 0 0 2px rgba(255, 255, 255, 0.25); }
</style>
