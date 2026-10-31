<script lang="ts">
  // Boş Sinema (lobi): tek kutu (link/dosya yolu açar, düz yazı Film & Dizi'de arar),
  // Ne izlesek? / Film & Dizi / Herkes indirsin kartları ve "Kaldığınız yerden".
  // mpv bu sırada gizli; sahneyi arayüz çizer (kokpit modeli bozulmaz).
  import { loadResume, removeResume, type ResumeEntry } from "./resume";
  import Icon from "./Icon.svelte";
  import { t, tx, locale } from "./i18n.svelte";

  let { value = $bindable(""), people, room, busy, error, hue, onOpen, onTitles, onNight, onDownload, onResume }: {
    value: string;
    /** Odadakiler (tek başınaysa boş). */
    people: { id: string; name: string }[];
    /** Oda adı; tek başınaysa null. */
    room: string | null;
    busy: boolean;
    /** Son açılamayan medya. */
    error: string | null;
    hue: (name: string) => number;
    onOpen: (text: string) => void;
    onTitles: (query: string) => void;
    onNight: (() => void) | null;
    onDownload: ((url: string) => void) | null;
    onResume: (r: ResumeEntry) => void;
  } = $props();

  let input = $state<HTMLInputElement>();
  let entries = $state(loadResume());
  /** Odadayken önce bu odanın kayıtları; yoksa tümü. */
  const list = $derived.by(() => {
    const here = room ? entries.filter((r) => r.room === room) : [];
    return (here.length ? here : entries).slice(0, 3);
  });
  const onlyHere = $derived(!!room && entries.some((r) => r.room === room));

  const isUrl = (s: string) => /^(https?|magnet):/i.test(s);
  /** Link, dosya yolu ya da bilinen uzantı: doğrudan açılır; gerisi film/dizi araması. */
  const direct = (s: string) => isUrl(s) || /^([a-z]:[\\/]|[\\/~]|\\\\)/i.test(s) || /\.(mkv|mp4|avi|mov|webm|m3u8|ts|srt|ass|vtt)$/i.test(s);
  const text = $derived(value.trim().replace(/^"(.*)"$/, "$1"));
  const isLink = $derived(/^https?:/i.test(text));
  function go() {
    if (!text || busy) return;
    if (direct(text)) onOpen(text);
    else onTitles(text);
  }
  function download() {
    if (!onDownload) return;
    if (isLink) return onDownload(text);
    input?.focus();
  }

  function ago(at: number): string {
    const d = Math.floor((Date.now() - at) / 86400_000);
    if (d <= 0) return t("bugün");
    if (d === 1) return t("dün");
    if (d < 7) return t("{n} gün önce", { n: d });
    return d < 14 ? t("geçen hafta") : t("{n} hafta önce", { n: Math.floor(d / 7) });
  }
  function left(r: ResumeEntry): string {
    const m = Math.max(1, Math.round((r.duration - r.pos) / 60));
    return m >= 60 ? t("{h} sa {m} dk kaldı", { h: Math.floor(m / 60), m: m % 60 }) : t("{m} dk kaldı", { m });
  }
  function forget(r: ResumeEntry) {
    entries = removeResume(r.src);
  }
  const hour = new Date().getHours();
  const whenTitle = $derived(t(hour < 5 ? "Bu gece ne izliyoruz?" : hour < 12 ? "Bu sabah ne izliyoruz?" : hour < 18 ? "Bugün ne izliyoruz?" : "Bu akşam ne izliyoruz?"));

  $effect(() => { input?.focus(); });
</script>

