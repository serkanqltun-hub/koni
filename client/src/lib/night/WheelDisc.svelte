<script lang="ts">
  // Çark çizimi (odadaki ortak çark ve Ne izlesem). Dilim i, tepedeki işaretten saat
  // yönünde [i/n, (i+1)/n) turu kaplar; `rot` derece, `dur` ms dönüş süresi.
  import { t } from "../i18n.svelte";

  let { items, rot, dur, size = 260 }: { items: { id: string | number; title: string }[]; rot: number; dur: number; size?: number } = $props();

  const C = $derived(size / 2);
  const R = $derived(C - 4);
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
      return {
        id: p.id, title,
        d: n === 1 ? "" : `M${C} ${C} L${pt(from)} A${R} ${R} 0 ${a > 180 ? 1 : 0} 1 ${pt(to)} Z`,
        fill: `hsl(${(i * 360) / n + 210} 55% ${i % 2 ? 38 : 46}%)`,
        x: +(C + R * 0.6 * Math.sin((mid * Math.PI) / 180)).toFixed(2),
        y: +(C - R * 0.6 * Math.cos((mid * Math.PI) / 180)).toFixed(2),
        rotate: mid - 90,
      };
    });
  });
</script>

<svg width={size} height={size} viewBox="0 0 {size} {size}" aria-label={t("Çark")}>
  <g style="transform: rotate({rot}deg); transform-origin: {C}px {C}px; transition: transform {dur}ms cubic-bezier(0.12, 0.6, 0.08, 1)">
    {#if slices.length === 1}
      <circle cx={C} cy={C} r={R} fill={slices[0].fill} />
    {/if}
    {#each slices as s (s.id)}
      {#if s.d}<path d={s.d} fill={s.fill} stroke="rgba(0,0,0,.35)" stroke-width="1" />{/if}
      <text x={s.x} y={s.y} transform="rotate({s.rotate} {s.x} {s.y})" text-anchor="middle" dominant-baseline="middle"
        fill="#fff" font-size="12" font-weight="600">{s.title}</text>
    {/each}
  </g>
  <circle cx={C} cy={C} r="16" fill="#1c1c1e" stroke="rgba(255,255,255,.25)" />
  <path d="M{C - 10} 0 L{C + 10} 0 L{C} 20 Z" fill="#fff" />
</svg>
