<script lang="ts">
  import { t, locale } from "./i18n.svelte";
  // Lounge (docs/design/media.md §6.3): ortak kuyruk + "sıradaki" oylaması. Sözler
  // müzik sahnesinin ortasında (MusicView). Parça bitince Santral kuyruğun başını herkese açar (PROTOCOL §3.11).
  import { fun } from "./fun.svelte";
  import { flip } from "svelte/animate";
  import { fade, fly } from "svelte/transition";
  import { queueAdd, syncFun, mediaList } from "./player";
  import { createList, addTracks, playList } from "./playlists.svelte";
  import { artFor, splitTitle } from "./music";
  import Icon from "./Icon.svelte";

  let { canControl, me, title, src, cover, onError, onLog }: {
    canControl: boolean;
    me: string | null;
    /** Odada çalan medyanın başlığı (yoksa boş). */
    title: string;
    /** Odada çalan medyanın adresi: "sıradaki" ile gider, çift basış tek atlama sayılır. */
    src: string;
    cover: string | null;
    onError: (msg: string) => void;
    onLog: (msg: string) => void;
  } = $props();

  let link = $state("");
  let adding = $state(false);
  let busyText = $state("");

  /** Spotify linki ya da YouTube çalma listesi: şarkılar tek tek çözülür. */
  const spotify = (s: string) => /open\.spotify\.com\/|^spotify:/i.test(s);
  const ytList = (s: string) => /youtube\.com\/.*[?&]list=|youtube\.com\/playlist/i.test(s);

  async function add() {
    const s = link.trim();
    if (!s || adding) return;
    adding = true;
    try {
      if (spotify(s) || ytList(s)) {
        busyText = spotify(s) ? t("Spotify şarkıları YouTube'da eşleniyor…") : t("Liste okunuyor…");
        const hits = (await mediaList(s)).filter((h) => !h.live);
        if (!hits.length) throw new Error(t("Bu linkte şarkı bulunamadı"));
        link = "";
        if (hits.length === 1) {
          onLog(t("Kuyruğa eklendi: ") + (await queueAdd(hits[0].url, hits[0].title)));
        } else {
          // Liste Listelerim'e kaydedilir ve Lounge'a sırayla aktarılır (App: liste besleme).
          const id = createList((spotify(s) ? "Spotify" : "YouTube") + " · " + new Date().toLocaleDateString(locale()));
          addTracks(id, hits.map((h) => ({ src: h.url, title: h.title, dur: h.durationSecs ?? null })));
          playList(id);
          onLog(t("{n} şarkı Listelerim'e kaydedildi, Lounge'a sırayla ekleniyor", { n: hits.length }));
        }
        return;
      }
      const title2 = await queueAdd(s);
      link = "";
      onLog(t("Kuyruğa eklendi: ") + title2);
    } catch (e) {
      onError(String(e));
    } finally {
      adding = false;
      busyText = "";
    }
  }
  function send(msg: Record<string, unknown>) {
    syncFun(msg).catch((e) => onError(String(e)));
  }

</script>

