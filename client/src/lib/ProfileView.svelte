<script lang="ts">
  // Profil (profil-liste mockup pl0/pl1): İzleyeceklerim / İzlediklerim. Kendi profilinde
  // listeleri düzenlersin; arkadaşının profilinde ikinizin de istediği filmler öne çıkar.
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import { catalogSearch, listsAdd, listsGet, listsRate, listsRemove, type CatalogTitle, type ListEntry, type ListName, type Lists } from "./player";
  import { social, splitHandle } from "./social.svelte";
  import { t, tx, locale } from "./i18n.svelte";

  let {
    user = "",
    busy = false,
    onBack,
    onAvatar,
    onNight,
    onJoin,
  }: {
    /** Arkadaşın `Ad#1234`'ü; boşsa kendi profilin. */
    user?: string;
    busy?: boolean;
    onBack: () => void;
    onAvatar: () => void;
    /** Ne izlesek? odası kur ve bu arkadaşı çağır. */
    onNight: (friend: string) => void;
    onJoin: (invite: string) => void;
  } = $props();

  const own = $derived(!user);
  let lists = $state<Lists | null>(null);
  let error = $state("");
  $effect(() => {
    const u = user;
    lists = null;
    error = "";
    if (!social.server) return;
    let live = true;
    listsGet(social.server, u || undefined).then(
      (l) => { if (live) lists = l; },
      (e) => { if (live) error = String(e).includes("not_friends") ? t("Listeleri yalnızca karşılıklı arkadaşlar görebilir.") : t("Listeler alınamadı: {e}", { e: tx(String(e)) }); },
    );
    return () => { live = false; };
  });

  async function change(f: () => Promise<Lists | null>) {
    try {
      const l = await f();
      if (l) lists = l;
    } catch (e) { error = t("Liste güncellenemedi: {e}", { e: tx(String(e)) }); }
  }
  const remove = (list: ListName, ref: string) => change(() => listsRemove(social.server, list, ref));
  const markSeen = (e: ListEntry) => change(() => listsAdd(social.server, "seen", { ref: e.ref, kind: e.kind, title: e.title, year: e.year, poster: e.poster }));
  const rate = (e: ListEntry, r: number) => change(() => listsRate(social.server, e.ref, e.rating === r ? 0 : r));
  /** Arkadaşın listesinden kendi İzleyeceklerime: kendi listeni değiştirir, görünümü değil. */
  let added = $state<string[]>([]);
  async function wantToo(e: ListEntry) {
    try {
      await listsAdd(social.server, "want", { ref: e.ref, kind: e.kind, title: e.title, year: e.year, poster: e.poster, source: "friend" });
      added = [...added, e.ref];
    } catch (err) { error = t("Eklenemedi: {e}", { e: tx(String(err)) }); }
  }

  // ---- kendi profilin: sekmeler ve süzgeç
  let tab = $state<ListName>("want");
  let chip = $state<"all" | "common" | "movie" | "series">("all");
  const want = $derived(lists?.want ?? []);
  const seen = $derived(lists?.seen ?? []);
  const commonN = $derived(want.filter((e) => e.mates?.length).length);
  const shownWant = $derived(want.filter((e) => chip === "all" || (chip === "common" ? !!e.mates?.length : e.kind === chip)));
  const shownSeen = $derived(seen.filter((e) => chip === "all" || chip === "common" || e.kind === chip));

  // ---- film ya da dizi ekle (Cinemeta araması, ref = IMDb kimliği)
  let adding = $state(false);
  let q = $state("");
  let found = $state<CatalogTitle[]>([]);
  let searching = $state(false);
  $effect(() => {
    const s = q.trim();
    if (!adding || s.length < 2) { found = []; return; }
    let live = true;
    const tm = setTimeout(() => {
      searching = true;
      catalogSearch(s).then((r) => { if (live) found = r.slice(0, 8); }, () => {}).finally(() => { if (live) searching = false; });
    }, 350);
    return () => { live = false; clearTimeout(tm); };
  });
  function addTitle(ct: CatalogTitle) {
    const year = parseInt(ct.releaseInfo ?? "", 10);
    const entry: ListEntry = { ref: ct.id, kind: ct.type, title: ct.name, year: Number.isFinite(year) ? year : undefined, poster: ct.poster?.startsWith("https://") ? ct.poster : undefined };
    void change(() => listsAdd(social.server, tab, entry));
    adding = false;
    q = "";
  }

  // ---- arkadaşın profili
  const friend = $derived(own ? null : social.presence?.friends.find((f) => f.name === user || splitHandle(f.name)[0] === splitHandle(user)[0]));
  const common = $derived(lists?.common ?? []);
  const commonItems = $derived(want.filter((e) => common.includes(e.ref)));
  const myName = $derived(social.profile?.name ?? "");
  const withMe = (e: ListEntry) => (e.with ?? []).some((w) => splitHandle(w)[0].toLocaleLowerCase(locale()) === splitHandle(myName)[0].toLocaleLowerCase(locale()));

  const [name, tag] = $derived(splitHandle(lists?.user || user || social.profile?.name || ""));
  const avatar = $derived(own ? social.profile?.avatar : (lists?.avatar ?? friend?.avatar));

  // ---- yardımcılar
  function hue(s: string) {
    let h = 0;
    for (const c of s) h = (h * 53 + c.codePointAt(0)! * 7) % 360;
    return h;
  }
  function ago(ts?: number): string {
    if (!ts) return "";
    const d = Math.floor((Date.now() - ts) / 86400000);
    if (d <= 0) return t("bugün");
    if (d === 1) return t("dün");
    if (d < 7) return t("{n} gün", { n: d });
    if (d < 30) return t("{n} hf", { n: Math.floor(d / 7) });
    return d < 365 ? t("{n} ay", { n: Math.floor(d / 30) }) : t("{n} yıl", { n: Math.floor(d / 365) });
  }
  const SOURCES: Record<string, string> = { night: "Ne izlesek?'ten", titles: "Film & Dizi'den", friend: "arkadaş listesinden" };
  function wantLine(e: (typeof want)[number]): string {
    if (e.mates?.length) {
      const n = e.mates.map((m) => splitHandle(m.name)[0]);
      return t("{names} de istiyor", { names: n.length <= 2 ? n.join(", ") : `${n.slice(0, 2).join(", ")} +${n.length - 2}` });
    }
    return `${t(SOURCES[e.source ?? ""] ?? "Sen ekledin")} · ${ago(e.at)}`;
  }
  function withLine(e: ListEntry): string {
    const w = (e.with ?? []).map((x) => splitHandle(x)[0]);
    if (!w.length) return t("tek başına");
    if (!own && withMe(e)) {
      const others = (e.with ?? []).filter((x) => splitHandle(x)[0].toLocaleLowerCase(locale()) !== splitHandle(myName)[0].toLocaleLowerCase(locale())).map((x) => splitHandle(x)[0]);
      return others.length ? t("seninle, {names}", { names: others.join(", ") }) : t("seninle");
    }
    return t("{names} ile", { names: `${w.slice(0, 2).join(", ")}${w.length > 2 ? ` +${w.length - 2}` : ""}` });
  }
  const sub = (e: ListEntry) => [e.year, e.kind === "series" ? t("dizi") : t("film")].filter(Boolean).join(" · ");
  const dead = new Set<string>();
  const focus = (el: HTMLElement) => el.focus();
