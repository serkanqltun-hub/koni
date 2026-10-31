<script lang="ts">
  // Otomatik kaynak seçimi (mockup ui-plan/kaynak-avi.html): kaynak listesi gösterilmez.
  // Oynat'a basınca en yüksek kaliteden 720p'ye inerek Real-Debrid'de açılan ilk sağlam
  // kaynak bulunur (önbellekte olmayan ya da açılmayan atlanır), sonra kullanıcıya sorulur.
  import { untrack } from "svelte";
  import { debridCheck, type Source } from "./player";
  import { t, tx } from "./i18n.svelte";
  import { LADDER, failKind, huntOrder, type Rung, type SourceInfo } from "./sources";

  let { sources, kicker, heading, inRoom, onPlay, onAddon }: {
    sources: SourceInfo[];
    kicker: string;
    heading: string;
    inRoom: boolean;
    onPlay: (s: Source) => void;
    onAddon: () => void;
  } = $props();

  const TOP_KEY = "koni.topQuality";
  let top = $state<Rung>((() => {
    try { const v = localStorage.getItem(TOP_KEY); return LADDER.includes(v as Rung) ? (v as Rung) : "4K"; } catch { return "4K"; }
  })());
  $effect(() => { try { localStorage.setItem(TOP_KEY, top); } catch { /* yok */ } });

  let phase = $state<"ready" | "search" | "found" | "none" | "slow">("ready");
  /** Real-Debrid istek sınırı: kalan bekleme (sn); bitince kaldığı yerden devam. */
  const SLOW_WAIT = 30;
  let wait = $state(0);
  let timer: ReturnType<typeof setInterval> | undefined;
  let order = $state<SourceInfo[]>([]);
  /** Her sıradaki kaynağın sonucu. */
  let results = $state<("ok" | "fail" | undefined)[]>([]);
  let cur = $state(-1);
  let next = 0;
  let found = $state<number[]>([]);
  let view = $state(0);
  let exhausted = $state(false);
  let counts = $state({ cache: 0, bad: 0 });
  let stopErr = $state("");
  let token = 0;

  const rungs = $derived(LADDER.slice(LADDER.indexOf(top)));
  const shown = $derived(found.length ? order[found[view]] : null);

  function reset() {
    token++;
    clearInterval(timer);
    order = huntOrder(sources, top);
    results = [];
    cur = -1;
    next = 0;
    found = [];
    view = 0;
    exhausted = false;
    counts = { cache: 0, bad: 0 };
    stopErr = "";
  }
  // Başka bölüm/kaynak listesi gelince baştan.
  $effect(() => { void sources; untrack(() => { reset(); phase = "ready"; }); });

  async function run() {
    const my = ++token;
    phase = "search";
    while (next < order.length) {
      const i = next++;
      cur = i;
      try {
        await debridCheck(order[i].src);
        if (my !== token) return;
        results[i] = "ok";
        found = [...found, i];
        view = found.length - 1;
        cur = -1;
        phase = "found";
        return;
      } catch (e) {
        if (my !== token) return;
        const k = failKind(String(e));
        if (k === "slow") { next = i; cur = -1; pause(); return; }
        results[i] = "fail";
        if (k === "stop") { stopErr = String(e); break; }
        counts[k]++;
      }
    }
    cur = -1;
    exhausted = true;
    phase = found.length && !stopErr ? "found" : "none";
  }

  /** İstek sınırına takıldı: aynı kaynaktan devam etmek için bekle. */
  function pause() {
    phase = "slow";
    wait = SLOW_WAIT;
    clearInterval(timer);
    timer = setInterval(() => {
      if (--wait > 0) return;
      clearInterval(timer);
      run();
    }, 1000);
  }
  $effect(() => () => clearInterval(timer));

  function start() { reset(); run(); }
  function cancel() { reset(); phase = "ready"; }
  function another() {
    if (view < found.length - 1) view++;
    else run();
  }

  /** Basamak durumu: yes (şu an gösterilen), now (deneniyor), no (bitti, açılmadı), wait. */
  function rung(q: Rung) {
    const idx = order.map((s, i) => (s.quality === q ? i : -1)).filter((i) => i >= 0);
    const tried = idx.filter((i) => results[i]).length;
    const st = shown?.quality === q && phase === "found" ? "yes"
      : idx.includes(cur) ? "now"
      : !idx.length || tried === idx.length ? "no" : "wait";
    const small = !idx.length ? t("Kaynak yok")
      : st === "now" ? t("{a} / {b} deneniyor", { a: tried + 1, b: idx.length })
      : st === "no" ? t("{n} denendi · açılmadı", { n: tried })
      : st === "yes" ? t("Bulundu")
      : t("Sırada · {n} kaynak", { n: idx.length });
    return { st, small, p: idx.length ? ((tried + (st === "now" ? 0.5 : 0)) / idx.length) * 100 : 0 };
  }

  /** Bulunandan yukarıdaki basamaklarda neden açılmadığı. */
  const above = $derived.by(() => {
    if (!shown) return "";
    const up = rungs.slice(0, rungs.indexOf(shown.quality as Rung));
    if (!up.length) return "";
    const why = [counts.cache && t("{n} önbellekte yok", { n: counts.cache }), counts.bad && t("{n} bozuk", { n: counts.bad })].filter(Boolean).join(", ");
    return why ? t("{q}'da açılan kaynak yoktu ({why}).", { q: up.join(t(" ve ")), why }) : t("{q}'da açılan kaynak yoktu.", { q: up.join(t(" ve ")) });
  });
  const qcls = (q: string) => (q === "4K" ? "k4" : q === "2K" ? "k2" : q === "1080p" ? "hd" : "");
