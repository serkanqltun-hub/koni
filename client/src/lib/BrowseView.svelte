<script lang="ts">
  // Keşfet: oda için tarayıcı. Tarayıcı ayrı bir native penceredir; `frame` kutusunun
  // üstüne oturur (browseEmbed), üstüne bir şey çizilemez: bildirimler adres çubuğunun
  // altında bant olarak çıkar. "KONİ'de izle": önce yt-dlp (KONİ'nin oynatıcısında açılır),
  // tanımazsa video sitenin oynatıcısında kalır ve odada herkeste eşitlenir (tarayıcıda senkron).
  import { onMount } from "svelte";
  import { browseOpen, browseEmbed, browseNav, browseState, snifferClose, syncPage, syncPlay, syncSeek, type BrowseState } from "./player";
  import Icon from "./Icon.svelte";
  import { t, tx, locale } from "./i18n.svelte";

  type Person = { id: string; name: string; me: boolean; ready: boolean };
  let { inRoom, page, people, pending, onProbe, onPendingTaken, onClose, onDiscover, onError }: {
    inRoom: boolean;
    /** Oda medyası `page` ise: herkesin açacağı sayfa. */
    page: { src: string; title: string } | null;
    people: Person[];
    /** Başka yerden (Video ekle) açılan, yt-dlp'nin tanımadığı adres. */
    pending: string | null;
    /** yt-dlp ile aç: başlık ya da `null` (site desteklenmiyor). Hata fırlatabilir. */
    onProbe: (url: string) => Promise<string | null>;
    onPendingTaken: () => void;
    /** Tarayıcıda senkronu herkes için bitir (oda medyası kapanır). */
    onClose: () => void;
    onDiscover: () => void;
    onError: (msg: string) => void;
  } = $props();

  const SITES = [
    { name: "YouTube", url: "https://www.youtube.com", dot: "#ff0033", ch: "▶" },
    { name: "Vimeo", url: "https://vimeo.com/watch", dot: "#1ab7ea", ch: "v" },
    { name: "Dailymotion", url: "https://www.dailymotion.com", dot: "#0066dc", ch: "d" },
    { name: "Internet Archive", url: "https://archive.org/details/movies", dot: "#444", ch: "A" },
  ];

  type Recent = { url: string; title: string; how: "player" | "page" };
  const loadRecent = (): Recent[] => {
    try { return JSON.parse(localStorage.getItem("koni.recentPages") ?? "[]"); } catch { return []; }
  };
  let recent = $state<Recent[]>(loadRecent());
  function remember(r: Recent) {
    recent = [r, ...recent.filter((x) => x.url !== r.url)].slice(0, 8);
    try { localStorage.setItem("koni.recentPages", JSON.stringify(recent)); } catch {}
  }

  let st = $state<BrowseState>({ url: null, title: "", video: null, blocked: 0 });
  let q = $state("");
  let editing = $state(false);
  /** `probe`: yt-dlp deniyor; `opened`: KONİ'nin oynatıcısında açıldı; `wait`: sayfada oynatılsın; `solo`: odada değil. */
  let phase = $state<"idle" | "probe" | "opened" | "wait" | "solo">("idle");
  let openedTitle = $state("");
  let waitUrl = "";
  /** Oynatıcıda açılamadıysa nedeni (bantta görünür). */
  let waitNote = $state("");
  let shieldOpen = $state(false);
  let frame: HTMLDivElement;
  /** Oda sayfasını bir kez aç (her `src` için). */
  let openedFor = "";

  const live = $derived(page !== null);
  const mine = $derived(live && st.video !== null);
  const readyCount = $derived(people.filter((p) => p.ready).length);

  const looksUrl = (s: string) => /^https?:\/\//i.test(s) || /^[\w-]+(\.[\w-]+)+(\/\S*)?$/.test(s);
  const split = (u: string | null) => {
    if (!u) return { host: "", path: "" };
    try { const x = new URL(u); return { host: x.host.replace(/^www\./, ""), path: x.pathname + x.search }; } catch { return { host: u, path: "" }; }
  };
  const addr = $derived(split(st.url));
  const sameUrl = (a: string | null, b: string) => !!a && a.replace(/\/$/, "") === b.replace(/\/$/, "");

  function go(url: string) {
    phase = "idle";
    browseOpen(url).catch((e) => fail(String(e)));
  }
  function submit() {
    const s = q.trim();
    if (!s) return;
    editing = false;
    (document.activeElement as HTMLElement | null)?.blur();
    go(looksUrl(s) ? s : "https://www.youtube.com/results?search_query=" + encodeURIComponent(s));
  }

  async function watch() {
    const url = st.url;
    if (!url || phase === "probe" || live) return;
    phase = "probe";
    err = "";
    try {
      const title = await onProbe(url);
      if (title !== null) {
        openedTitle = title;
        phase = "opened";
        remember({ url, title, how: "player" });
        return;
      }
      waitNote = "";
      waitUrl = url;
      phase = "wait";
    } catch (e) {
      const msg = String(e).replace(/^Error:\s*/, "");
      if (/başka bir video açılıyor/.test(msg)) { phase = "idle"; fail(msg); return; }
      // yt-dlp siteyi tanıdı ama açamadı (YouTube "bot değilim", Vimeo giriş ister…):
      // video sayfada oynatılır, herkeste yine eşitlenir.
      waitNote = msg;
      waitUrl = url;
      phase = "wait";
    }
  }

  // Sayfada video başladı: odadaysa tarayıcıda senkron başlar, kaldığı yerden.
  $effect(() => {
    const v = st.video;
    if (phase !== "wait" || !v || v.paused) return;
    const url = st.url ?? waitUrl;
    const title = st.title || split(url).host;
    remember({ url, title, how: "page" });
    if (!inRoom) { phase = "solo"; return; }
    phase = "idle";
    openedFor = url;
    const pos = v.pos;
    syncPage(url, title)
      .then(() => (pos > 2 ? syncSeek(pos) : syncPlay()))
      .catch((e) => fail(String(e)));
  });

  // Oda sayfaya geçti: herkes aynı sayfayı açar (açansa zaten oradadır).
  $effect(() => {
    const p = page;
    if (!p || p.src === openedFor) return;
    openedFor = p.src;
    phase = "idle";
    if (!sameUrl(st.url, p.src)) browseOpen(p.src).catch((e) => fail(String(e)));
  });

  // Video ekle'den gelen, yt-dlp'nin tanımadığı adres.
  $effect(() => {
    const u = pending;
    if (!u) return;
    onPendingTaken();
    waitUrl = u;
    browseOpen(u).then(() => (phase = "wait")).catch((e) => fail(String(e)));
  });

  const isYtWatch = (u: string | null) =>
    !!u && /^https?:\/\/(www\.|m\.)?youtube\.com\/(watch\?.*v=|shorts\/)/i.test(u);

  // Adres değişti: eski sayfanın bandı kapanır; YouTube videosuna geçildiyse otomatik senkron başlar.
  let lastUrl: string | null = null;
  $effect(() => {
    const u = st.url;
    if (u === lastUrl) return;
    const prev = lastUrl;
    lastUrl = u;
    if (!editing) q = u ?? "";
    if (u && (isYtWatch(u) || (prev && phase === "wait"))) {
      // YouTube videosu açıldı veya beklerken listeden videoya geçildi: otomatik eşitle.
      waitNote = "";
      phase = "idle";
      queueMicrotask(watch);
    } else if (prev && u && phase !== "wait" && phase !== "probe") {
      phase = "idle";
    }
    if (!u) { phase = "idle"; shieldOpen = false; }
  });

  /** Gömülü tarayıcının üstüne bir şey çizilemez: hata bantta görünür. */
  let err = $state("");
  function fail(m: string) {
    err = m.replace(/^Error:\s*/, "");
    onError(err);
  }

  function cancelWait() { phase = "idle"; }
  function home() { phase = "idle"; snifferClose().catch(() => {}); }

  const fmt = (s: number | null | undefined) => {
    if (s == null || !isFinite(s)) return "--:--";
    const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), x = Math.floor(s % 60);
    const p = (n: number) => String(n).padStart(2, "0");
    return h ? `${h}:${p(m)}:${p(x)}` : `${m}:${p(x)}`;
  };
  const short = (m: string) => {
    const s = m.replace(/^ERROR:\s*/i, "").replace(/\s+/g, " ").trim();
    return s.length > 90 ? s.slice(0, 90) + "…" : s;
  };
  const initial = (n: string) => (n.trim()[0] ?? "?").toLocaleUpperCase(locale());

  onMount(() => {
    const send = () => {
      const r = frame.getBoundingClientRect();
      browseEmbed({ x: r.left, y: r.top, w: r.width, h: r.height }).catch(() => {});
    };
    const ro = new ResizeObserver(send);
    ro.observe(frame);
    send();
    let alive = true;
    const poll = async () => {
      if (!alive) return;
      try { st = await browseState(); } catch {}
      if (alive) setTimeout(poll, 700);
    };
    poll();
    return () => {
      alive = false;
      ro.disconnect();
      browseEmbed(null).catch(() => {});
      snifferClose().catch(() => {});
    };
  });
