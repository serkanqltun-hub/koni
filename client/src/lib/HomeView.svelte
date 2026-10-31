<script lang="ts">
  // Ana ekran (ana-ekran mockup h0): solda çağrılar, oda kur, istatistiklerin ve yarım
  // kalanlar; sağda profil ve arkadaşların anlık durumu. Odaya kod ile girilmez:
  // oda sahibi içeriden arkadaşlarını çağırır, çağrı burada belirir.
  import { untrack } from "svelte";
  import type { ResumeEntry } from "./resume";
  import Icon from "./Icon.svelte";
  import Avatar from "./Avatar.svelte";
  import SocialModals from "./SocialModals.svelte";
  import ProfileView from "./ProfileView.svelte";
  import { loadResume, removeResume } from "./resume";
  import { feedGet, friendsCall, lobbyDecline, statsGet, type FeedItem, type OpenRoom, type PresenceFriend, type RoomView, type Stats, type StatsPeriod } from "./player";
  import { refreshProfile, social, splitHandle } from "./social.svelte";
  import { i18n, t, tx, locale } from "./i18n.svelte";

  let {
    name,
    roomName = $bindable(),
    busy,
    error,
    welcome = false,
    onCreate,
    onJoin,
    onSolo,
    onResume,
  }: {
    name: string;
    roomName: string;
    busy: boolean;
    error: string | null;
    /** Yeni kayıt: profil fotoğrafı seçimi açılır. */
    welcome?: boolean;
    /** Oda kur: açılacak sahne ve kurulunca çağrılacak arkadaşlar. */
    onCreate: (mode: RoomView | "night" | null, calls?: string[]) => void;
    /** Çağrıyı kabul et: odaya davet koduyla gir. */
    onJoin: (code: string) => void;
    onSolo: () => void;
    onResume: (r: ResumeEntry) => void;
  } = $props();

  /** Açık profil (p0.23): "" kendi profilin, `Ad#1234` arkadaşın; `null` ana ekran. */
  let prof = $state<string | null>(null);
  let modal = $state<"invite" | "add" | "avatar" | null>(untrack(() => welcome) ? "avatar" : null);

  // ---- arkadaşlar: lobi bağlıyken anlık, değilse bir kez liste
  let fallback = $state<{ friends: PresenceFriend[]; requests: { name: string; avatar: string }[] } | null>(null);
  let rev = $state(0);
  $effect(() => {
    void rev;
    if (!social.server) return;
    friendsCall(social.server, "list").then(
      (l) => (fallback = l && { friends: l.friends.map((f) => ({ ...f, online: false })), requests: l.requests }),
      () => {},
    );
  });
  const pres = $derived(social.presence ?? (fallback ? { ...fallback, calls: [] } : null));
  const mutual = $derived((pres?.friends ?? []).filter((f) => f.mutual));
  const online = $derived(mutual.filter((f) => f.online).sort((a, b) => Number(!!b.room) - Number(!!a.room)));
  const offline = $derived(mutual.filter((f) => !f.online));
  const calls = $derived([...(pres?.calls ?? [])].sort((a, b) => b.at - a.at));
  // Açık odalar (p0.22): geri dönebileceğin odalar ve arkadaşlarının odaları. Çağrısı
  // gelmiş oda zaten yukarıda: iki kez gösterme.
  const rooms = $derived((social.presence?.rooms ?? []).filter((r) => !calls.some((c) => c.invite === r.invite)).slice(0, 3));
  const bareName = (n: string) => splitHandle(n)[0].toLocaleLowerCase(locale());
  function avatarOf(n: string): string | undefined {
    const b = bareName(n);
    if (social.profile && bareName(social.profile.name) === b) return social.profile.avatar;
    return pres?.friends.find((f) => bareName(f.name) === b)?.avatar;
  }
  function names(list: string[]): string {
    const n = list.map((x) => splitHandle(x)[0]);
    return n.length <= 2 ? n.join(", ") : `${n.slice(0, 2).join(", ")} +${n.length - 2}`;
  }
  function roomLine(r: OpenRoom): string {
    const who = r.members.length ? t("{names} içeride", { names: names(r.members) }) : t("kimse yok");
    if (r.mine && r.left) return t("{who} · {ago} çıktın", { who, ago: since(r.left) });
    return who;
  }
  function roomState(r: OpenRoom): [string, boolean] {
    if (r.night) return ["Ne izlesek?", true];
    if (r.view === "listen") return ["Müzik", true];
    if (r.title) return [r.playing ? "Oynuyor" : "Duraklatıldı", !!r.playing];
    return [r.view === "browse" ? "Keşfet" : "Sinema", true];
  }
  const clock = (sec: number) => {
    const s = Math.floor(sec), h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60);
    return h ? `${h}:${String(m).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}` : `${m}:${String(s % 60).padStart(2, "0")}`;
  };

  // Etkinlik sekmesi (p0.22): sen ve karşılıklı arkadaşların, son 7 gün
  let tab = $state<"friends" | "feed">("friends");
  let feed = $state<FeedItem[] | null>(null);
  let feedYou = "";
  $effect(() => {
    if (tab !== "feed" || !social.server) return;
    void rev;
    let live = true;
    feedGet(social.server).then((f) => { if (live && f) { feed = f.items; feedYou = f.you; } }, () => { if (live) feed = feed ?? []; });
    return () => { live = false; };
  });
  const isMe = (n: string) => sameUser(n, feedYou || social.profile?.name);
  function people(users: string[]): string {
    const others = users.filter((u) => !isMe(u)).map((u) => splitHandle(u)[0]);
    const me = users.some(isMe);
    if (!others.length) return t("Sen");
    const list = others.length > 2 ? `${others.slice(0, 2).join(", ")} +${others.length - 2}` : others.join(me ? ", " : t(" ve "));
    return me ? t("{list} ve sen", { list }) : list;
  }
  function feedLine(it: FeedItem): string {
    if (it.kind === "joined") return t("KONİ'ye katıldı") + (it.by ? " · " + t("{name} davet etti", { name: splitHandle(it.by)[0] }) : "");
    const many = it.users.length > 1;
    const what = it.title ? `${it.title}` : t("bir şey");
    return `${what} ${many ? t("izlediniz") : t("izledi")}`;
  }
  function day(ts: number): string {
    const d = new Date(ts), now = new Date();
    const diff = Math.floor((new Date(now.toDateString()).getTime() - new Date(d.toDateString()).getTime()) / 86400000);
    return diff <= 0 ? t("Bugün") : diff === 1 ? t("Dün") : d.toLocaleDateString(locale(), { weekday: "long" });
  }
  const feedDays = $derived.by(() => {
    const out: [string, FeedItem[]][] = [];
    for (const it of feed ?? []) {
      const d = day(it.at);
      const last = out[out.length - 1];
      if (last?.[0] === d) last[1].push(it); else out.push([d, [it]]);
    }
    return out;
  });
  const hm = (ts: number) => new Date(ts).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });

  // Oda kurarken sahne (mockup: Ne izlesek? / Sinema / Müzik / Keşfet)
  const MODES: [RoomView | "night", string, string, string][] = [
    ["night", "✦", "Ne izlesek?", "oylamayla başla"],
    ["watch", "🎬", "Sinema", "filmi sen seç"],
    ["listen", "🎵", "Müzik", "sırayı birlikte kur"],
    ["browse", "🧭", "Keşfet", "YouTube, siteler"],
  ];
  let mode = $state<RoomView | "night">((localStorage.getItem("koni.mode") as RoomView | "night" | null) ?? "browse");
  $effect(() => { try { localStorage.setItem("koni.mode", mode); } catch { /* depolama kapalı */ } });
  let busyFriend = $state(false);
  async function answer(user: string, yes: boolean) {
    if (busyFriend) return;
    busyFriend = true;
    try {
      await friendsCall(social.server, yes ? "add" : "decline", user);
      rev++;
    } catch { /* lobi bir sonraki itişte düzeltir */ } finally { busyFriend = false; }
  }
  function where(f: PresenceFriend): string {
    if (f.room) return t("{room} odasında", { room: f.room }) + (f.title ? " · " + f.title : "");
    if (f.online) return t("ana ekranda");
    return t("çevrimdışı");
  }

  // ---- istatistikler (yalnızca kendi satırın)
  const PERIODS: [StatsPeriod, string][] = [["week", "Bu hafta"], ["month", "Bu ay"], ["all", "Tümü"]];
  let period = $state<StatsPeriod>("week");
  let stats = $state<Stats | null>(null);
  let first = true;
  $effect(() => {
    const p = period;
    void rev;
    let live = true;
    // Sunucu olayları ~1 sn'de bir yazar: odadan yeni çıkınca son süre görünsün.
    const tm = setTimeout(() => statsGet(social.server, p).then((s) => { if (live) stats = s; }, () => {}), first ? 1200 : 0);
    first = false;
    return () => { live = false; clearTimeout(tm); };
  });
  function sameUser(a?: string, b?: string): boolean {
    if (!a || !b) return false;
    if (a === b) return true;
    return splitHandle(a)[0].toLowerCase() === splitHandle(b)[0].toLowerCase();
  }

  const mine = $derived(
    stats?.users?.find((u) => sameUser(u.user, stats?.you) || sameUser(u.user, social.profile?.name))
  );
  const badges = $derived.by(() => {
    const all = (stats?.badges ?? []).filter((b) => b.users?.length);
    const me = stats?.you || social.profile?.name || "";
    return [
      ...all.filter((b) => b.users!.some((u) => sameUser(u, me))),
      ...all.filter((b) => !b.users!.some((u) => sameUser(u, me))),
    ].slice(0, 3);
  });
  function badgeParts(title: string): [string, string] {
    const i = title.indexOf(" ");
    return i > 0 ? [title.slice(0, i), title.slice(i + 1)] : ["🏅", title];
  }
  function dur(sec: number): string {
    if (sec < 60) return t("0 dk");
    const h = Math.floor(sec / 3600), m = Math.round((sec % 3600) / 60);
    if (h === 0) return t("{m} dk", { m });
    return m && h < 10 ? t("{h} sa {m} dk", { h, m }) : t("{h} sa", { h });
  }

  // ---- son 7 gün (bugün en sonda)
  const DAYS = ["Paz", "Pzt", "Sal", "Çar", "Per", "Cuma", "Cmt"];
  const week = $derived.by(() => {
    const d = social.profile?.daily ?? [0, 0, 0, 0, 0, 0, 0];
    const top = Math.max(1, ...d);
    const today = new Date().getDay();
    return d.map((v, i) => ({ v, pct: Math.max(4, Math.round((v / top) * 100)), day: DAYS[(today - 6 + i + 7) % 7] }));
  });
  const best = $derived(week.reduce((a, b) => (b.v > a.v ? b : a), week[0]));

  // ---- yarım kalanlar
  let resumes = $state(loadResume().sort((a, b) => b.at - a.at).slice(0, 3));
  function drop(r: ResumeEntry) {
    removeResume(r.src);
    resumes = resumes.filter((x) => x.src !== r.src);
  }
  function ago(ts: number): string {
    const d = Math.floor((Date.now() - ts) / 86400000);
    return d <= 0 ? t("bugün") : d === 1 ? t("dün") : t("{d} gün önce", { d });
  }
  function since(ts: number): string {
    const m = Math.floor((Date.now() - ts) / 60000);
    return m < 1 ? t("şimdi") : m < 60 ? t("{m} dk önce", { m }) : t("{h} sa önce", { h: Math.floor(m / 60) });
  }
  function hue(s: string) {
    let h = 0;
    for (const c of s) h = (h * 53 + c.codePointAt(0)! * 7) % 360;
    return h;
  }

  const hour = new Date().getHours();
  const greet = hour < 5 ? "İyi geceler" : hour < 12 ? "Günaydın" : hour < 18 ? "İyi günler" : "İyi akşamlar";
  const when = hour < 5 ? "Bu gece" : hour < 12 ? "Bu sabah" : hour < 18 ? "Bugün" : "Bu akşam";
  const me = $derived(social.profile?.name ?? splitHandle(name)[0]);
  const display = $derived(me ? me.charAt(0).toLocaleUpperCase(locale()) + me.slice(1) : "");
