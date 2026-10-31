<script lang="ts">
  import { t } from "./i18n.svelte";
  // Çalma listeleri (Spotify benzeri): oluştur, ara/link yapıştır ya da çalanı ekle,
  // sırala, karıştırıp çal. Tek başına modda sırayla yerelde çalar; odada Lounge'a gider.
  import Icon from "./Icon.svelte";
  import { mediaList, webSearch, type SearchHit } from "./player";
  import {
    lists, playing, findList, createList, renameList, deleteList, addTracks, removeTrack, moveTrack,
    playList, stopList, stepList, isLiked, toggleLike, nowTrack, fmtDur, type Track,
  } from "./playlists.svelte";

  let { inRoom, current, onError, onLog, onNext, onPrev, onStop }: {
    inRoom: boolean;
    /** Şu an çalan (link ise); kalp ve "Çalanı ekle" için. */
    current: Track | null;
    onError: (msg: string) => void;
    onLog: (msg: string) => void;
    onNext?: () => void;
    onPrev?: () => void;
    /** "Listeyi durdur" (odada kuyruğa eklenmiş liste şarkılarını da çıkarır). */
    onStop?: () => void;
  } = $props();

  /** Kalple eklenen listenin sabit adı ve varsayılan ad çevrilerek gösterilir. */
  const dn = (n: string) => t(n);
  let openId = $state("");
  const list = $derived(openId ? findList(openId) : undefined);
  let newName = $state("");
  let renaming = $state(false);
  let nameDraft = $state("");
  let confirmDelete = $state(false);
  let query = $state("");
  let busy = $state(false);
  let hits = $state<SearchHit[]>([]);

  /** "N şarkı · toplam süre"; süresi bilinmeyen liste için yalnızca sayı. */
  function summary(ts: Track[]) {
    const s = ts.reduce((n, t) => n + (t.dur ?? 0), 0);
    return t("{n} şarkı", { n: ts.length }) + (s ? " · " + fmtDur(s) : "");
  }
  const toTrack = (h: SearchHit): Track => ({ src: h.url, title: h.title, dur: h.durationSecs ?? null });

  function create() {
    openId = createList(newName);
    newName = "";
  }
  function show(id: string) {
    openId = id;
    renaming = confirmDelete = false;
    hits = [];
    query = "";
  }
  function play(id: string, from = 0, shuffle = false) {
    playList(id, from, shuffle);
    onLog((shuffle ? t("Karıştırılarak çalınıyor: ") : t("Çalınıyor: ")) + dn(findList(id)?.name ?? ""));
  }

  /** Link: listedeki/tek videodaki şarkılar eklenir. Metin: YouTube'da aranır. */
  async function find() {
    const q = query.trim();
    if (!q || busy || !list) return;
    busy = true;
    hits = [];
    try {
      if (/^https?:\/\//i.test(q)) {
        const got = (await mediaList(q)).filter((h) => !h.live);
        const n = addTracks(list.id, got.map(toTrack));
        onLog(n ? t("{name}: {n} şarkı eklendi", { name: dn(list.name), n }) : t("Yeni şarkı yok (hepsi listede)"));
        query = "";
      } else {
        hits = (await webSearch(q)).filter((h) => !h.live);
        if (!hits.length) onLog(t("Sonuç yok: ") + q);
      }
    } catch (e) {
      onError(String(e));
    } finally {
      busy = false;
    }
  }

  // Odada liste Lounge'a azar azar eklenir (pos ilerlemez): çalan, listede çalan şarkıdır.
  const roomPos = $derived(inRoom && current ? playing.queue.findIndex((t) => t.src === current.src) : -1);
  const now = $derived(inRoom ? (roomPos >= 0 ? playing.queue[roomPos] : null) : nowTrack());
  const liked = $derived(current ? isLiked(current.src) : false);
</script>