</script>

{#snippet poster(e: ListEntry, small = false)}
  <span class="po" class:small style="--h:{hue(e.title)}">
    {#if e.poster && !dead.has(e.ref)}<img src={e.poster} alt="" loading="lazy" referrerpolicy="no-referrer" onerror={(ev) => { dead.add(e.ref); (ev.currentTarget as HTMLElement).style.display = "none"; }} />{/if}
    {#if !small}<b>{e.title.toLocaleUpperCase(locale())}</b>{/if}
  </span>
{/snippet}

{#snippet seenRow(e: ListEntry)}
  <div class="sr">
    {@render poster(e, true)}
    <span class="t"><b>{e.title}</b><small>{sub(e)}</small></span>
    <span class="with">{#each (e.with ?? []).slice(0, 3) as w (w)}<Avatar id={social.presence?.friends.find((f) => splitHandle(f.name)[0] === splitHandle(w)[0])?.avatar ?? (withMe(e) && splitHandle(w)[0] === splitHandle(myName)[0] ? social.profile?.avatar : undefined)} name={w} size={20} />{/each}<em>{withLine(e)}</em></span>
    {#if own}
      <span class="rate">
        <button class:on={e.rating === 1} title={t("Beğendim")} onclick={() => rate(e, 1)}>❤️</button>
        <button class:on={e.rating === -1} title={t("Beğenmedim")} onclick={() => rate(e, -1)}>👎</button>
      </span>
    {:else}
      <span class="rate ro">{e.rating === 1 ? "❤️" : e.rating === -1 ? "👎" : ""}{#if !e.rating}<em>{t("oy yok")}</em>{/if}</span>
    {/if}
    <small class="when">{ago(e.at)}</small>
    {#if own}<button class="x" title={t("Listemden çıkar")} aria-label={t("Listemden çıkar")} onclick={() => remove("seen", e.ref)}><Icon name="x" size={11} /></button>{/if}
  </div>
{/snippet}

<div class="pv">
  <button class="back" onclick={onBack}><Icon name="back" size={13} /> {t("Ana ekran")}</button>

  <div class="head">
    <span class="ring"><Avatar id={avatar} name={name || "?"} size={84} /></span>
    <div class="who">
      <h1>{name}{#if tag}<span class="tag">#{tag}</span>{/if}</h1>
      <div class="nums">
        <span><b>{seen.length}</b> {t("izlendi")}</span>
        <span><b>{want.length}</b> {t("izlenecek")}</span>
        {#if !own && lists?.together}<span><b>{lists.together}</b> {t("film birlikte izlediniz")}</span>{/if}
        {#if friend?.room}<span class="r">{t("{room} odasında", { room: friend.room })}{friend.title ? ` · ${friend.title}` : ""}</span>{/if}
      </div>
    </div>
    {#if own}<button class="lg" onclick={onAvatar}><Icon name="pencil" size={13} /> {t("Profil fotoğrafı")}</button>
    {:else if friend?.invite}<button class="lg" disabled={busy} onclick={() => onJoin(friend.invite!)}>{t("Katıl")}</button>{/if}
  </div>

  {#if error}<div class="err">{error}</div>{/if}

  {#if own}
    <div class="bar">
      <span class="tabs" role="tablist">
        <button role="tab" class:on={tab === "want"} onclick={() => { tab = "want"; }}>{t("İzleyeceklerim")} <i>{want.length}</i></button>
        <button role="tab" class:on={tab === "seen"} onclick={() => { tab = "seen"; if (chip === "common") chip = "all"; }}>{t("İzlediklerim")} <i>{seen.length}</i></button>
      </span>
      <span class="chips">
        <button class:on={chip === "all"} onclick={() => (chip = "all")}>{t("Tümü")}</button>
        {#if tab === "want"}<button class:on={chip === "common"} onclick={() => (chip = "common")}>{t("Ortak olanlar")} · {commonN}</button>{/if}
        <button class:on={chip === "movie"} onclick={() => (chip = "movie")}>{t("Film")}</button>
        <button class:on={chip === "series"} onclick={() => (chip = "series")}>{t("Dizi")}</button>
      </span>
    </div>

    {#if adding}
      <div class="add-box">
        <input placeholder={t("Film ya da dizi ara…")} bind:value={q} use:focus onkeydown={(e) => { if (e.key === "Escape") adding = false; }} />
        <button class="sm" onclick={() => { adding = false; q = ""; }}>{t("Vazgeç")}</button>
        {#if found.length || searching}
          <div class="found">
            {#if searching && !found.length}<p class="muted">{t("Aranıyor…")}</p>{/if}
            {#each found as ft (ft.id)}
              <button class="fo" onclick={() => addTitle(ft)}>
                <span class="po small" style="--h:{hue(ft.name)}">{#if ft.poster}<img src={ft.poster} alt="" referrerpolicy="no-referrer" onerror={(ev) => ((ev.currentTarget as HTMLElement).style.display = "none")} />{/if}</span>
                <span class="t"><b>{ft.name}</b><small>{[ft.releaseInfo, ft.type === "series" ? t("dizi") : t("film")].filter(Boolean).join(" · ")}</small></span>
                <em>+ {tab === "want" ? t("İzleyeceklerime") : t("İzlediklerime")}</em>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    {#if lists === null && !error}<p class="muted">{t("Yükleniyor…")}</p>
    {:else if tab === "want"}
      <div class="grid">
        {#each shownWant as e (e.ref)}
          <div class="card">
            <div class="pw">
              {@render poster(e)}
              {#if e.mates?.length}
                <span class="mates">{#each e.mates.slice(0, 3) as m (m.name)}<Avatar id={m.avatar} name={m.name} size={20} />{/each}</span>
                <span class="ortak">{t("ORTAK")}</span>
              {/if}
              <span class="hov">
                <button title={t("İzledim olarak işaretle")} onclick={() => markSeen(e)}>{t("✓ İzledim")}</button>
                <button class="x2" title={t("Listemden çıkar")} aria-label={t("Listemden çıkar")} onclick={() => remove("want", e.ref)}><Icon name="x" size={11} /></button>
              </span>
            </div>
            <b>{e.title}</b><small>{wantLine(e)}</small>
          </div>
        {/each}
        {#if !adding}<button class="card addc" onclick={() => (adding = true)}><span>+</span>{t("Film ya da dizi ekle")}</button>{/if}
      </div>
      {#if !want.length}<p class="muted">{t("Ne izlesek? oylamasında, Film & Dizi künyesinde ya da buradan ekleyebilirsin. Arkadaşların da aynı başlığı isterse ORTAK olur.")}</p>{/if}
    {:else}
      <div class="rows">
        {#each shownSeen as e (e.ref)}{@render seenRow(e)}{/each}
        {#if !adding}<button class="addr" onclick={() => (adding = true)}>{t("+ İzlediğin bir film ya da dizi ekle")}</button>{/if}
      </div>
      {#if !seen.length}<p class="muted">{t("Odada bir filmin %80'ini birlikte izleyince buraya kiminle izlediğinizle kendiliğinden düşer.")}</p>{/if}
    {/if}
  {:else if lists}
    {#if commonItems.length}
      <div class="common">
        <span class="stack">{#each commonItems.slice(0, 4) as e (e.ref)}{@render poster(e, true)}{/each}</span>
        <span class="t"><b>{t("İkinizin de listesinde {n} başlık var", { n: commonItems.length })}</b>
          <small>{commonItems.slice(0, 4).map((e) => e.title).join(", ")} · {t("bu akşam bunlardan biri?")}</small></span>
        <button class="lg go" disabled={busy} onclick={() => onNight(lists!.user)}>{t("✦ Ne izlesek?'e koy")}</button>
      </div>
    {/if}
    <div class="two">
      <div class="col">
        <div class="lab">{t("İzledikleri")} <span>{seen.length}</span></div>
        <div class="rows">
          {#each seen as e (e.ref)}{@render seenRow(e)}{:else}<p class="muted">{t("Henüz yok.")}</p>{/each}
        </div>
      </div>
      <div class="col">
        <div class="lab">{t("İzleyecekleri")} <span>{want.length}</span></div>
        <div class="grid g3">
          {#each want as e (e.ref)}
            {@const both = common.includes(e.ref)}
            <div class="card">
              <div class="pw">
                {@render poster(e)}
                {#if both}<span class="ortak">{t("ORTAK")}</span>
                {:else}<span class="hov">
                  <button disabled={added.includes(e.ref)} onclick={() => wantToo(e)}>{added.includes(e.ref) ? t("✓ Eklendi") : t("+ Ben de")}</button></span>{/if}
              </div>
              <b>{e.title}</b><small>{both ? t("ikiniz de istiyorsunuz") : t("{name} ekledi", { name })}</small>
            </div>
          {:else}<p class="muted">{t("Henüz yok.")}</p>{/each}
        </div>
      </div>
    </div>
  {:else if !error}<p class="muted">{t("Yükleniyor…")}</p>{/if}
</div>

<style>
  .pv {
    min-height: 0; overflow: auto; padding: 18px 32px 28px; display: flex; flex-direction: column; gap: 14px;
    background: radial-gradient(70% 55% at 15% 0%, rgba(10, 132, 255, 0.14), transparent 70%), radial-gradient(40% 40% at 100% 100%, rgba(191, 90, 242, 0.08), transparent 70%);
  }
  .back { align-self: flex-start; height: auto; padding: 2px 0; background: none; border: 0; color: var(--text-2); font-size: 12.5px; font-weight: 600; display: flex; gap: 6px; align-items: center; }
  .back:hover { background: none; color: #fff; }
  .muted { margin: 0; color: var(--text-2); font-size: 12.5px; }
  .err { color: #ff6961; font-size: 12.5px; }
  .head { display: flex; align-items: center; gap: 18px; }
  .ring { border-radius: 50%; box-shadow: 0 0 0 3px #0b0c0e, 0 0 0 5px rgba(48, 209, 88, 0.6); flex: none; }
  .who { flex: 1; min-width: 0; }
  .who h1 { margin: 0; font-size: 32px; font-weight: 900; letter-spacing: -0.035em; line-height: 1.1; }
  .tag { color: var(--text-3); font: 500 0.55em var(--font-mono); margin-left: 8px; }
  .nums { display: flex; gap: 14px; flex-wrap: wrap; color: var(--text-2); font-size: 13px; margin-top: 4px; }
  .nums b { color: #fff; }
  .nums .r { color: #64b0ff; }
  .head .lg { display: flex; gap: 6px; align-items: center; }

  .bar { display: flex; align-items: flex-end; justify-content: space-between; gap: 10px; border-bottom: 1px solid var(--line); }
  .tabs { display: flex; gap: 4px; }
  .tabs button { height: auto; padding: 8px 14px 10px; background: none; border: 0; border-bottom: 2px solid transparent; border-radius: 0; font-size: 14px; font-weight: 700; color: var(--text-2); }
  .tabs button.on { color: #fff; border-bottom-color: #fff; }
  .tabs i { font-style: normal; font-size: 11px; padding: 1px 7px; border-radius: 99px; background: var(--glass-2); margin-left: 4px; color: var(--text-2); }
  .chips { display: flex; gap: 6px; padding-bottom: 8px; }
  .chips button { height: 28px; padding: 0 12px; border-radius: 99px; font-size: 12px; font-weight: 650; }
  .chips button.on { background: #fff; color: #000; border-color: #fff; }

  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(124px, 1fr)); gap: 14px 12px; }
  .card { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .card > b { font-size: 12.5px; font-weight: 650; margin-top: 6px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .card > small { font-size: 11px; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pw { position: relative; }
  .po { position: relative; display: block; aspect-ratio: 2 / 3; border-radius: 10px; overflow: hidden; border: 1px solid var(--line-2);
    background: radial-gradient(60% 50% at 40% 30%, hsl(var(--h) 40% 30%), transparent 75%), linear-gradient(180deg, hsl(var(--h) 30% 16%), hsl(var(--h) 30% 5%)); }
  .po img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .po b { position: absolute; left: 10px; right: 10px; bottom: 10px; font-size: 13px; font-weight: 900; letter-spacing: -0.01em; line-height: 1.05; text-shadow: 0 2px 8px rgba(0, 0, 0, 0.6); }
  .po img + b { display: none; }
  .po.small { width: 38px; flex: none; border-radius: 6px; }
  .mates { position: absolute; left: 8px; top: 8px; display: flex; }
  .mates :global(.avw) { margin-left: -5px; border-radius: 50%; box-shadow: 0 0 0 1.5px #000; }
  .mates :global(.avw:first-child) { margin-left: 0; }
  .ortak { position: absolute; right: 8px; top: 8px; padding: 2px 8px; border-radius: 99px; background: var(--green); color: #00170a; font-size: 10px; font-weight: 850; }
  .hov { position: absolute; left: 0; right: 0; bottom: 0; padding: 8px; display: flex; gap: 6px; justify-content: center; opacity: 0; transition: opacity 0.15s; background: linear-gradient(0deg, rgba(0, 0, 0, 0.85), transparent); border-radius: 0 0 10px 10px; }
  .pw:hover .hov, .hov:focus-within { opacity: 1; }
  .hov button { height: 28px; padding: 0 10px; border-radius: 8px; font-size: 11.5px; font-weight: 700; background: rgba(255, 255, 255, 0.92); color: #000; border: 0; }
  .hov button.x2 { width: 28px; padding: 0; display: grid; place-items: center; background: rgba(0, 0, 0, 0.6); color: #fff; border: 1px solid rgba(255, 255, 255, 0.25); }
  .addc { aspect-ratio: 2 / 3; height: auto; border-radius: 10px; border: 1.5px dashed rgba(255, 255, 255, 0.22); background: none; color: var(--text-2); font-size: 12px; font-weight: 650; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 4px; }
  .addc span { font-size: 22px; font-weight: 400; }

  .add-box { position: relative; display: flex; gap: 8px; }
  .add-box input { flex: 1; }
  .found { position: absolute; z-index: 5; left: 0; right: 0; top: 100%; margin-top: 6px; padding: 6px; border-radius: 14px; background: #1a1b1f; border: 1px solid var(--line-2); box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6); display: flex; flex-direction: column; gap: 2px; max-height: 380px; overflow: auto; }
  .found .muted { padding: 8px; }
  .fo { height: auto; display: flex; align-items: center; gap: 10px; padding: 6px 8px; background: none; border: 0; border-radius: 10px; text-align: left; }
  .fo:hover { background: var(--glass-2); }
  .fo em { font-style: normal; font-size: 11.5px; color: var(--text-3); white-space: nowrap; }
  .fo:hover em { color: #fff; }

  .t { flex: 1; min-width: 0; display: flex; flex-direction: column; line-height: 1.3; }
  .t b { font-weight: 650; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t small { color: var(--text-3); font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .rows { display: flex; flex-direction: column; gap: 6px; }
  .sr { display: flex; align-items: center; gap: 12px; padding: 8px 10px; border-radius: 12px; background: var(--glass); border: 1px solid var(--line); }
  .sr .t { flex: 1.2; }
  .with { flex: 1; min-width: 0; display: flex; align-items: center; }
  .with :global(.avw) { margin-left: -5px; border-radius: 50%; box-shadow: 0 0 0 1.5px #17181b; }
  .with :global(.avw:first-child) { margin-left: 0; }
  .with em { font-style: normal; font-size: 11.5px; color: var(--text-2); margin-left: 7px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rate { display: flex; gap: 2px; flex: none; }
  .rate button { width: 28px; height: 28px; padding: 0; border-radius: 8px; background: none; border: 0; opacity: 0.3; font-size: 14px; }
  .rate button.on { opacity: 1; }
  .rate button:hover { opacity: 0.8; background: var(--glass-2); }
  .rate.ro { width: 44px; justify-content: center; font-size: 14px; }
  .rate.ro em { font-style: normal; font-size: 11px; color: var(--text-3); }
  .when { flex: none; width: 52px; text-align: right; color: var(--text-3); font-size: 11px; }
  .x { flex: none; width: 24px; height: 24px; padding: 0; display: grid; place-items: center; background: none; border: 0; color: var(--text-3); }
  .addr { height: 38px; border-radius: 12px; border: 1.5px dashed rgba(255, 255, 255, 0.18); background: none; color: var(--text-2); font-size: 12px; font-weight: 650; }

  .common { display: flex; align-items: center; gap: 16px; padding: 12px 14px; border-radius: 16px; background: linear-gradient(90deg, rgba(48, 209, 88, 0.2), rgba(48, 209, 88, 0.05)); border: 1px solid rgba(48, 209, 88, 0.45); }
  .common .t b { font-size: 14px; }
  .common .t small { color: var(--text-2); font-size: 12px; }
  .stack { display: flex; flex: none; }
  .stack .po { margin-left: -14px; box-shadow: 0 0 0 2px #0f1a12; }
  .stack .po:first-child { margin-left: 0; }
  .go { background: var(--green); color: #00170a; border: 0; font-weight: 800; }
  .two { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 18px; }
  .col { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  .lab { display: flex; justify-content: space-between; color: var(--text-2); font-size: 11.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; }
  .lab span { color: var(--text-3); font-weight: 500; }
  .g3 { grid-template-columns: repeat(auto-fill, minmax(110px, 1fr)); }

  @media (max-width: 980px) {
    .two { grid-template-columns: 1fr; }
    .bar { flex-direction: column; align-items: stretch; }
  }
</style>