</script>

<div class="home">
  {#if prof !== null}
  {#key prof}
  <ProfileView user={prof} {busy} onBack={() => (prof = null)} onAvatar={() => (modal = "avatar")}
    onNight={(f) => onCreate("night", [f])} onJoin={onJoin} />
  {/key}
  {:else}
  <div class="main">
    <div class="hello"><h1>{t(greet)}, {display}</h1><p>{t(when + " kimlerle, ne izliyoruz?")}</p></div>
    {#if error}<div class="err">{error}</div>{/if}

    {#each calls.slice(0, 2) as c (c.invite)}
      <div class="call">
        <Avatar id={c.avatar} name={c.from} size={40} />
        <span class="t"><b>{t("{name} seni \"{room}\" odasına çağırıyor", { name: splitHandle(c.from)[0], room: c.room })}</b><small>{since(c.at)}</small></span>
        <span class="go">
          <button class="sm" onclick={() => lobbyDecline(c.invite)}>{t("Sonra")}</button>
          <button class="sm primary" disabled={busy} onclick={() => onJoin(c.invite)}>{t("Katıl")}</button>
        </span>
      </div>
    {/each}

    {#if rooms.length}
      <div class="lab">{t("Açık odalar")} <span class="hint">{t("senin odan kapanmadıkça geri dönebilirsin")}</span></div>
      <div class="rooms" style="--n:{rooms.length}">
        {#each rooms as r (r.invite)}
          {@const [state, live] = roomState(r)}
          <div class="rc" class:mine={r.mine}>
            <div class="rth" style="--h:{hue(r.title || r.room)};--w:{r.dur ? Math.min(100, ((r.pos ?? 0) / r.dur) * 100) : 0}%">
              <span class="live" class:pause={!live}><i></i>{t(state)}</span>
              {#if r.mine}<span class="tagm">{t("SENİN ODAN")}</span>{/if}
              {#if r.title}<span class="now"><b>{r.title}</b>{#if r.dur}<em>{clock(r.pos ?? 0)} / {clock(r.dur)}</em>{/if}</span>{/if}
              {#if r.dur}<span class="pb"></span>{/if}
            </div>
            <div class="rb">
              <span class="t"><b>{r.room}</b><small>{roomLine(r)}</small></span>
              <span class="ppl">{#each r.members.slice(0, 3) as m (m)}<Avatar id={avatarOf(m)} name={m} size={22} />{/each}</span>
              <button class="sm" class:back={r.mine} disabled={busy} onclick={() => onJoin(r.invite)}>{r.mine ? t("Geri dön") : t("Katıl")}</button>
            </div>
          </div>
        {/each}
      </div>
    {/if}

    <div class="acts">
      <form class="act big" onsubmit={(e) => { e.preventDefault(); if (!busy) onCreate(mode); }}>
        <h3>{t("Oda kur")}</h3>
        <div class="row">
          <input class="lg" placeholder={t("Oda adı ne olsun?")} bind:value={roomName} maxlength="48" />
          <button class="white lg" type="submit" disabled={busy}>{busy ? "…" : t("Kur")}</button>
        </div>
        <div class="modes" role="radiogroup" aria-label={t("Odada ilk ne açılsın")}>
          {#each MODES as [id, e, label, sub] (id)}
            <button type="button" role="radio" aria-checked={mode === id} class:on={mode === id} onclick={() => (mode = id)}>
              <b>{e} {id === "night" && i18n.lang === "en" ? "What to watch?" : t(label)}</b><small>{t(sub)}</small></button>
          {/each}
        </div>
      </form>
      <div class="act">
        <h3>{t("Tek başına izle")}</h3><p>{t("Oda olmadan oynatıcı; kaldığın yer yine saklanır.")}</p>
        <button class="lg wide" onclick={onSolo}>{t("Oynatıcıyı aç")}</button>
      </div>
    </div>

    <div class="lab">{t("Senin KONİ'n")}
      <span class="seg per" role="tablist">
        {#each PERIODS as [p, label] (p)}<button role="tab" class:on={period === p} onclick={() => (period = p)}>{t(label)}</button>{/each}
      </span>
    </div>
    <div class="stats">
      <div class="st"><b>{dur(mine?.watchSec ?? 0)}</b><small>{t("izledin")}</small></div>
      <div class="st"><b>{mine?.days ?? 0}</b><small>{t("gün izledin")}</small></div>
      <div class="st"><b>{dur(mine?.nightSec ?? 0)}</b><small>{t("gece yarısından sonra")}</small></div>
      <div class="st"><b>{dur(mine?.longestSec ?? 0)}</b><small>{t("en uzun oturum")}</small></div>
    </div>
    <div class="wk">
      <div class="chart">
        <div class="ch-h"><span>{t("Son 7 gün")}</span>{#if best?.v}<span>{t("en çok")} <b>{t(best.day)}</b> · {dur(best.v)}</span>{/if}</div>
        <div class="bars">
          {#each week as d, i (i)}
            <div title={dur(d.v)}><i class:lo={d.v === 0} style="height:{d.pct}%"></i><span>{t(d.day)}</span></div>
          {/each}
        </div>
      </div>
      <div class="badges">
        {#each badges as b (b.id)}
          {@const [emoji, title] = badgeParts(b.title)}
          <div class="bdg" class:lock={!b.users!.some((u) => sameUser(u, stats?.you || social.profile?.name))} title={tx(b.rule)}>
            <span class="e">{emoji}</span>
            <span><b>{tx(title)}</b><small>{b.users!.map((u) => splitHandle(u)[0]).join(", ")} · {tx(b.rule).toLocaleLowerCase(locale())}</small></span>
          </div>
        {:else}
          <p class="muted">{t("Rozetler arkadaşlarınla izledikçe gelir.")}</p>
        {/each}
      </div>
    </div>

    {#if resumes.length}
      <div class="lab">{t("Yarım kalanlar")} <span class="hint">{t("aynı ekiple devam: oda kurulur, onlar çağrılır")}</span></div>
      <div class="cont">
        {#each resumes as r (r.src)}
          <div class="ci">
            <button class="go" disabled={busy} onclick={() => onResume(r)} title={r.src}>
              <span class="th" style="--h:{hue(r.title)};--w:{Math.min(100, (r.pos / Math.max(1, r.duration)) * 100)}%"></span>
              <span class="t"><b>{r.title}</b><small>{t("{time} kaldı", { time: dur(Math.max(0, r.duration - r.pos)) })} · {ago(r.at)}</small>
                {#if r.with?.length}<span class="with">{#each r.with.slice(0, 3) as m (m)}<Avatar id={avatarOf(m)} name={m} size={16} />{/each}<em>{t("{names} ile", { names: names(r.with) })}</em></span>{/if}</span>
            </button>
            <button class="x" title={t("Kaydı sil")} aria-label={t("Kaydı sil")} onclick={() => drop(r)}><Icon name="x" size={12} /></button>
          </div>
        {/each}
      </div>
    {/if}
  </div>
  {/if}

  <aside class="side">
    <div class="prof">
      <span class="mew">
        <button class="me" class:on={prof === ""} title={t("Profilin ve listelerin")} onclick={() => (prof = prof === "" ? null : "")}>
          <Avatar id={social.profile?.avatar} name={me} size={66} />
        </button>
        <button class="edit" title={t("Profil fotoğrafını değiştir")} aria-label={t("Profil fotoğrafını değiştir")} onclick={() => (modal = "avatar")}><Icon name="pencil" size={12} /></button>
      </span>
      <h2>{me}{#if social.profile?.tag}<span class="tag">&nbsp;#{social.profile.tag}</span>{/if}</h2>
      {#if social.profile?.admin}<span class="atag solo">{t("ADMIN")}</span>{/if}
      <small>{t("{n} arkadaş", { n: mutual.length })}{social.online ? "" : " · " + t("bağlanıyor…")}</small>
    </div>
    <div class="fr">
      <div class="fr-h">
        <span class="seg tabs" role="tablist">
          <button role="tab" class:on={tab === "friends"} onclick={() => (tab = "friends")}>{t("Arkadaşlar")} · {mutual.length}</button>
          <button role="tab" class:on={tab === "feed"} onclick={() => (tab = "feed")}>{t("Etkinlik")}</button>
        </span>
        <button class="ib" title={t("Arkadaş ekle")} aria-label={t("Arkadaş ekle")} onclick={() => (modal = "add")}>+</button></div>
      {#if tab === "feed"}
      <div class="list">
        {#if feed === null}<p class="muted empty">{t("Yükleniyor…")}</p>
        {:else if !feed.length}<p class="muted empty">{t("Bu hafta sessiz. Arkadaşlarınla izledikçe burada görünür.")}</p>{/if}
        {#each feedDays as [d, items] (d)}
          <div class="grp">{d}</div>
          {#each items as it (it.kind + it.at + it.users.join())}
            <div class="ev">
              <Avatar id={avatarOf(it.users.find((u) => !isMe(u)) ?? it.users[0])} name={it.users.find((u) => !isMe(u)) ?? it.users[0]} size={30} />
              <span class="t"><span class="l"><b>{people(it.users)}</b> {feedLine(it)}</span>
                <small>{it.kind === "watch" ? `${it.room ? it.room + " · " : ""}${dur(it.sec ?? 0)} · ` : ""}{hm(it.at)}</small></span>
              {#if it.kind === "watch" && it.title}<span class="evp" style="--h:{hue(it.title)}"></span>{/if}
            </div>
          {/each}
        {/each}
      </div>
      {:else}
      <div class="list">
        {#each pres?.requests ?? [] as r (r.name)}
          <div class="req">
            <Avatar id={r.avatar} name={r.name} size={34} />
            <span class="t"><b>{splitHandle(r.name)[0]}<span class="tag">#{splitHandle(r.name)[1]}</span></b><small>{t("arkadaşlık isteği gönderdi")}</small></span>
            <span class="yn">
              <button disabled={busyFriend} title={t("Reddet")} aria-label={t("Reddet")} onclick={() => answer(r.name, false)}>✕</button>
              <button class="y" disabled={busyFriend} title={t("Kabul et")} aria-label={t("Kabul et")} onclick={() => answer(r.name, true)}>✓</button>
            </span>
          </div>
        {/each}
        {#if online.length}<div class="grp">{t("Çevrimiçi")} · {online.length}</div>{/if}
        {#each online as f (f.name)}
          <div class="f" class:sel={prof === f.name}><button class="fo" title={t("Profili ve listeleri")} onclick={() => (prof = f.name)}><Avatar id={f.avatar} name={f.name} size={34} dot={f.room ? "room" : "on"} />
            <span class="t"><b>{splitHandle(f.name)[0]}{#if f.admin}<span class="atag">{t("ADMIN")}</span>{/if}</b><small class:r={!!f.room}>{where(f)}</small></span></button>
            {#if f.invite}<button class="fb" disabled={busy} title={t("{room} odasına katıl", { room: f.room ?? "" })} onclick={() => onJoin(f.invite!)}>{t("Katıl")}</button>
            {:else if !f.room}<button class="fb blue" disabled={busy} title={t("Oda kur ve {name} çağır", { name: splitHandle(f.name)[0] })} onclick={() => onCreate(mode, [f.name])}>{t("Çağır")}</button>{/if}
          </div>
        {/each}
        {#if offline.length}<div class="grp">{t("Çevrimdışı")}</div>{/if}
        {#each offline as f (f.name)}
          <div class="f off" class:sel={prof === f.name}><button class="fo" title={t("Profili ve listeleri")} onclick={() => (prof = f.name)}><Avatar id={f.avatar} name={f.name} size={34} dot="off" />
            <span class="t"><b>{splitHandle(f.name)[0]}{#if f.admin}<span class="atag">{t("ADMIN")}</span>{/if}</b><small class="mono">#{splitHandle(f.name)[1]}</small></span></button></div>
        {/each}
        {#if !mutual.length && !pres?.requests.length}
          <p class="muted empty">{t("Henüz arkadaşın yok. Birini davet et ya da \"Ad#1234\" ile ekle; kabul edince burada görünür.")}</p>
        {/if}
      </div>
      {/if}
      <div class="inv">
        <button class="lg" onclick={() => (modal = "add")}>{t("+ Arkadaş ekle")}</button>
        <button class="white lg" onclick={() => (modal = "invite")}>{t("Davet et")}</button>
      </div>
    </div>
  </aside>
</div>

{#if modal}
  <SocialModals kind={modal} {welcome} onClose={() => { modal = null; refreshProfile(); }} onFriends={() => rev++} />
{/if}

<style>
  .home { grid-area: main; min-height: 0; display: grid; grid-template-columns: 1fr 340px; }
  .main {
    min-height: 0; overflow: auto; padding: 22px 32px 28px; display: flex; flex-direction: column; gap: 13px;
    background: radial-gradient(70% 55% at 15% 0%, rgba(10, 132, 255, 0.14), transparent 70%), radial-gradient(40% 40% at 100% 100%, rgba(191, 90, 242, 0.08), transparent 70%);
  }
  .hello h1 { margin: 0; font-size: 32px; font-weight: 900; letter-spacing: -0.035em; line-height: 1.05; }
  .hello p { margin: 4px 0 0; color: var(--text-2); font-size: 14px; }
  .err { color: #ff6961; }
  .muted { margin: 0; color: var(--text-2); font-size: 12.5px; }
  .tag { color: var(--text-3); font: 500 0.88em var(--font-mono); }
  .t { flex: 1; min-width: 0; display: flex; flex-direction: column; line-height: 1.3; }
  .t b { font-weight: 650; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t small { color: var(--text-2); font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button.sm { height: 32px; padding: 0 12px; border-radius: 10px; font-size: 12px; font-weight: 650; }

  .call { display: flex; align-items: center; gap: 14px; padding: 9px 12px; border-radius: 16px; background: linear-gradient(90deg, rgba(10, 132, 255, 0.22), rgba(10, 132, 255, 0.06)); border: 1px solid rgba(10, 132, 255, 0.45); }
  .call b { font-size: 14px; }
  .call .go { display: flex; gap: 8px; }

  .hint { text-transform: none; letter-spacing: 0; color: var(--text-3); font-weight: 500; }
  .rooms { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; }
  .rc { position: relative; border-radius: 16px; overflow: hidden; border: 1px solid var(--line); background: var(--glass); display: flex; flex-direction: column; min-width: 0; }
  .rc.mine { border-color: rgba(48, 209, 88, 0.5); background: linear-gradient(120deg, rgba(48, 209, 88, 0.16), rgba(48, 209, 88, 0.03)); box-shadow: 0 14px 40px -18px rgba(48, 209, 88, 0.6); }
  .rth { position: relative; height: 62px; background: radial-gradient(40% 80% at 30% 40%, hsl(var(--h) 55% 38%), transparent 70%), linear-gradient(100deg, hsl(var(--h) 40% 10%), hsl(var(--h) 30% 4%)); }
  .rth::after { content: ""; position: absolute; inset: 0; background: linear-gradient(0deg, rgba(0, 0, 0, 0.7), transparent 70%); }
  .rth .live, .rth .tagm, .rth .now, .rth .pb { position: absolute; z-index: 1; }
  .rth .live { left: 10px; top: 9px; padding: 2px 8px; border-radius: 99px; font-size: 10.5px; font-weight: 750; background: rgba(0, 0, 0, 0.55); border: 1px solid rgba(255, 255, 255, 0.14); display: flex; gap: 5px; align-items: center; }
  .rth .live i { width: 6px; height: 6px; border-radius: 50%; background: var(--green); box-shadow: 0 0 6px var(--green); }
  .rth .live.pause i { background: var(--yellow); box-shadow: none; }
  .rth .tagm { right: 10px; top: 9px; padding: 2px 8px; border-radius: 99px; font-size: 10.5px; font-weight: 800; background: var(--green); color: #00170a; }
  .rth .now { left: 10px; right: 10px; bottom: 8px; font-size: 12px; display: flex; justify-content: space-between; gap: 8px; }
  .rth .now b { font-weight: 650; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rth .now em { font-style: normal; font: 500 11px var(--font-mono); color: rgba(255, 255, 255, 0.75); white-space: nowrap; }
  .rth .pb { left: 0; bottom: 0; height: 3px; width: var(--w); background: #fff; }
  .rb { padding: 9px 10px; display: flex; align-items: center; gap: 8px; }
  .rb .t b { font-size: 13.5px; }
  .rb .t small { font-size: 11px; }
  .ppl { display: flex; }
  .ppl :global(.avw) { margin-left: -6px; border-radius: 50%; box-shadow: 0 0 0 2px #17181b; }
  .ppl :global(.avw:first-child) { margin-left: 0; }
  .rb button { flex: none; height: 30px; }
  .rb button.back { background: var(--green); color: #00170a; border: 0; }
  .modes { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 6px; }
  .modes button { height: auto; padding: 7px 8px; border-radius: 11px; background: rgba(0, 0, 0, 0.28); border: 1px solid var(--line-2); text-align: left; display: flex; flex-direction: column; gap: 1px; color: var(--text-2); }
  .modes button b { font-size: 12px; font-weight: 650; white-space: nowrap; }
  .modes button small { font-size: 10.5px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .modes button.on { background: rgba(255, 255, 255, 0.12); color: #fff; border-color: rgba(255, 255, 255, 0.4); }
  .with { display: flex; align-items: center; gap: 0; margin-top: 3px; }
  .with :global(.avw) { margin-left: -5px; border-radius: 50%; box-shadow: 0 0 0 1.5px #17181b; }
  .with :global(.avw:first-child) { margin-left: 0; }
  .with em { font-style: normal; color: var(--text-3); font-size: 10.5px; margin-left: 6px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tabs { flex: 1; display: flex; }
  .tabs button { flex: 1; padding: 5px 0; font-size: 12.5px; font-weight: 650; height: auto; }
  .ev { display: flex; align-items: flex-start; gap: 10px; padding: 7px 6px; border-radius: 12px; }
  .ev .t .l { font-size: 12.5px; color: rgba(255, 255, 255, 0.85); line-height: 1.35; white-space: normal; }
  .ev .t .l b { color: #fff; white-space: normal; }
  .ev .t small { font-size: 11px; color: var(--text-3); }
  .evp { width: 30px; height: 44px; border-radius: 5px; flex: none; background: radial-gradient(50% 50% at 40% 40%, hsl(var(--h) 45% 38%), transparent 80%), hsl(var(--h) 30% 8%); box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.08); }
  .fb { flex: none; height: 26px; padding: 0 10px; border-radius: 8px; font-size: 11.5px; font-weight: 700; }
  .fb.blue { background: var(--accent); border: 0; color: #fff; }
  .acts { display: grid; grid-template-columns: 1.4fr 1fr; gap: 12px; }
  .act { border-radius: 18px; padding: 15px 16px; border: 1px solid var(--line); background: var(--glass); display: flex; flex-direction: column; gap: 10px; }
  .act h3 { margin: 0; font-size: 17px; font-weight: 800; letter-spacing: -0.02em; }
  .act p { margin: 0; color: var(--text-2); font-size: 12.5px; }
  .act.big { background: linear-gradient(120deg, rgba(10, 132, 255, 0.24), rgba(94, 92, 230, 0.16)); border-color: rgba(10, 132, 255, 0.4); }
  .row { display: flex; gap: 8px; }
  .row input { flex: 1; min-width: 0; background: rgba(0, 0, 0, 0.35); border: 1px solid var(--line-2); }
  .row input::placeholder { color: rgba(255, 255, 255, 0.35); }
  .wide { margin-top: auto; width: 100%; }

  .lab { display: flex; justify-content: space-between; align-items: center; color: var(--text-2); font-size: 11.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; margin-top: 4px; }
  .per { text-transform: none; letter-spacing: 0; }
  .per button { padding: 3px 9px; font-size: 12px; font-weight: 600; }
  .stats { display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; }
  .st { padding: 10px 14px; border-radius: 14px; background: var(--glass); border: 1px solid var(--line); }
  .st b { display: block; font-size: 22px; font-weight: 850; letter-spacing: -0.02em; font-variant-numeric: tabular-nums; }
  .st small { color: var(--text-2); font-size: 11.5px; }
  .wk { display: grid; grid-template-columns: 1.3fr 1fr; gap: 10px; }
  .chart { padding: 12px 14px; border-radius: 14px; background: var(--glass); border: 1px solid var(--line); display: flex; flex-direction: column; gap: 10px; }
  .ch-h { display: flex; justify-content: space-between; font-size: 12px; color: var(--text-2); }
  .ch-h b { color: #fff; }
  .bars { display: flex; align-items: flex-end; gap: 10px; height: 76px; }
  .bars div { flex: 1; display: flex; flex-direction: column; align-items: center; gap: 4px; height: 100%; justify-content: flex-end; }
  .bars i { width: 100%; border-radius: 6px 6px 3px 3px; background: linear-gradient(180deg, #3a9bff, #0a64c8); }
  .bars i.lo { background: rgba(255, 255, 255, 0.12); }
  .bars span { font-size: 10.5px; color: var(--text-3); }
  .badges { padding: 10px 14px; border-radius: 14px; background: var(--glass); border: 1px solid var(--line); display: flex; flex-direction: column; gap: 8px; justify-content: center; }
  .bdg { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .bdg .e { width: 30px; height: 30px; border-radius: 10px; display: grid; place-items: center; font-size: 18px; background: var(--glass-2); flex: none; }
  .bdg > span:last-child { min-width: 0; display: flex; flex-direction: column; }
  .bdg b { font-size: 13px; }
  .bdg small { color: var(--text-3); font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bdg.lock { opacity: 0.45; }

  .cont { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
  .ci { display: flex; align-items: center; border-radius: 14px; background: var(--glass); border: 1px solid var(--line); min-width: 0; }
  .ci .go { flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; padding: 9px; text-align: left; background: none; border: 0; }
  .th { width: 70px; height: 40px; border-radius: 8px; flex: none; position: relative; overflow: hidden; background: radial-gradient(40% 50% at 40% 40%, hsl(var(--h) 40% 35%), transparent 70%), hsl(var(--h) 30% 7%); }
  .th::after { content: ""; position: absolute; left: 0; bottom: 0; height: 3px; width: var(--w); background: #ff375f; }
  .ci .x { flex: none; width: 26px; height: 26px; margin-right: 6px; padding: 0; display: grid; place-items: center; background: none; border: 0; color: var(--text-3); }

  .side { background: var(--panel); border-left: 1px solid var(--line); display: flex; flex-direction: column; min-height: 0; --dot-ring: var(--panel); }
  .prof { padding: 16px 20px 14px; display: flex; flex-direction: column; align-items: center; gap: 6px; border-bottom: 1px solid var(--line); background: radial-gradient(80% 90% at 50% 0%, rgba(48, 209, 88, 0.12), transparent 70%); }
  .me { position: relative; padding: 0; border: 0; background: none; border-radius: 50%; box-shadow: 0 0 0 3px var(--panel), 0 0 0 5px rgba(48, 209, 88, 0.6), 0 16px 40px rgba(0, 0, 0, 0.6); }
  .me:hover { background: none; }
  .me.on { box-shadow: 0 0 0 3px var(--panel), 0 0 0 5px #fff, 0 16px 40px rgba(0, 0, 0, 0.6); }
  .mew { position: relative; }
  .fo { flex: 1; min-width: 0; height: auto; display: flex; align-items: center; gap: 11px; padding: 0; background: none; border: 0; text-align: left; }
  .fo:hover { background: none; }
  .f.sel { background: var(--glass-2); }
  .edit { position: absolute; right: -4px; bottom: -4px; width: 28px; height: 28px; padding: 0; border-radius: 50%; background: #fff; color: #000; display: grid; place-items: center; border: 3px solid var(--panel); }
  .edit:hover { background: #ddd; }
  .prof h2 { margin: 8px 0 0; font-size: 20px; font-weight: 850; letter-spacing: -0.02em; }
  .prof small { color: var(--text-3); font-size: 12px; }
  .atag.solo { margin: 0; }
  .fr { flex: 1; min-height: 0; display: flex; flex-direction: column; padding: 12px 14px 0; gap: 6px; }
  .fr-h { display: flex; align-items: center; gap: 8px; padding: 0 4px; }
  .ib { margin-left: auto; width: 32px; height: 32px; padding: 0; border-radius: 10px; display: grid; place-items: center; color: var(--text-2); font-size: 16px; }
  .list { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 4px; }
  .req { display: flex; align-items: center; gap: 10px; padding: 10px; border-radius: 14px; background: rgba(255, 159, 10, 0.08); border: 1px solid rgba(255, 159, 10, 0.3); }
  .yn { display: flex; gap: 4px; }
  .yn button { width: 30px; height: 30px; padding: 0; border-radius: 9px; font-weight: 800; }
  .yn button.y { background: var(--green); color: #000; }
  .grp { font-size: 11px; color: var(--text-3); font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; padding: 6px 6px 2px; }
  .f { display: flex; align-items: center; gap: 11px; padding: 5px 8px; border-radius: 12px; }
  .f:hover { background: var(--glass); }
  .f small.r { color: #64b0ff; }
  .f.off { opacity: 0.55; }
  .empty { padding: 8px 6px; }
  .inv { margin: 8px -14px 0; padding: 14px; border-top: 1px solid var(--line); display: flex; gap: 8px; }
  .inv button { flex: 1; }

  @media (max-width: 980px) {
    .home { grid-template-columns: 1fr 290px; }
    .stats { grid-template-columns: repeat(2, 1fr); }
    .wk, .acts { grid-template-columns: 1fr; }
    .cont { grid-template-columns: 1fr 1fr; }
    .rooms { grid-template-columns: 1fr; }
    .modes { grid-template-columns: 1fr 1fr; }
  }
</style>