</script>

{#if phase === "ready"}
  <div class="play">
    <div class="row">
      <div class="grow"><span class="k">{kicker}</span><h3>{heading}</h3></div>
      <label class="sel" title={t("Aramaya bu kaliteden başlanır")}>{t("En iyi kalite:")}
        <select bind:value={top}>{#each LADDER as q (q)}<option value={q}>{q}</option>{/each}</select></label>
      <button class="white big" onclick={start}>▶ {t("Oynat")}</button>
    </div>
    <div class="why">{t("Addon'undan {n} kaynak geldi. Oynat'a basınca KONİ {top}'dan başlayıp açılan ilk sağlam kaynağı bulur, sonra sana sorar.", { n: sources.length, top })}</div>
  </div>
{:else if phase === "search"}
  <div class="play">
    <div class="row">
      <div class="grow"><span class="k">{kicker} · <span lang="en">{heading}</span></span><h3>{t("Oynatılabilir kaynak aranıyor…")}</h3></div>
      <button class="sm" onclick={found.length ? () => { token++; cur = -1; phase = "found"; } : cancel}>{t("Vazgeç")}</button>
    </div>
    <div class="lad" style="--n:{rungs.length}">
      {#each rungs as q (q)}
        {@const r = rung(q)}
        <div class="st4 {r.st}" style="--p:{r.p}%"><span class="q"><span class="q-{qcls(q) || 'sd'}">{q}</span><i class="ic">{r.st === "no" ? "✕" : r.st === "yes" ? "✓" : ""}</i></span><small>{r.small}</small></div>
      {/each}
    </div>
    <div class="row g8">
      <div class="live grow"><i class="pulse"></i><span>{t("Bağlantı deneniyor")} · {cur >= 0 ? order[cur].release : ""}</span></div>
      <span class="chip"><b>{counts.cache}</b> {t("önbellekte yok")}</span><span class="chip"><b>{counts.bad}</b> {t("bozuk")}</span>
    </div>
  </div>
{:else if phase === "slow"}
  <div class="play slow">
    <div class="row">
      <div class="grow">
        <span class="k slowk">{t("Servis yavaşlattı")}</span>
        <h3>{t("Çok hızlı denedik; {n} sn sonra kaldığı yerden devam", { n: wait })}</h3>
      </div>
      <button onclick={cancel}>{t("Vazgeç")}</button>
      <button class="white" disabled>↻ {t("Devam")} · {wait}</button>
    </div>
    <div class="row g8"><span class="chip"><b>{results.filter(Boolean).length}</b> / {order.length} {t("denendi")}</span><span class="chip"><b>{counts.cache}</b> {t("önbellekte yok")}</span><span class="chip"><b>{counts.bad}</b> {t("bozuk")}</span>
      <span class="why">{t("Servis dakikada sınırlı istek kabul ediyor. Denenenler tekrar denenmez.")}</span></div>
  </div>
{:else if phase === "found" && shown}
  <div class="play ok">
    <div class="found">
      <span class="qbig {qcls(shown.quality)}">{shown.quality}</span>
      <div class="grow">
        <span class="k okk">✓ {view > 0 ? t("{n}. kaynak bulundu", { n: view + 1 }) : found[0] === 0 ? t("{q} kaynak bulundu · ilk denemede", { q: shown.quality }) : t("Kaynak bulundu")}</span>
        <div class="rel" title={shown.release}>{shown.release}</div>
        <div class="tags">
          {#each shown.tags as tg (tg)}<span>{tg}</span>{/each}
          {#if shown.size}<span>{shown.size}</span>{/if}
          {#if shown.trDub}<span class="tr">{t("TR dublaj")}</span>{/if}
          {#if shown.trSub}<span class="tr">{t("TR altyazı")}</span>{/if}
        </div>
      </div>
      <div class="acts">
        {#if view > 0}<button onclick={() => view--}>‹ {t("Öncekine dön")}</button>{/if}
        {#if view < found.length - 1 || !exhausted}<button onclick={another}>↻ {t("Başka kaynak")}</button>{/if}
        <button class="white big" onclick={() => onPlay(shown.src)}>▶ {t("Oynat")}</button>
      </div>
    </div>
    <div class="why">
      {#if exhausted && view === found.length - 1}{t("Başka sağlam kaynak kalmadı.")}{/if}
      {above}
      {#if shown.quality === "4K"}{t("4K yüksek hız ister (≈ 25 Mbit/s); odada interneti yetmeyen olursa \"Başka kaynak\"la alt kaliteye inebilirsin.")}{/if}
      {#if inRoom}{t("Oynat'a basınca odada herkeste açılır; herkes kendi addon'u ya da hesabıyla izler.")}{/if}
    </div>
  </div>
{:else}
  <div class="play bad">
    <div class="row">
      <div class="grow">
        <span class="k badk">{t("Oynatılabilir kaynak bulunamadı")}</span>
        <h3>{stopErr ? tx(stopErr) : order.length ? t("{n} kaynağın hiçbiri açılmadı", { n: order.length }) : t("{q}–720p arasında paylaşılabilir kaynak yok", { q: rungs[0] })}</h3>
      </div>
      <button onclick={onAddon}>{t("Addon ekle")}</button>
      <button class="white" onclick={start}>↻ {t("Tekrar dene")}</button>
    </div>
    {#if order.length && !stopErr}
      <div class="row g8"><span class="chip"><b>{counts.cache}</b> {t("önbellekte yok")}</span><span class="chip"><b>{counts.bad}</b> {t("bozuk")}</span>
        <span class="why">{t("Kaynaklar sık değişir; birkaç dakika sonra yeniden denemek işe yarayabilir.")}</span></div>
    {/if}
  </div>
{/if}


<!-- Listeyi kendisi görmek isteyen için: varsayılan kapalı. -->
{#if sources.length}
  <details class="manual">
    <summary>{t("Kaynakları kendim seçeyim")} <span>· {sources.length}</span></summary>
    <ul>
      {#each sources as s, i (i)}
        {@const r = results[order.indexOf(s)]}
        <li class:tried={r === "fail"}>
          <span class="q q-{qcls(s.quality) || 'sd'}">{s.quality}</span>
          <span class="mrel" title={s.release}>{s.release}</span>
          <span class="mtags">
            {#if s.size}<span>{s.size}</span>{/if}
            {#if s.seeds != null}<span>{s.seeds} seed</span>{/if}
            {#if s.trDub}<span class="tr">{t("TR dublaj")}</span>{/if}
            {#if s.trSub}<span class="tr">{t("TR altyazı")}</span>{/if}
            {#if r === "fail"}<span class="x">{t("açılmadı")}</span>{/if}
          </span>
          <button class="sm" disabled={!s.ok} onclick={() => onPlay(s.src)}>▶ {t("Oynat")}</button>
        </li>
      {/each}
    </ul>
  </details>
{/if}

<style>
  .play { border-radius: 20px; border: 1px solid var(--line-2); background: linear-gradient(120deg, rgba(10, 132, 255, 0.16), rgba(255, 255, 255, 0.03) 60%); padding: 18px 20px; display: flex; flex-direction: column; gap: 14px; }
  .play.ok { background: linear-gradient(120deg, rgba(48, 209, 88, 0.16), rgba(255, 255, 255, 0.03) 60%); border-color: rgba(48, 209, 88, 0.45); }
  .play.slow { background: linear-gradient(120deg, rgba(255, 159, 10, 0.14), rgba(255, 255, 255, 0.03) 60%); border-color: rgba(255, 159, 10, 0.4); }
  .slowk { color: var(--orange); }
  .play.bad { background: linear-gradient(120deg, rgba(255, 105, 97, 0.13), rgba(255, 255, 255, 0.03) 60%); border-color: rgba(255, 105, 97, 0.35); }
  .row { display: flex; align-items: center; gap: 14px; }
  .row.g8 { gap: 8px; }
  .grow { flex: 1; min-width: 0; }
  .k { color: var(--text-2); font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em; }
  .okk { color: var(--green); } .badk { color: var(--red); }
  h3 { margin: 2px 0 0; font-size: 20px; font-weight: 850; letter-spacing: -0.02em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .big { height: 50px; padding: 0 26px; font-size: 15.5px; font-weight: 800; border-radius: 15px; }
  .sm { height: 34px; padding: 0 12px; font-size: 12.5px; }
  .why { font-size: 12px; color: var(--text-3); }
  .sel { display: inline-flex; align-items: center; gap: 6px; padding: 0 4px 0 11px; height: 34px; border-radius: 10px; border: 1px solid var(--line-2); background: rgba(0, 0, 0, 0.3); font-weight: 600; font-size: 12.5px; color: var(--text-2); }
  .sel select { background: transparent; border: 0; color: var(--text); font: inherit; font-weight: 700; padding: 0 4px; height: 30px; }
  .sel option { background: #1b1c20; }
  .chip { display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: 99px; background: var(--glass-2); border: 1px solid var(--line-2); font-size: 12px; font-weight: 600; color: var(--text-2); white-space: nowrap; }
  .chip b { color: var(--text); }

  .lad { display: grid; grid-template-columns: repeat(var(--n), 1fr); gap: 8px; }
  .st4 { border-radius: 14px; padding: 10px 12px; border: 1px solid var(--line); background: rgba(0, 0, 0, 0.25); display: flex; flex-direction: column; gap: 3px; position: relative; overflow: hidden; }
  .st4 .q { font-weight: 900; font-size: 15px; display: flex; align-items: center; gap: 8px; }
  .st4 small { color: var(--text-3); font-size: 11.5px; }
  .ic { margin-left: auto; width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; font-size: 11px; font-weight: 900; font-style: normal; box-sizing: border-box; }
  .st4.no { opacity: 0.62; } .st4.no .ic { background: rgba(255, 105, 97, 0.18); color: var(--red); }
  .st4.now { border-color: rgba(10, 132, 255, 0.6); background: rgba(10, 132, 255, 0.12); }
  .st4.now .ic { border: 2.5px solid rgba(10, 132, 255, 0.25); border-top-color: var(--accent); animation: sp 0.9s linear infinite; }
  .st4.now::after { content: ""; position: absolute; left: 0; bottom: 0; height: 3px; width: var(--p); background: var(--accent); transition: width 0.3s; }
  .st4.yes { border-color: rgba(48, 209, 88, 0.55); background: rgba(48, 209, 88, 0.12); } .st4.yes .ic { background: var(--green); color: #000; }
  .st4.wait .ic { border: 1.5px dashed var(--text-3); }
  @keyframes sp { to { transform: rotate(360deg); } }
  .q-k4 { color: var(--yellow); } .q-k2 { color: var(--orange); } .q-hd { color: #64d2ff; } .q-sd { color: var(--text-2); }

  .live { display: flex; align-items: center; gap: 10px; font: 500 12px var(--font-mono); color: var(--text-2); }
  .pulse { width: 7px; height: 7px; border-radius: 50%; background: var(--accent); box-shadow: 0 0 0 4px rgba(10, 132, 255, 0.2); flex: none; }
  .live span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  .found { display: flex; align-items: center; gap: 16px; }
  .qbig { width: 74px; height: 74px; border-radius: 18px; display: grid; place-items: center; font-weight: 900; font-size: 19px; flex: none; background: rgba(255, 255, 255, 0.1); }
  .qbig.hd { background: rgba(100, 210, 255, 0.14); color: #64d2ff; border: 1px solid rgba(100, 210, 255, 0.35); }
  .qbig.k2 { background: rgba(255, 159, 10, 0.16); color: var(--orange); border: 1px solid rgba(255, 159, 10, 0.4); }
  .qbig.k4 { background: linear-gradient(135deg, #ffd60a, #ff9f0a); color: #000; }
  .rel { margin-top: 4px; font: 600 13px var(--font-mono); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tags { display: flex; gap: 6px; flex-wrap: wrap; margin-top: 7px; }
  .tags span { padding: 2px 8px; border-radius: 6px; background: var(--glass-2); font-size: 11.5px; font-weight: 600; color: var(--text-2); }
  .tags span.tr { background: rgba(255, 55, 95, 0.16); color: #ff8aa5; }
  .acts { display: flex; gap: 8px; flex: none; }
  .acts button:not(.big) { height: 44px; padding: 0 16px; font-weight: 700; }

  .manual { margin-top: 10px; }
  .manual summary { cursor: pointer; width: fit-content; font-size: 12.5px; font-weight: 600; color: var(--text-3); padding: 4px 2px; list-style: none; }
  .manual summary::before { content: "›"; display: inline-block; margin-right: 6px; transition: transform 0.15s; }
  .manual[open] summary::before { transform: rotate(90deg); }
  .manual summary:hover { color: var(--text-2); }
  .manual summary span { color: var(--text-3); font-weight: 500; }
  .manual ul { list-style: none; margin: 6px 0 0; padding: 0; display: flex; flex-direction: column; gap: 4px; max-height: 360px; overflow-y: auto; }
  .manual li { display: flex; align-items: center; gap: 12px; padding: 7px 8px 7px 12px; border-radius: 10px; border: 1px solid var(--line); background: rgba(0, 0, 0, 0.2); }
  .manual li.tried { opacity: 0.55; }
  .manual .q { font-weight: 900; font-size: 12.5px; width: 44px; flex: none; }
  .mrel { flex: 1; min-width: 0; font: 500 12px var(--font-mono); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--text-2); }
  .mtags { display: flex; gap: 5px; flex: none; }
  .mtags span { padding: 1px 7px; border-radius: 6px; background: var(--glass-2); font-size: 11px; font-weight: 600; color: var(--text-3); white-space: nowrap; }
  .mtags span.tr { background: rgba(255, 55, 95, 0.16); color: #ff8aa5; }
  .mtags span.x { background: rgba(255, 105, 97, 0.14); color: var(--red); }
</style>
