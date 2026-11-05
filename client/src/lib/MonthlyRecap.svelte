<script lang="ts">
  // Aylık KONİ özeti (p0.28 `recap`): geçen ay / bu ay kartı, PNG kaydet, panoya paylaş.
  import Avatar from "./Avatar.svelte";
  import { recapGet, recapSavePng, type Recap } from "./player";
  import { social, splitHandle } from "./social.svelte";
  import { t, tx, locale } from "./i18n.svelte";

  const key = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
  const now = new Date();
  const thisMonth = key(now);
  const lastMonth = key(new Date(now.getFullYear(), now.getMonth() - 1, 1));
  // Ayın ilk haftasında geçen ayın özeti öne çıkar.
  let month = $state(now.getDate() <= 7 ? lastMonth : thisMonth);
  let rc = $state<Recap | null>(null);
  let note = $state("");

  $effect(() => {
    const m = month;
    if (!social.server) return;
    let live = true;
    recapGet(social.server, m).then((r) => { if (live) rc = r; }, () => { if (live) rc = null; });
    return () => { live = false; };
  });

  const monthName = (m: string) => {
    const [y, mo] = m.split("-").map(Number);
    return new Date(y, mo - 1, 1).toLocaleDateString(locale(), { month: "long", year: "numeric" });
  };
  function dur(sec: number): string {
    if (sec < 60) return t("0 dk");
    const h = Math.floor(sec / 3600), m = Math.round((sec % 3600) / 60);
    if (h === 0) return t("{m} dk", { m });
    return m && h < 10 ? t("{h} sa {m} dk", { h, m }) : t("{h} sa", { h });
  }
  const hh = (h: number) => `${String(h).padStart(2, "0")}:00`;
  const bare = (n: string) => splitHandle(n)[0];
  const empty = $derived(!rc || (rc.watchSec < 60 && !rc.movies && !rc.series));
  const top = $derived(Math.max(1, ...(rc?.hours ?? [0])));
  const stars = (s: number) => s.toFixed(1).replace(/\.0$/, "");

  function flash(s: string) {
    note = s;
    setTimeout(() => { if (note === s) note = ""; }, 2500);
  }

  function shareText(r: Recap): string {
    const lines = [t("KONİ · {month} özetim", { month: monthName(r.month) }),
      t("{time} izledim · {n} gün", { time: dur(r.watchSec), n: r.days })];
    if (r.peakHour >= 0) lines.push(t("En sık saat {h}", { h: hh(r.peakHour) }));
    lines.push(t("{m} film · {s} dizi", { m: r.movies, s: r.series }));
    if (r.genres.length) lines.push(t("Türler: {list}", { list: r.genres.map((g) => tx(g.name)).join(", ") }));
    if (r.mates.length) lines.push(t("En çok birlikte: {list}", { list: r.mates.map((m) => `${bare(m.name)} (${dur(m.sec)})`).join(", ") }));
    if (r.badges.length) lines.push(t("Rozetler: {list}", { list: r.badges.map((b) => tx(b.title)).join(", ") }));
    if (r.best) lines.push(t("Ekibin favorisi: {title} ★{stars}", { title: r.best.title, stars: stars(r.best.stars) }));
    return lines.join("\n");
  }

  function share() {
    if (!rc) return;
    navigator.clipboard.writeText(shareText(rc)).then(() => flash(t("Panoya kopyalandı")), () => flash(t("Kopyalanamadı")));
  }

  // Kart tuval üzerinde yeniden çizilir (afiş/avatar yok: dış resimler tuvali kirletir).
  function draw(r: Recap): string {
    const W = 1080, H = 1350, c = document.createElement("canvas");
    c.width = W; c.height = H;
    const g = c.getContext("2d")!;
    const font = (px: number, w = 600) => `${w} ${px}px Inter, "Segoe UI", system-ui, sans-serif`;
    const bg = g.createLinearGradient(0, 0, W, H);
    bg.addColorStop(0, "#1b1f2a"); bg.addColorStop(1, "#0b0c0e");
    g.fillStyle = bg; g.fillRect(0, 0, W, H);
    g.textBaseline = "top";
    g.fillStyle = "#0a84ff"; g.font = font(30, 800); g.fillText("KONİ", 80, 80);
    g.fillStyle = "#f5f5f7"; g.font = font(64, 850); g.fillText(monthName(r.month), 80, 124);

    const tiles: [string, string][] = [
      [dur(r.watchSec), t("izledin")], [String(r.days), t("gün izledin")],
      [r.peakHour >= 0 ? hh(r.peakHour) : "–", t("en sık saat")], [t("{m} / {s}", { m: r.movies, s: r.series }), t("film / dizi")],
    ];
    tiles.forEach(([v, l], i) => {
      const x = 80 + (i % 2) * 470, y = 250 + Math.floor(i / 2) * 170;
      g.fillStyle = "rgba(255,255,255,0.06)"; g.beginPath(); g.roundRect(x, y, 450, 150, 28); g.fill();
      g.fillStyle = "#f5f5f7"; g.font = font(54, 850); g.fillText(v, x + 32, y + 26);
      g.fillStyle = "#a1a1a6"; g.font = font(26, 500); g.fillText(l, x + 32, y + 96);
    });

    // saat dağılımı
    const bx = 80, by = 610, bw = 920, bh = 110, step = bw / 24;
    r.hours.forEach((v, h) => {
      const hgt = Math.max(4, (v / top) * bh);
      g.fillStyle = h === r.peakHour ? "#0a84ff" : "rgba(255,255,255,0.18)";
      g.beginPath(); g.roundRect(bx + h * step + 4, by + bh - hgt, step - 8, hgt, 4); g.fill();
    });
    g.fillStyle = "#6e6e73"; g.font = font(20, 500);
    [0, 6, 12, 18].forEach((h) => g.fillText(hh(h), bx + h * step, by + bh + 10));

    let y = 790;
    const row = (label: string, text: string) => {
      if (!text) return;
      g.fillStyle = "#6e6e73"; g.font = font(22, 700); g.fillText(label.toLocaleUpperCase(locale()), 80, y);
      g.fillStyle = "#f5f5f7"; g.font = font(34, 600);
      let s = text;
      while (g.measureText(s).width > 920 && s.length > 4) s = s.slice(0, -2);
      g.fillText(s === text ? s : s + "…", 80, y + 34);
      y += 112;
    };
    row(t("Türler"), r.genres.map((x) => tx(x.name)).join(" · "));
    row(t("En çok birlikte izlediğin"), r.mates.map((m) => `${bare(m.name)} ${dur(m.sec)}`).join(" · "));
    row(t("Rozetler"), r.badges.map((b) => tx(b.title)).join(" · "));
    if (r.best) row(t("Ekibin en yüksek puanı"), `${r.best.title}  ★${stars(r.best.stars)}`);
    return c.toDataURL("image/png");
  }

  async function savePng() {
    if (!rc) return;
    try {
      const p = await recapSavePng(rc.month, draw(rc));
      flash(t("Kaydedildi: {path}", { path: p }));
    } catch (e) {
      flash(t("Kaydedilemedi: {err}", { err: String(e) }));
    }
  }