</script>

<div class="browse">
  <form class="addr" onsubmit={(e) => { e.preventDefault(); submit(); }}>
    {#if st.url}
      <button type="button" class="ib" title={t("Ana sayfa")} onclick={home}><Icon name="home" size={15} /></button>
      <button type="button" class="ib" title={t("Geri")} onclick={() => browseNav("back").catch(() => {})}><Icon name="back" /></button>
      <button type="button" class="ib" title={t("İleri")} onclick={() => browseNav("forward").catch(() => {})}><Icon name="fwd" /></button>
      <button type="button" class="ib" title={t("Yenile")} onclick={() => browseNav("reload").catch(() => {})}><Icon name="reload" size={15} /></button>
    {/if}
    <label class="url">
      {#if st.url && !editing}
        <span class="shown" lang="en"><span class="host">{addr.host}</span><span class="path">{addr.path}</span></span>
      {/if}
      <input class:hide={!!st.url && !editing} placeholder={t("Adres yaz ya da YouTube'da ara")} bind:value={q}
        onfocus={(e) => { editing = true; (e.currentTarget as HTMLInputElement).select(); }}
        onblur={() => { editing = false; q = st.url ?? q; }} />
      {#if st.url}
        <button type="button" class="shield" title={t("Reklam engelleyici")} onclick={() => (shieldOpen = !shieldOpen)}>
          <Icon name="shield" size={13} /><b>{st.blocked}</b> {t("reklam engellendi")}
        </button>
      {/if}
    </label>
    {#if live}
      <span class="koni live"><i></i>{t("Senkronda")} · {readyCount}/{people.length}</span>
    {:else if phase === "probe" || phase === "wait"}
      <button type="button" class="koni busy" onclick={phase === "wait" ? cancelWait : undefined}><span class="sp"></span>{t("Video aranıyor…")}</button>
    {:else}
      <button type="button" class="koni" class:dim={!st.url} class:pulse={!!st.video && phase === "idle"} disabled={!st.url} onclick={watch}><i></i>{t("KONİ'de izle")}</button>
    {/if}
  </form>

  {#if err}
    <div class="band bad">
      <span class="ic">!</span>
      <div class="grow"><b>{t("Olmadı")}</b><small>{short(tx(err))}</small></div>
      <button type="button" class="btn" onclick={() => (err = "")}>{t("Tamam")}</button>
    </div>
  {/if}

  {#if shieldOpen && st.url}
    <div class="band">
      <span class="ic"><Icon name="shield" size={15} /></span>
      <div class="grow"><b>{t("Reklam engelleyici açık · bu sayfada {n} reklam ve izleyici engellendi", { n: st.blocked })}</b><small>{t("Reklam ve izleyici adresleri yüklenmez, reklam sitesine yönlendirme ve açılır pencereler durdurulur.")}</small></div>
      <button type="button" class="btn" onclick={() => (shieldOpen = false)}>{t("Tamam")}</button>
    </div>
  {/if}

  {#if phase === "probe"}
    <div class="band wait">
      <span class="ic"><span class="sp"></span></span>
      <div class="grow"><b>{t("Video aranıyor")}</b><small>{inRoom ? t("KONİ bu sayfadaki videoyu tanırsa kendi oynatıcısında herkese açar: tam senkron, altyazı, kalite.") : t("KONİ bu sayfadaki videoyu tanırsa kendi oynatıcısında açar: tam senkron, altyazı, kalite.")}</small></div>
    </div>
  {:else if phase === "opened"}
    <div class="band wait">
      <span class="ic"><Icon name="film" size={15} /></span>
      <div class="grow"><b>{inRoom ? t("Video bulundu · Sinema'da herkeste açılıyor") : t("Video bulundu · Sinema'da açılıyor")}</b><small>{t("KONİ'nin oynatıcısında açılır. Sayfa Keşfet'te açık kalır.")}</small></div>
      <div class="vid"><span></span><div><b>{openedTitle}</b><small lang="en">{addr.host}</small></div></div>
    </div>
  {:else if phase === "wait"}
    <div class="band wait">
      <span class="ic"><Icon name="play" size={15} /></span>
      <div class="grow"><b>{t("Videoyu sayfada başlat")}</b><small>{#if waitNote}{t("Oynatıcıda açılamadı ({why}).", { why: short(tx(waitNote)) })}{:else}{t("KONİ bu siteyi tanımadı.")}{/if} {inRoom ? t("Sitenin oynatıcısında ▶'e bas: video bulununca odadakilere aynı sayfa açılır, oynatma, durdurma ve sarma herkeste aynı anda olur. Video sitenin oynatıcısında kalır.") : t("Sitenin oynatıcısında ▶'e bas: video bulununca odada herkesle eşitlenebilir. Video sitenin oynatıcısında kalır.")}</small></div>
      <div class="steps">
        <div class="step no"><i>✕</i>{t("Oynatıcı")}</div>
        <div class="step now"><i></i>{t("Video bekleniyor")}</div>
      </div>
      <button type="button" class="btn" onclick={cancelWait}>{t("Vazgeç")}</button>
    </div>
  {:else if phase === "solo"}
    <div class="band">
      <span class="ic"><Icon name="play" size={15} /></span>
      <div class="grow"><b>{t("Video sayfada oynuyor")}</b><small>{t("Tek başınasın, sayfada izle. Bir odadayken KONİ'de izle'ye basınca bu sayfa herkeste eşitlenir.")}</small></div>
      <button type="button" class="btn" onclick={() => (phase = "idle")}>{t("Tamam")}</button>
    </div>
  {/if}

  {#if live}
    {#if mine}
      <div class="band">
        <span class="ic"><Icon name="sync" size={15} /></span>
        <div class="grow"><b>{t("Tarayıcıda senkron")}</b><small>{t("Bu sayfadaki video herkeste aynı anda oynuyor. Sitenin oynatıcısını kullan; durdurunca, sarınca herkes seninle.")}</small></div>
        <span class="time">{fmt(st.video?.pos)} / {fmt(st.video?.dur)}</span>
        <div class="ppl">
          {#each people as p (p.id)}
            <span class="pp"><i>{initial(p.name)}</i>{p.me ? t("Sen") : p.name}{#if !p.me}<u class:o={!p.ready}>{p.ready ? "✓" : t("bekleniyor")}</u>{/if}</span>
          {/each}
        </div>
        <button type="button" class="btn" onclick={onClose}>{t("Senkronu bitir")}</button>
      </div>
    {:else}
      <div class="band hold">
        <span class="ic"><Icon name="pause" size={15} /></span>
        <div class="grow"><b>{t("Sayfa açıldı · video bekleniyor")}</b><small>{t("Odada bu sayfadaki video oynatılıyor. Senin sayfanda video bulununca kendiliğinden eşitlenir; giriş ya da çerez sorarsa sen onayla, gerekirse ▶'e bas.")}</small></div>
        <div class="ppl"><span class="pp"><i>{initial(people.find((p) => p.me)?.name ?? "S")}</i>{t("Sen")} <u class="o">{t("video aranıyor")}</u></span></div>
      </div>
    {/if}
  {/if}

  <div class="frame" bind:this={frame}>
    {#if !st.url}
      <div class="home">
        <div>
          <h1>{t("Nerede izliyoruz?")}</h1>
          <p>{t("Bir siteye git, videoyu bul, adres çubuğunun yanındaki")} <b>{t("KONİ'de izle")}</b>{t("'ye bas.")} {inRoom ? t("Odadaki herkes aynı anda izler.") : t("Odadayken herkes aynı anda izler.")}</p>
        </div>
        <div>
          <span class="k">{t("Siteler")}</span>
          <div class="tiles">
            {#each SITES as s (s.url)}
              <button class="tile" onclick={() => go(s.url)}>
                <span class="dot" style:background={s.dot}>{s.ch}</span>
                <span><b lang="en">{s.name}</b><small>{t("Doğrudan oynatıcıda")}</small></span>
              </button>
            {/each}
            {#if inRoom}
              <button class="tile accent" onclick={onDiscover}>
                <span class="dot spark"><Icon name="spark" size={13} /></span>
                <span><b>{t("Ne izlesek?")}</b><small>{t("Odayla birlikte seç")}</small></span>
              </button>
            {/if}
          </div>
        </div>
        <div class="how">
          <div><em>1</em><span><b>{t("Siteye git")}</b><small>{t("Adres yaz ya da yukarıdan ara. Reklamlar engellenir.")}</small></span></div>
          <div><em>2</em><span><b>{t("KONİ'de izle'ye bas")}</b><small>{t("KONİ videoyu tanırsa kendi oynatıcısında herkese açar.")}</small></span></div>
          <div><em>3</em><span><b>{t("Tanımazsa sayfada oynat")}</b><small>{t("Video sitenin oynatıcısında kalır, herkeste eşitlenir.")}</small></span></div>
        </div>
        {#if recent.length}
          <div>
            <span class="k">{t("Son açılanlar")}</span>
            <div class="recent">
              {#each recent.slice(0, 4) as r (r.url)}
                <button class="rc" title={r.url} onclick={() => go(r.url)}>
                  <span class="th"><Icon name={r.how === "page" ? "globe" : "film"} size={18} /></span>
                  <span class="tx"><b>{r.title}</b><small><span lang="en">{split(r.url).host}</span> · {r.how === "page" ? t("tarayıcıda") : t("oynatıcıda")}</small></span>
                </button>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    {:else}
      <div class="loading">{t("Sayfa açılıyor…")}</div>
    {/if}
  </div>
</div>

<style>
  .browse { grid-area: video; display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--bg); }
  .addr { display: flex; align-items: center; gap: 8px; padding: 12px 16px; flex: none; }
  .ib {
    width: 38px; height: 38px; padding: 0; border-radius: 11px; border: 1px solid var(--line); background: var(--glass, var(--raise));
    display: grid; place-items: center; color: var(--text-2); flex: none;
  }
  .url {
    flex: 1; height: 40px; border-radius: 12px; border: 1px solid var(--line-2); background: rgba(0, 0, 0, 0.35);
    display: flex; align-items: center; gap: 10px; padding: 0 8px 0 14px; font-size: 14px; min-width: 0; position: relative; cursor: text;
  }
  .url:focus-within { border-color: var(--accent); }
  .shown { flex: 1; display: flex; min-width: 0; white-space: nowrap; pointer-events: none; }
  .host { color: var(--text); }
  .path { color: var(--text-3); overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .browse .addr .url input { flex: 1; min-width: 0; border: 0; background: transparent; padding: 0; height: 100%; outline: none; box-shadow: none; border-radius: 0; font-size: 14px; }
  .browse .addr .url input.hide { position: absolute; inset: 0; opacity: 0; }
  .shield {
    display: inline-flex; align-items: center; gap: 6px; height: 28px; padding: 0 10px; border-radius: 8px; border: 0;
    background: rgba(48, 209, 88, 0.12); color: var(--green); font-weight: 700; font-size: 12px; flex: none; position: relative; z-index: 1;
  }
  .shield b { color: #fff; font-weight: 800; }
  .koni {
    display: inline-flex; align-items: center; gap: 9px; height: 40px; padding: 0 16px 0 12px; border-radius: 12px; border: 0;
    background: #fff; color: #000; font-weight: 800; font-size: 13.5px; flex: none; transition: box-shadow 0.25s;
  }
  .koni i { width: 14px; height: 14px; border-radius: 4px; background: var(--accent); transform: rotate(45deg) scale(0.8); flex: none; }
  .koni.pulse { box-shadow: 0 0 0 4px rgba(10, 132, 255, 0.35), 0 8px 30px rgba(10, 132, 255, 0.35); }
  .koni.busy { background: rgba(10, 132, 255, 0.18); color: #fff; border: 1px solid rgba(10, 132, 255, 0.6); }
  .koni.live { background: rgba(48, 209, 88, 0.16); color: #fff; border: 1px solid rgba(48, 209, 88, 0.6); }
  .koni.live i { background: var(--green); border-radius: 50%; transform: none; width: 8px; height: 8px; }
  .koni.dim, .koni:disabled { background: var(--glass-2, var(--raise)); color: var(--text-2); }
  .koni.dim i { background: var(--text-3); }
  .sp { width: 14px; height: 14px; border-radius: 50%; border: 2px solid rgba(255, 255, 255, 0.2); border-top-color: #fff; animation: r 0.9s linear infinite; flex: none; }
  @keyframes r { to { transform: rotate(360deg); } }

  .band {
    margin: 0 16px 10px; border-radius: 14px; padding: 10px 12px; display: flex; align-items: center; gap: 12px; flex: none;
    border: 1px solid rgba(48, 209, 88, 0.45); background: linear-gradient(90deg, rgba(48, 209, 88, 0.18), rgba(48, 209, 88, 0.04));
  }
  .band.wait { border-color: rgba(10, 132, 255, 0.5); background: linear-gradient(90deg, rgba(10, 132, 255, 0.18), rgba(10, 132, 255, 0.04)); }
  .band.bad { border-color: rgba(255, 105, 97, 0.5); background: linear-gradient(90deg, rgba(255, 105, 97, 0.16), rgba(255, 105, 97, 0.03)); }
  .band.bad .ic { background: rgba(255, 105, 97, 0.22); color: var(--red); font-weight: 900; }
  .band.hold { border-color: rgba(255, 159, 10, 0.5); background: linear-gradient(90deg, rgba(255, 159, 10, 0.18), rgba(255, 159, 10, 0.04)); }
  .band .ic { width: 34px; height: 34px; border-radius: 10px; background: rgba(48, 209, 88, 0.22); display: grid; place-items: center; flex: none; }
  .band.wait .ic { background: rgba(10, 132, 255, 0.22); }
  .band.hold .ic { background: rgba(255, 159, 10, 0.22); }
  .band b { font-size: 13.5px; display: block; }
  .band small { color: var(--text-2); font-size: 12px; }
  .grow { flex: 1; min-width: 0; }
  .btn {
    height: 38px; padding: 0 14px; border-radius: 11px; border: 1px solid var(--line-2); background: var(--glass-2, var(--raise));
    font-weight: 700; font-size: 13px; white-space: nowrap; flex: none;
  }
  .vid { display: flex; gap: 10px; align-items: center; padding: 6px 10px 6px 6px; border-radius: 12px; background: rgba(0, 0, 0, 0.3); border: 1px solid var(--line); max-width: 300px; flex: none; }
  .vid > span { width: 72px; aspect-ratio: 16 / 9; border-radius: 7px; background: linear-gradient(135deg, #1a2433, #0b0f16); flex: none; }
  .vid div { min-width: 0; }
  .vid b { font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .vid small { color: var(--text-3); font-size: 11.5px; }
  .steps { display: flex; gap: 14px; flex: none; }
  .step { display: flex; align-items: center; gap: 8px; font-size: 12.5px; color: var(--text-2); }
  .step i { width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; font-size: 11px; font-weight: 900; font-style: normal; flex: none; }
  .step.no i { background: rgba(255, 105, 97, 0.2); color: var(--red); }
  .step.now { color: var(--text); }
  .step.now i { border: 2.5px solid rgba(10, 132, 255, 0.25); border-top-color: var(--accent); animation: r 0.9s linear infinite; }
  .time { font: 600 12.5px var(--mono, monospace); color: var(--text); flex: none; font-variant-numeric: tabular-nums; }
  .ppl { display: flex; gap: 6px; flex-wrap: wrap; justify-content: flex-end; max-width: 45%; }
  .pp {
    display: inline-flex; align-items: center; gap: 6px; padding: 3px 9px 3px 3px; border-radius: 99px;
    background: rgba(0, 0, 0, 0.3); border: 1px solid var(--line-2); font-size: 12px; font-weight: 600; white-space: nowrap;
  }
  .pp i { width: 20px; height: 20px; border-radius: 50%; display: grid; place-items: center; font-size: 10px; font-weight: 800; font-style: normal; color: #fff; background: var(--accent); }
  .pp u { text-decoration: none; font-size: 11px; color: var(--green); }
  .pp u.o { color: var(--orange, #ff9f0a); }

  .frame { position: relative; flex: 1; min-height: 0; overflow: hidden; }
  .loading { position: absolute; inset: 0; display: grid; place-items: center; color: var(--text-3); }
  .home { position: absolute; inset: 0; overflow-y: auto; padding: 18px 34px 30px; display: flex; flex-direction: column; gap: 22px; }
  .home h1 { margin: 0; font-size: 30px; font-weight: 900; letter-spacing: -0.035em; }
  .home p { margin: 4px 0 0; color: var(--text-2); font-size: 14px; max-width: 640px; }
  .k { color: var(--text-2); font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em; }
  .tiles { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 10px; margin-top: 10px; }
  .tile {
    height: 86px; border-radius: 16px; border: 1px solid var(--line); background: var(--glass, var(--raise)); text-align: left;
    display: flex; flex-direction: column; justify-content: space-between; align-items: flex-start; padding: 12px 14px;
  }
  .tile:hover { border-color: var(--line-2); }
  .tile.accent { border-color: color-mix(in srgb, var(--accent) 50%, transparent); }
  .tile b { font-size: 14px; display: block; }
  .tile small { color: var(--text-3); font-size: 11.5px; }
  .tile .dot { width: 26px; height: 26px; border-radius: 8px; display: grid; place-items: center; font-weight: 900; font-size: 12px; color: #fff; }
  .tile .dot.spark { background: var(--accent); }
  .how { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
  .how div { border-radius: 16px; padding: 14px 16px; border: 1px solid var(--line); background: var(--glass, var(--raise)); display: flex; gap: 12px; }
  .how em { font-style: normal; width: 26px; height: 26px; border-radius: 50%; background: rgba(48, 209, 88, 0.16); color: var(--green); display: grid; place-items: center; font-weight: 900; flex: none; }
  .how b { display: block; font-size: 13.5px; }
  .how small { color: var(--text-2); font-size: 12px; }
  .recent { display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; margin-top: 10px; }
  .rc { padding: 0; border-radius: 14px; overflow: hidden; border: 1px solid var(--line); background: var(--glass, var(--raise)); text-align: left; display: block; min-width: 0; }
  .rc .th { display: grid; place-items: center; aspect-ratio: 16 / 9; background: linear-gradient(135deg, #263445, #10151d); color: var(--text-3); }
  .rc .tx { display: block; padding: 8px 10px; }
  .rc b { font-size: 12.5px; display: block; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .rc small { color: var(--text-3); font-size: 11px; }
</style>