{#if current}
  <div class="now">
    <span class="t" title={current.src}>{current.title}</span>
    <button class="icon like" class:on={liked} title={liked ? t("Beğendiklerim'den çıkar") : t("Beğendiklerim'e ekle")}
      onclick={() => current && toggleLike(current)}><Icon name="heart" size={15} /></button>
  </div>
{/if}

{#if playing.id}
  <div class="playing">
    <Icon name="music" size={14} />
    <span class="t">
      <b>{now?.title || dn(playing.name)}</b>
      <small>{dn(playing.name)}{inRoom ? (roomPos >= 0 ? ` · ${roomPos + 1}/${playing.queue.length}` : ` · ${t("{n} şarkı", { n: playing.queue.length })}`) : ` · ${playing.pos + 1}/${playing.queue.length}`}</small>
    </span>
    <button class="icon" title={t("Önceki")} onclick={() => (onPrev ? onPrev() : stepList(-1))}><Icon name="prev" size={13} /></button>
    <button class="icon" title={t("Sonraki")} onclick={() => (onNext ? onNext() : stepList(1))}><Icon name="next" size={13} /></button>
    <button class="icon" title={t("Listeyi durdur")} onclick={() => (onStop ? onStop() : stopList())}><Icon name="stop" size={13} /></button>
  </div>
{/if}

{#if !list}
  <div class="eyebrow">{t("Çalma listelerin")}</div>
  <div class="row">
    <input placeholder={t("Yeni liste adı")} maxlength="80" bind:value={newName} onkeydown={(e) => e.key === "Enter" && create()} />
    <button class="white" onclick={create}><Icon name="plus" size={14} />{t("Oluştur")}</button>
  </div>
  {#if lists.all.length}
    <ul class="lists">
      {#each lists.all as p (p.id)}
        <li class:on={playing.id === p.id}>
          <button class="open" onclick={() => show(p.id)}>
            <span class="cover"><Icon name={p.id === "liked" ? "heart" : "list"} size={16} /></span>
            <span class="t"><b>{dn(p.name)}</b><small>{summary(p.tracks)}</small></span>
          </button>
          <button class="icon" title={t("Çal")} disabled={!p.tracks.length} onclick={() => play(p.id)}><Icon name="play" size={13} /></button>
        </li>
      {/each}
    </ul>
  {:else}
    <div class="hint">{t("Henüz liste yok. Bir liste oluştur; içine şarkı ara, YouTube çalma listesi linki yapıştır ya da çalan şarkıyı kalple beğen.")}</div>
  {/if}
  <div class="hint gap">{inRoom
    ? t("Çaldığın liste odanın Lounge kuyruğuna sırayla eklenir; herkes oy verebilir.")
    : t("Listeler bu bilgisayarda saklanır. Müzik modunda video gizlenir, görselleştirici açılır.")}</div>
{:else}
  <div class="head">
    <button class="icon" title={t("Listeler")} onclick={() => (openId = "")}><Icon name="back" size={14} /></button>
    {#if renaming}
      <input class="name" maxlength="80" bind:value={nameDraft}
        onkeydown={(e) => { if (e.key === "Enter") { renameList(list.id, nameDraft); renaming = false; } else if (e.key === "Escape") renaming = false; }} />
      <button class="small" onclick={() => { renameList(list.id, nameDraft); renaming = false; }}>{t("Kaydet")}</button>
    {:else}
      <h3 title={t("Yeniden adlandırmak için tıkla")}><button class="link" onclick={() => { nameDraft = dn(list.name); renaming = true; }}>{dn(list.name)}</button></h3>
    {/if}
  </div>
  <div class="meta">{summary(list.tracks)}</div>
  <div class="actions">
    <button class="white" disabled={!list.tracks.length} onclick={() => play(list.id)}><Icon name="play" size={13} />{t("Çal")}</button>
    <button class="small" disabled={list.tracks.length < 2} onclick={() => play(list.id, 0, true)}><Icon name="shuffle" size={13} />{t("Karıştır")}</button>
    {#if current}
      <button class="small" title={current.title} onclick={() => current && onLog(addTracks(list.id, [current]) ? t("Listeye eklendi: ") + current.title : t("Zaten listede"))}>
        <Icon name="plus" size={13} />{t("Çalanı ekle")}</button>
    {/if}
    <span class="sp"></span>
    {#if confirmDelete}
      <button class="small danger" onclick={() => { deleteList(list.id); openId = ""; }}>{t("Sil")}</button>
      <button class="small" onclick={() => (confirmDelete = false)}>{t("Vazgeç")}</button>
    {:else}
      <button class="icon" title={t("Listeyi sil")} onclick={() => (confirmDelete = true)}><Icon name="trash" size={13} /></button>
    {/if}
  </div>

  <div class="row">
    <input placeholder={t("Şarkı ara ya da YouTube/Spotify linki yapıştır")} bind:value={query} onkeydown={(e) => e.key === "Enter" && find()} />
    <button class="small" disabled={!query.trim() || busy} onclick={find}>{busy ? "…" : /^https?:\/\//i.test(query.trim()) ? t("Ekle") : t("Ara")}</button>
  </div>
  <div class="hint">{t("YouTube/SoundCloud çalma listesi linki yapıştırırsan tüm şarkıları eklenir.")}</div>

  {#if hits.length}
    <div class="eyebrow gap">{t("Sonuçlar")} <button class="link" onclick={() => (hits = [])}>{t("kapat")}</button></div>
    <ul class="tracks">
      {#each hits as h (h.url)}
        {@const added = list.tracks.some((t) => t.src === h.url)}
        <li>
          <span class="t" title={h.url}>{h.title}<small>{[h.channel, fmtDur(h.durationSecs)].filter(Boolean).join(" · ")}</small></span>
          <button class="icon" class:on={added} title={added ? t("Listede") : t("Listeye ekle")} disabled={added}
            onclick={() => addTracks(list.id, [toTrack(h)])}><Icon name={added ? "check" : "plus"} size={13} /></button>
        </li>
      {/each}
    </ul>
  {/if}

  <div class="eyebrow gap">{t("Şarkılar")}</div>
  {#if list.tracks.length}
    <ol class="tracks">
      {#each list.tracks as tk, i (tk.src)}
        <li class:on={playing.id === list.id && now?.src === tk.src}>
          <button class="t play" title={inRoom ? t("Buradan itibaren Lounge'a ekle") : t("Buradan çal")} onclick={() => play(list.id, i)}>
            {tk.title}<small>{fmtDur(tk.dur)}</small>
          </button>
          <span class="tools">
            <button class="icon" title={t("Yukarı")} disabled={i === 0} onclick={() => moveTrack(list.id, i, -1)}><Icon name="up" size={12} /></button>
            <button class="icon" title={t("Aşağı")} disabled={i === list.tracks.length - 1} onclick={() => moveTrack(list.id, i, 1)}><Icon name="dn" size={12} /></button>
            <button class="icon" title={t("Listeden çıkar")} onclick={() => removeTrack(list.id, tk.src)}><Icon name="x" size={12} /></button>
          </span>
        </li>
      {/each}
    </ol>
  {:else}
    <div class="hint">{t("Liste boş. Yukarıdan şarkı ara ya da link yapıştır.")}</div>
  {/if}
{/if}

<style>
  .now, .playing, .head, .actions, .row { display: flex; align-items: center; gap: 8px; }
  .now { margin-bottom: 10px; }
  .t { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .now .t { font-weight: 600; }
  .like.on { color: #ff4f8b; border-color: #ff4f8b; }
  .playing { padding: 8px 10px; border-radius: 10px; background: var(--glass); margin-bottom: 12px; color: var(--accent); }
  .playing .t, .lists .t, .tracks .t { display: flex; flex-direction: column; }
  .playing b { color: var(--text-1, #fff); font-weight: 600; overflow: hidden; text-overflow: ellipsis; }
  small { color: var(--text-3); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row input { flex: 1; min-width: 0; }
  .row button, .actions button { display: inline-flex; align-items: center; gap: 5px; }
  .gap { margin-top: 16px; }
  .hint { color: var(--text-2); font-size: 12px; margin-top: 6px; }
  .lists, .tracks { list-style: none; margin: 8px 0 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .lists li, .tracks li { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-radius: 10px; background: var(--glass); }
  .lists li.on, .tracks li.on { outline: 1px solid var(--accent); }
  .tracks li.on .t { color: var(--accent); }
  .open, .play { all: unset; cursor: pointer; flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; }
  .play { flex-direction: column; align-items: stretch; gap: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cover { width: 34px; height: 34px; flex: none; display: grid; place-items: center; border-radius: 8px; background: linear-gradient(135deg, #5b3df5, #1db9a4); color: #fff; }
  .lists .t b { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .icon { width: 24px; height: 24px; padding: 0; display: inline-grid; place-items: center; flex: none; }
  .icon.on { color: var(--accent); }
  .tools { display: flex; gap: 2px; opacity: 0; transition: opacity 0.15s; }
  .tracks li:hover .tools, .tracks li:focus-within .tools { opacity: 1; }
  .head { margin-bottom: 2px; }
  .head h3 { margin: 0; flex: 1; min-width: 0; font-size: 17px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .head .name { flex: 1; min-width: 0; }
  .link { all: unset; cursor: pointer; }
  .eyebrow .link { margin-left: 6px; color: var(--text-3); text-transform: none; letter-spacing: 0; }
  .meta { color: var(--text-3); font-size: 12px; margin: 0 0 10px 32px; }
  .actions { margin-bottom: 12px; flex-wrap: wrap; }
  .sp { flex: 1; }
  .danger { color: #ff6b6b; border-color: #ff6b6b; }
</style>