</script>

{#if rc}
  <div class="lab">{t("Aylık özet")}
    <span class="seg per" role="tablist">
      <button role="tab" class:on={month === lastMonth} onclick={() => (month = lastMonth)}>{t("Geçen ay")}</button>
      <button role="tab" class:on={month === thisMonth} onclick={() => (month = thisMonth)}>{t("Bu ay")}</button>
    </span>
  </div>
  <div class="rc">
    <div class="hd">
      <span><small>KONİ</small><b>{monthName(rc.month)}</b></span>
      {#if !empty}
        <span class="btns">
          <button class="sm" onclick={share} title={t("Özeti metin olarak panoya kopyala")}>{t("Paylaş")}</button>
          <button class="sm white" onclick={savePng}>{t("PNG olarak kaydet")}</button>
        </span>
      {/if}
    </div>
    {#if empty}
      <p class="muted">{t("Bu ay henüz bir şey izlemedin.")}</p>
    {:else}
      <div class="tiles">
        <div><b>{dur(rc.watchSec)}</b><small>{t("izledin")}</small></div>
        <div><b>{rc.days}</b><small>{t("gün izledin")}</small></div>
        <div><b>{rc.peakHour >= 0 ? hh(rc.peakHour) : "–"}</b><small>{t("en sık saat")}</small></div>
        <div><b>{rc.movies} / {rc.series}</b><small>{t("film / dizi")}</small></div>
      </div>
      <div class="hours" aria-hidden="true">
        {#each rc.hours as v, h (h)}<i class:pk={h === rc.peakHour} style="height:{Math.max(4, (v / top) * 100)}%" title="{hh(h)} · {dur(v)}"></i>{/each}
      </div>
      <div class="grid">
        {#if rc.genres.length}
          <div><span class="k">{t("Türler")}</span>
            <span class="chips">{#each rc.genres as g (g.name)}<em>{tx(g.name)} · {g.count}</em>{/each}</span></div>
        {/if}
        {#if rc.mates.length}
          <div><span class="k">{t("En çok birlikte izlediğin")}</span>
            {#each rc.mates as m (m.name)}
              <span class="mate"><Avatar id={m.avatar} name={m.name} size={22} /><b>{bare(m.name)}</b><small>{dur(m.sec)} · {t("{n} oda", { n: m.rooms })}</small></span>
            {/each}</div>
        {/if}
        {#if rc.badges.length}
          <div><span class="k">{t("Rozetler")}</span>
            {#each rc.badges as b (b.id)}<span class="bd" title={tx(b.rule)}>{tx(b.title)}</span>{/each}</div>
        {/if}
        {#if rc.best}
          <div class="best"><span class="k">{t("Ekibin en yüksek puanı")}</span>
            <span class="bf">{#if rc.best.poster}<img src={rc.best.poster} alt="" />{/if}
              <span><b>{rc.best.title}</b><small>★ {stars(rc.best.stars)} · {t("{n} oy", { n: rc.best.votes })}</small></span></span></div>
        {/if}
      </div>
    {/if}
    {#if note}<p class="note">{note}</p>{/if}
  </div>
{/if}

<style>
  .lab { display: flex; justify-content: space-between; align-items: center; color: var(--text-2); font-size: 11.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; margin-top: 4px; }
  .per { text-transform: none; letter-spacing: 0; }
  .per button { padding: 3px 9px; font-size: 12px; font-weight: 600; }
  .rc { padding: 14px 16px; border-radius: 16px; background: linear-gradient(135deg, rgba(10, 132, 255, 0.10), var(--glass)); border: 1px solid var(--line); display: flex; flex-direction: column; gap: 12px; }
  .hd { display: flex; justify-content: space-between; align-items: center; gap: 10px; }
  .hd > span:first-child { display: flex; flex-direction: column; }
  .hd small { color: var(--accent); font-size: 10.5px; font-weight: 800; letter-spacing: 0.08em; }
  .hd b { font-size: 18px; font-weight: 800; text-transform: capitalize; }
  .btns { display: flex; gap: 6px; }
  button.sm { height: 30px; padding: 0 12px; border-radius: 10px; font-size: 12px; font-weight: 650; }
  .tiles { display: grid; grid-template-columns: repeat(4, 1fr); gap: 8px; }
  .tiles div { padding: 8px 12px; border-radius: 12px; background: var(--glass); }
  .tiles b { display: block; font-size: 19px; font-weight: 850; letter-spacing: -0.02em; font-variant-numeric: tabular-nums; }
  .tiles small { color: var(--text-2); font-size: 11px; }
  .hours { display: flex; align-items: flex-end; gap: 3px; height: 36px; }
  .hours i { flex: 1; border-radius: 3px; background: var(--glass-3); }
  .hours i.pk { background: var(--accent); }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px 16px; }
  .grid > div { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
  .k { color: var(--text-3); font-size: 10.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .chips em { font-style: normal; font-size: 11.5px; padding: 2px 8px; border-radius: 99px; background: var(--glass-2); }
  .mate { display: flex; align-items: center; gap: 7px; font-size: 12.5px; min-width: 0; }
  .mate small, .bf small { color: var(--text-2); font-size: 11px; }
  .bd { font-size: 12.5px; }
  .bf { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .bf img { width: 32px; height: 48px; object-fit: cover; border-radius: 6px; flex: none; }
  .bf > span { display: flex; flex-direction: column; min-width: 0; }
  .bf b { font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .muted { color: var(--text-2); font-size: 12.5px; margin: 0; }
  .note { color: var(--text-2); font-size: 11.5px; margin: 0; word-break: break-all; }
</style>