<div class="add">
  <Icon name="plus" size={15} />
  <input class="q" placeholder={t("Şarkı, Spotify ya da YouTube listesi linki")} bind:value={link} onkeydown={(e) => e.key === "Enter" && add()} />
  {#if link.trim()}<button class="go" disabled={adding} onclick={add}>{adding ? "…" : t("Ekle")}</button>{/if}
</div>
<div class="hint">{busyText || t("YouTube, SoundCloud ya da Spotify (şarkı, albüm, liste) linki.")}</div>

<div class="eyebrow gap">{t("Şimdi çalıyor")}</div>
{#if title}
  {@const m = splitTitle(title)}
  <div class="item now">
    <span class="thumb">{#if cover}<img src={cover} alt="" />{:else}<Icon name="music" size={16} />{/if}</span>
    <span class="t"><b>{m.song}</b><small>{m.artist || "Lounge"}</small></span>
    {#if canControl && fun.queue.length}
      <button class="ghost" title={t("Sıradakine geç")} onclick={() => send({ t: "queue_next", src })}><Icon name="next" size={15} /></button>
    {/if}
  </div>
{:else}
  <div class="hint">{t("Bir şey çalmıyor.")}</div>
{/if}

<div class="eyebrow gap">{t("Sıradakiler")}{fun.queue.length ? ` · ${fun.queue.length}` : ""}</div>
{#if fun.queue.length}
  <ol class="queue">
    {#each fun.queue as q, i (q.id)}
      {@const voted = me != null && q.voters.includes(me)}
      {@const m = splitTitle(q.media.title)}
      {@const art = artFor(q.media.src)}
      <li class="item" animate:flip={{ duration: 300 }} in:fly={{ x: 16, duration: 280 }} out:fade={{ duration: 160 }}>
        <span class="n">{i + 1}</span>
        <span class="thumb">{#if art}<img src={art} alt="" loading="lazy" />{:else}<Icon name="music" size={14} />{/if}</span>
        <span class="t" title={q.media.src}><b>{m.song}</b><small>{m.artist ? m.artist + " · " : ""}{q.byName}</small></span>
        <button class="vote" class:on={voted} title={voted ? t("Oyunu geri al") : t("Bunu sıradaki yap")}
          onclick={() => send({ t: "queue_vote", queueId: q.id })}>▲ {q.voters.length}</button>
        {#if q.by === me || canControl}
          <button class="ghost x" title={t("Çıkar")} onclick={() => send({ t: "queue_remove", queueId: q.id })}><Icon name="x" size={13} /></button>
        {/if}
      </li>
    {/each}
  </ol>
{:else}
  <div class="hint">{t("Kuyruk boş. Şarkı ekle; çalan bitince sıradaki herkeste başlar, en çok oy alan öne geçer.")}</div>
{/if}

<style>
  .add {
    display: flex; align-items: center; gap: 8px; height: 40px; padding: 0 6px 0 12px; border-radius: 99px;
    background: rgba(255, 255, 255, 0.08); color: var(--text-2); border: 1px solid transparent; transition: border-color 0.2s, background 0.2s;
  }
  .add:focus-within { border-color: rgba(255, 255, 255, 0.35); background: rgba(255, 255, 255, 0.1); }
  .add input.q { flex: 1; min-width: 0; height: 100%; background: transparent; border: 0; box-shadow: none; padding: 0; outline: none; }
  .go { height: 28px; padding: 0 14px; border-radius: 99px; border: 0; background: #1ed760; color: #000; font-weight: 700; }
  .gap { margin-top: 18px; }
  .hint { color: var(--text-2); font-size: 12px; }
  .queue { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .item { display: flex; align-items: center; gap: 10px; padding: 6px 8px; border-radius: 8px; transition: background 0.15s; }
  .item:hover { background: rgba(255, 255, 255, 0.07); }
  .item.now .t b { color: #1ed760; }
  .n { width: 14px; text-align: right; font-size: 12px; color: var(--text-3); font-variant-numeric: tabular-nums; flex: none; }
  .thumb {
    flex: none; width: 40px; height: 40px; border-radius: 4px; overflow: hidden; display: grid; place-items: center;
    background: linear-gradient(135deg, #2a2a30, #151518); color: var(--text-3);
  }
  .thumb img { width: 100%; height: 100%; object-fit: cover; transform: scale(1.34); }
  .t { flex: 1; min-width: 0; display: flex; flex-direction: column; overflow: hidden; }
  .t b, .t small { text-overflow: ellipsis; white-space: nowrap; overflow: hidden; }
  .t b { font-weight: 600; font-size: 13px; }
  .t small { color: var(--text-3); font-size: 11.5px; }
  .vote { padding: 3px 8px; font-size: 11.5px; font-variant-numeric: tabular-nums; border-radius: 99px; background: transparent; }
  .vote.on { color: #1ed760; border-color: #1ed760; }
  .ghost { width: 28px; height: 28px; padding: 0; display: grid; place-items: center; border: 0; background: transparent; color: var(--text-2); border-radius: 50%; }
  .ghost:hover { color: #fff; background: rgba(255, 255, 255, 0.1); }
  .item .x { opacity: 0; }
  .item:hover .x { opacity: 1; }
</style>