<div class="lobby">
  <div class="lb-h">
    <div>
      <h1>{whenTitle}</h1>
      <p>{t("Link yapıştır, dosya bırak ya da bir film ara.")}{room ? " " + t("Açtığın şey herkeste aynı anda başlar.") : ""}</p>
    </div>
    {#if people.length}
      <div class="ready">
        <span class="avs">{#each people.slice(0, 5) as m (m.id)}<span class="av" style="--h:{hue(m.name)}" title={m.name}>{m.name.slice(0, 1).toLocaleUpperCase(locale())}</span>{/each}</span>
        {people.length === 1 ? t("yalnızsın") : t("{n} kişi burada", { n: people.length })}
      </div>
    {/if}
  </div>

  {#if error}
    <div class="err">
      <Icon name="x" size={14} />
      <span style="flex:1">{error.startsWith("Son video") ? tx(error) : t("Son video açılamadı: {err}", { err: tx(error) })}</span>
      <button type="button" class="err-x" onclick={() => (error = null)} title={t("Kapat")}>✕</button>
    </div>
  {/if}

  <form class="omni" onsubmit={(e) => { e.preventDefault(); go(); }}>
    <Icon name="search" size={20} />
    <input class="bare" bind:this={input} bind:value placeholder={t("Film adı, YouTube linki ya da dosya yolu…")} spellcheck="false" />
    {#if !text}<span class="keys"><kbd>Ctrl</kbd><kbd>V</kbd> {t("yapıştır")}</span>{/if}
    <button class="go" type="submit" disabled={!text || busy}>{busy ? t("Açılıyor…") : !text || direct(text) ? t("Aç") : t("Ara")}</button>
  </form>

  <div class="lb-grid">
    <div class="srcs" class:few={!onNight}>
      {#if onNight}
        <button class="src night" onclick={onNight}>
          <span class="fan" aria-hidden="true"><i></i><i></i><i></i></span>
          <h3>{t("Karar veremediniz mi?")}</h3>
          <p>{t("Ne izlesek? herkesin önerisini toplar, oylatır, eşleştirir.")}</p>
          <span class="cta"><Icon name="spark" size={14} /> {t("Ne izlesek? başlat")}</span>
        </button>
      {/if}
      <button class="src" class:wide={!onDownload} onclick={() => onTitles(direct(text) ? "" : text)}>
        <span class="ico green"><Icon name="film" size={20} /></span>
        <h3>{t("Film & Dizi")}</h3><p>{room ? t("Film ve dizi ara, herkes kendi hesabıyla açar") : t("Film ve dizi ara")}</p>
      </button>
      {#if onDownload}
        <button class="src" onclick={download} title={isLink ? t("Bu linki herkes indirsin") : t("Önce kutuya bir link yapıştır")}>
          <span class="ico orange"><Icon name="down" size={20} /></span>
          <h3>{t("Herkes indirsin")}</h3><p>{isLink ? t("Kutudaki linki herkes kendine indirir") : t("Herkes kendine indirir, hazır olunca birlikte başlar")}</p>
        </button>
      {/if}
    </div>
    <div class="cont">
      <h4>{t("Kaldığınız yerden")} {#if list.length}<span>{onlyHere ? t("bu oda") : t("bu bilgisayar")}</span>{/if}</h4>
      {#each list as r (r.src)}
        <div class="ci">
          <div class="th" style="--p:{(r.pos / r.duration) * 100}%;--h:{hue(r.title)}"><span>{r.title.slice(0, 1)}</span></div>
          <span class="ct"><b title={r.title}>{r.title}</b><small>{ago(r.at)} · {left(r)}</small></span>
          <button class="x" title={t("Listeden çıkar")} onclick={() => forget(r)}><Icon name="x" size={12} /></button>
          <button class="resume" disabled={busy} onclick={() => onResume(r)}><Icon name="play" size={12} /> {t("Devam")}</button>
        </div>
      {:else}
        <div class="none">{t("Yarım kalan film yok. İzlediğin her şeyin yeri burada tutulur.")}</div>
      {/each}
      <div class="drop">
        <Icon name="file" size={20} />
        <span><b>{t("Dosyayı pencereye bırak")}</b><br />{room ? t("Herkes kendi kopyasını açar; altyazıyı da bırakabilirsin") : t("Altyazı dosyasını da bırakabilirsin")}</span>
      </div>
    </div>
  </div>
</div>

<style>
  .lobby {
    position: absolute; inset: 0; display: flex; flex-direction: column; gap: 22px; padding: 34px 44px; overflow-y: auto;
    background: radial-gradient(70% 60% at 20% 0%, rgba(10, 132, 255, 0.16), transparent 70%), radial-gradient(50% 50% at 90% 100%, rgba(94, 92, 230, 0.12), transparent 70%), #08090b;
  }
  .lb-h { display: flex; align-items: flex-end; gap: 16px; flex-wrap: wrap; }
  h1 { margin: 0; font-size: 40px; font-weight: 900; letter-spacing: -0.035em; line-height: 1.05; }
  .lb-h p { margin: 6px 0 0; color: var(--text-2); font-size: 14px; }
  .ready { margin-left: auto; display: flex; align-items: center; gap: 8px; color: var(--text-2); font-size: 12.5px; }
  .avs { display: flex; padding-left: 8px; }
  .av {
    width: 28px; height: 28px; margin-left: -8px; border-radius: 50%; display: grid; place-items: center; font-weight: 700; font-size: 11.5px; color: #fff;
    background: hsl(var(--h) 55% 45%); border: 2px solid #0b0c0e;
  }
  .err { padding: 10px 14px; border-radius: 12px; background: rgba(255, 69, 58, 0.1); border: 1px solid rgba(255, 69, 58, 0.35); color: #ff8a80; font-size: 12.5px; display: flex; gap: 8px; align-items: center; }
  .err-x { all: unset; cursor: pointer; color: #ff8a80; padding: 0 4px; font-size: 14px; opacity: 0.75; }
  .err-x:hover { opacity: 1; }

  .omni {
    flex: none; height: 62px; border-radius: 18px; background: rgba(255, 255, 255, 0.06); border: 1px solid rgba(255, 255, 255, 0.18);
    display: flex; align-items: center; gap: 14px; padding: 0 10px 0 20px; color: var(--text-2);
    box-shadow: 0 20px 50px -20px rgba(10, 132, 255, 0.5); transition: border-color 0.2s, box-shadow 0.2s;
  }
  .omni:focus-within { border-color: rgba(10, 132, 255, 0.7); box-shadow: 0 20px 50px -16px rgba(10, 132, 255, 0.65); }
  .omni input.bare, .omni input.bare:focus-visible { flex: 1; min-width: 0; height: 100%; padding: 0; border: 0; background: none; box-shadow: none; outline: none; border-radius: 0; font-size: 15px; color: var(--text); }
  .keys { display: flex; gap: 6px; align-items: center; font-size: 12px; white-space: nowrap; }
  kbd { font: 500 11px var(--font-mono); padding: 2px 6px; border-radius: 6px; border: 1px solid var(--line-2); }
  .go { height: 44px; padding: 0 22px; border-radius: 13px; border: 0; background: #fff; color: #000; font-weight: 800; }
  .go:hover:not(:disabled) { background: #e8e8ed; }
  .go:disabled { opacity: 0.4; }

  .lb-grid { display: grid; grid-template-columns: 1.15fr 1fr; gap: 22px; flex: 1; min-height: 300px; }
  .srcs { display: grid; grid-template-columns: 1fr 1fr; grid-auto-rows: 1fr; gap: 12px; }
  .srcs.few { grid-auto-rows: 170px; align-content: start; }
  .src {
    position: relative; border-radius: 18px; padding: 18px; border: 1px solid var(--line); background: rgba(255, 255, 255, 0.045);
    display: flex; flex-direction: column; justify-content: flex-end; align-items: flex-start; gap: 4px; overflow: hidden; text-align: left; height: auto; min-height: 130px;
    transition: transform 0.2s, border-color 0.2s, background 0.2s;
  }
  .src:hover { transform: translateY(-2px); border-color: var(--line-2); background: rgba(255, 255, 255, 0.07); }
  .src.wide, .src.night { grid-column: span 2; }
  .src h3 { margin: 0; font-size: 17px; font-weight: 800; letter-spacing: -0.02em; }
  .src p { margin: 0; color: var(--text-2); font-size: 12.5px; }
  .ico { position: absolute; top: 16px; left: 18px; width: 40px; height: 40px; border-radius: 12px; display: grid; place-items: center; }
  .ico.green { background: rgba(48, 209, 88, 0.16); color: var(--green); }
  .ico.orange { background: rgba(255, 159, 10, 0.16); color: #ff9f0a; }
  .src.night { background: linear-gradient(120deg, rgba(10, 132, 255, 0.28), rgba(94, 92, 230, 0.22) 50%, rgba(191, 90, 242, 0.18)); border-color: rgba(10, 132, 255, 0.4); }
  .src.night:hover { border-color: rgba(10, 132, 255, 0.7); }
  .fan { position: absolute; right: 22px; top: 14px; display: flex; }
  .fan i { width: 62px; height: 92px; border-radius: 8px; margin-left: -18px; box-shadow: 0 14px 30px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.1); transition: transform 0.3s; }
  .fan i:nth-child(1) { background: linear-gradient(165deg, #1b2a3a, #0b0f14); transform: rotate(-10deg) translateY(6px); }
  .fan i:nth-child(2) { background: linear-gradient(165deg, #c46a24, #0d0605); transform: rotate(-2deg); z-index: 1; }
  .fan i:nth-child(3) { background: linear-gradient(165deg, #2c3a24, #0b0e08); transform: rotate(8deg) translateY(6px); }
  .src.night:hover .fan i:nth-child(1) { transform: rotate(-16deg) translateY(4px) translateX(-6px); }
  .src.night:hover .fan i:nth-child(3) { transform: rotate(14deg) translateY(4px) translateX(6px); }
  .cta { margin-top: 8px; height: 34px; padding: 0 14px; border-radius: 10px; background: #fff; color: #000; font-weight: 750; font-size: 12.5px; display: inline-flex; align-items: center; gap: 6px; }

  .cont { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  h4 { margin: 0; font-size: 11.5px; color: var(--text-2); text-transform: uppercase; letter-spacing: 0.06em; display: flex; justify-content: space-between; }
  h4 span { text-transform: none; letter-spacing: 0; color: var(--text-3); font-weight: 500; }
  .ci { display: flex; align-items: center; gap: 12px; padding: 10px; border-radius: 14px; background: rgba(255, 255, 255, 0.045); border: 1px solid var(--line); }
  .th {
    position: relative; width: 92px; height: 52px; border-radius: 8px; flex: none; overflow: hidden; display: grid; place-items: center;
    background: radial-gradient(50% 60% at 40% 40%, hsl(var(--h) 45% 30%), transparent 75%), hsl(var(--h) 30% 7%);
    font-weight: 900; font-size: 20px; color: rgba(255, 255, 255, 0.5);
  }
  .th::after { content: ""; position: absolute; left: 0; bottom: 0; height: 3px; width: var(--p); background: #ff375f; }
  .ct { display: flex; flex-direction: column; min-width: 0; flex: 1; line-height: 1.25; }
  .ct b { font-size: 13.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ct small { color: var(--text-2); font-size: 11.5px; }
  .x { width: 24px; height: 24px; padding: 0; border-radius: 7px; display: grid; place-items: center; color: var(--text-3); background: transparent; border: 0; opacity: 0; transition: opacity 0.15s; }
  .ci:hover .x { opacity: 1; }
  .resume { height: 32px; padding: 0 12px; border-radius: 10px; font-weight: 650; font-size: 12px; white-space: nowrap; display: inline-flex; gap: 5px; align-items: center; }
  .none { padding: 16px; border-radius: 14px; border: 1px solid var(--line); color: var(--text-3); font-size: 12.5px; }
  .drop { border: 1.5px dashed rgba(255, 255, 255, 0.16); border-radius: 14px; padding: 14px; display: flex; align-items: center; gap: 12px; color: var(--text-2); font-size: 12.5px; margin-top: auto; }
  .drop b { color: #fff; }
  @media (max-width: 900px) {
    .lb-grid { grid-template-columns: 1fr; }
    h1 { font-size: 32px; }
  }
</style>
