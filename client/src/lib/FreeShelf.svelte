<script lang="ts">
  // "Ücretsiz & klasik" rafı (Keşfet ana sayfası ve Sinema lobisi): kamu malı filmler tek tıkla
  // oynatıcıda açılır (odadaysa herkeste). `onChannel` varsa resmi ücretsiz kanallar da görünür.
  import { freeFilms, freeFilmUrl, FREE_CHANNELS, ARCHIVE_MORE, type FreeFilm } from "./freeshelf";
  import Icon from "./Icon.svelte";
  import { t, tx } from "./i18n.svelte";

  let { inRoom, busy = false, featured = false, onPlay, onChannel = null }: {
    inRoom: boolean;
    busy?: boolean;
    /** Sağlayıcı kurulu değil: raf öne çıkar, açıklaması uzun. */
    featured?: boolean;
    /** Doğrudan MP4 adresini oynatıcıda aç. Hata fırlatabilir. */
    onPlay: (url: string) => Promise<unknown> | void;
    /** Sayfayı Keşfet'te aç (kanallar ve "Daha fazla"); yoksa gösterilmez. */
    onChannel?: ((url: string) => void) | null;
  } = $props();

  let films = $state<FreeFilm[]>([]);
  let failed = $state(false);
  let opening = $state<string | null>(null);
  let err = $state("");

  function load() {
    failed = false;
    freeFilms().then((l) => (films = l)).catch(() => (failed = true));
  }
  load();

  async function play(f: FreeFilm) {
    if (opening || busy) return;
    opening = f.id;
    err = "";
    try {
      await onPlay(await freeFilmUrl(f.id));
    } catch (e) {
      err = tx(String(e).replace(/^Error:\s*/, ""));
    } finally {
      opening = null;
    }
  }
  const meta = (f: FreeFilm) => [f.year, f.mins ? t("{m} dk", { m: f.mins }) : null].filter(Boolean).join(" · ");
</script>

<section class="shelf" class:featured>
  <div class="hd">
    <h4>{t("Ücretsiz & klasik")}</h4>
    <span>{featured ? t("Hesap ya da kurulum gerekmez: kamu malı klasikler, tek tıkla oynatıcıda.") : t("Kamu malı filmler · Internet Archive")}{inRoom ? " " + t("Odadaki herkeste açılır.") : ""}</span>
    {#if onChannel}<button type="button" class="more" onclick={() => onChannel(ARCHIVE_MORE)}>{t("Daha fazla")} <Icon name="fwd" size={12} /></button>{/if}
  </div>
  {#if err}<div class="bad">{err}</div>{/if}
  <div class="row">
    {#each films as f (f.id)}
      <button type="button" class="card" disabled={busy || (!!opening && opening !== f.id)} title={f.title} onclick={() => play(f)}>
        <span class="th">
          <img src={f.thumb} alt="" loading="lazy" />
          <span class="pl">{#if opening === f.id}<span class="sp"></span>{:else}<Icon name="play" size={14} />{/if}</span>
        </span>
        <b>{f.title}</b>
        <small>{meta(f)}</small>
      </button>
    {:else}
      <div class="none">
        {#if failed}{t("Liste yüklenemedi.")} <button type="button" class="link" onclick={load}>{t("Tekrar dene")}</button>{:else}{t("Yükleniyor…")}{/if}
      </div>
    {/each}
    {#if onChannel}
      {#each FREE_CHANNELS as c (c.url)}
        <button type="button" class="card ch" onclick={() => onChannel(c.url)}>
          <span class="th"><Icon name="film" size={22} /></span>
          <b lang="en">{c.name}</b>
          <small>{t("Ücretsiz resmi kanal")}</small>
        </button>
      {/each}
    {/if}
  </div>
</section>

<style>
  .shelf { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .hd { display: flex; align-items: baseline; gap: 10px; flex-wrap: wrap; }
  h4 { margin: 0; font-size: 11.5px; color: var(--text-2); text-transform: uppercase; letter-spacing: 0.06em; }
  .featured h4 { font-size: 15px; color: var(--text); text-transform: none; letter-spacing: -0.01em; font-weight: 800; }
  .hd span { color: var(--text-3); font-size: 12px; }
  .more { all: unset; cursor: pointer; margin-left: auto; color: var(--text-2); font-size: 12px; display: inline-flex; align-items: center; gap: 2px; }
  .more:hover { color: var(--text); }
  .row { display: flex; gap: 12px; overflow-x: auto; padding-bottom: 6px; scrollbar-width: thin; }
  .card {
    all: unset; cursor: pointer; flex: none; width: 150px; display: flex; flex-direction: column; gap: 3px; min-width: 0;
  }
  .card:disabled { cursor: default; opacity: 0.55; }
  .th {
    position: relative; width: 150px; height: 96px; border-radius: 10px; overflow: hidden; display: grid; place-items: center;
    background: rgba(255, 255, 255, 0.06); border: 1px solid var(--line); color: var(--text-3); margin-bottom: 4px;
  }
  .th img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .pl {
    position: absolute; right: 8px; bottom: 8px; width: 28px; height: 28px; border-radius: 50%; display: grid; place-items: center;
    background: rgba(0, 0, 0, 0.6); color: #fff; opacity: 0; transition: opacity 0.15s;
  }
  .card:hover:not(:disabled) .pl, .card:focus-visible .pl, .pl:has(.sp) { opacity: 1; }
  .card:hover:not(:disabled) .th { border-color: var(--line-2); }
  .card b { font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .card small { color: var(--text-3); font-size: 11.5px; min-height: 1em; }
  .ch .th { background: linear-gradient(140deg, rgba(10, 132, 255, 0.18), rgba(94, 92, 230, 0.12)); }
  .none { padding: 14px; border-radius: 12px; border: 1px solid var(--line); color: var(--text-3); font-size: 12.5px; flex: 1; }
  .link { all: unset; cursor: pointer; color: var(--accent); }
  .bad { color: #ff8a80; font-size: 12px; }
  .sp { width: 12px; height: 12px; border-radius: 50%; border: 2px solid rgba(255, 255, 255, 0.3); border-top-color: #fff; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
