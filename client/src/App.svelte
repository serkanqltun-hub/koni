<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { fade, fly, scale } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { backOut, cubicOut } from "svelte/easing";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import {
    COCKPIT_LAYOUT, playerLoad, playerPause, playerTogglePause, playerSeek, playerSetSpeed,
    playerSetVolume, playerAddSubtitle, playerSubDelay, playerTracks, playerSetTrack,
    type Tracks, type TrackKind,
    playerState, setLayout, setFullscreen, openCastPanel, isMac, type PlayerState,
    syncConnect, syncPlay, syncPause, syncSeek, syncLeave, syncContinueWithout, syncResync, syncDownload,
    syncSetCohost, syncSetPolicy, accountGet, accountLogin, accountRegister, accountLogout, snifferClose,
    updateCheck, updateInstall, syncClose, playerAddSubtitleText, syncFun, mediaWaveform, mediaPrefetch, isSuperseded, queueAdd, bugReport,
    type SyncStatus, type ServerMsg, type Member, type MediaEvent, type LinkEvent, type DlStatus, type Policy,
    type SnifferEvent, type UpdateInfo, type FunMsg, type Movie, syncSetView, type RoomView,
    debridOpen, type Source, playerMusicView, playerPlay, playerCover, relayOn, setRelay, lobbyRemoteState,
  } from "./lib/player";
  import BrowseView from "./lib/BrowseView.svelte";
  import TitleBar from "./lib/TitleBar.svelte";
  import Logo from "./lib/Logo.svelte";
  import LoginView from "./lib/LoginView.svelte";
  import HomeView from "./lib/HomeView.svelte";
  import AdminView from "./lib/AdminView.svelte";
  import Avatar from "./lib/Avatar.svelte";
  import { onRemote, pic, social, startSocial, stopSocial, type RemoteOp } from "./lib/social.svelte";
  import SettingsView from "./lib/SettingsView.svelte";
  import { DEFAULT_SERVER, normServer } from "./lib/server";
  import { loadRecent, rememberRoom, forgetRoom, type RecentRoom } from "./lib/recent";
  import ResumeBar from "./lib/ResumeBar.svelte";
  import type { ResumeEntry, ResumeMedia } from "./lib/resume";
  import Icon from "./lib/Icon.svelte";
  import NightView from "./lib/NightView.svelte";
  import CinemaLobby from "./lib/CinemaLobby.svelte";
  import TitlesPane from "./lib/TitlesPane.svelte";
  import DebridSettings from "./lib/DebridSettings.svelte";
  import SubsFinder from "./lib/SubsFinder.svelte";
  import QueuePanel from "./lib/QueuePanel.svelte";
  import MusicView from "./lib/MusicView.svelte";
  import { artFor, splitTitle } from "./lib/music";
  import { isLiked, toggleLike } from "./lib/playlists.svelte";
  import PlaylistPanel from "./lib/PlaylistPanel.svelte";
  import { playing as listPlay, nowTrack, stepList, stopList, type Track } from "./lib/playlists.svelte";
  import { fun, onFun, resetFun } from "./lib/fun.svelte";
  import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
  import { parseInvite } from "./lib/invite";
  import Reactions from "./lib/Reactions.svelte";
  import NetBadge from "./lib/NetBadge.svelte";
  import BugReport from "./lib/BugReport.svelte";
  import { t, tx, locale } from "./lib/i18n.svelte";

  const L = COCKPIT_LAYOUT;
  const speeds = [0.96, 1.0, 1.04];
  const DENIED: Record<string, string> = {
    host_only: "Bu odada yalnızca host ve yardımcı hostlar kontrol edebilir",
    rate_limited: "Çok hızlı komut gönderdin",
    not_host: "Bunu yalnızca host yapabilir",
    bad_op: "Geçersiz komut",
    bad_media: "Geçersiz medya (http(s) adresi, dosya adı ya da bağlantı)",
  };
  const SUB_EXT = /\.(srt|ass|ssa|vtt|sub)$/i;

  let src = $state("");
  let st = $state<PlayerState | null>(null);
  /** Tek başına modda açılan videonun adı (başlık çubuğunda). */
  let soloTitle = $state("");
  /** Sürgünün üstünde gezinilen konum (sn); sol zaman etiketi bunu gösterir. */
  let hoverPos = $state<number | null>(null);
  /** Sessize almadan önceki ses düzeyi. */
  let lastVolume = 100;
  let fullscreen = $state(false);
  let seeking = $state(false);
  let seekVal = $state(0);
  /** Etkinlik satırı: `msg` Türkçe (sorun bildirimi), `show` ekranda o anki dilde. */
  let log = $state<{ t: string; msg: string; err: boolean; show: () => string }[]>([]);
  let pollErrLogged = false;

  // Oda
  let serverUrl = $state(localStorage.getItem("koni.server") || DEFAULT_SERVER);
  let username = $state(localStorage.getItem("koni.username") ?? "");
  let roomName = $state("");
  let inviteIn = $state("");
  let connecting = $state(false);
  let room = $state<{ name: string; invite: string; host: string; policy: Policy } | null>(null);
  let members = $state<Record<string, Member & { driftMs?: number; state?: string; bufferPct?: number; rttMs?: number }>>({});
  let sync = $state<SyncStatus | null>(null);
  let roomMedia = $state<MediaEvent | null>(null);
  let opening = $state(false);
  /** Açık Sanal Yakalayıcının sayfası (yt-dlp'nin tanımadığı site). */
  let sniffing = $state<string | null>(null);
  /** Video ekle'den açılan, yt-dlp'nin tanımadığı adres: Keşfet sayfada oynatır. */
  let pageWait = $state<string | null>(null);
  let volume = $state(Number(localStorage.getItem("koni.volume") ?? 100));
  /** Host'a gelen "onsuz devam" teklifleri: üye id → ad. */
  let offers = $state<Record<string, string>>({});
  let reconnecting = $state<number | null>(null);
  /** "Herkes İndirsin": geçerli görev ve üye başına durum. */
  let dlTask = $state<string | null>(null);
  let downloads = $state<Record<string, DlStatus>>({});
  /** Bu sunucuda kayıtlı hesap adı; hesapsız sunucuda null. */
  let account = $state<string | null>(null);
  /** Hesap kontrolü bitti mi (bitmeden giriş ekranı gösterilmez). */
  let accountChecked = $state(false);
  /** Hesapsız (açık) sunucuda yalnızca kullanıcı adıyla devam. */
  let guest = $state(localStorage.getItem("koni.guest") === "1");
  let authBusy = $state(false);
  /** Oda dışı ekranlarda gösterilen son hata. */
  let notice = $state<(() => string) | null>(null);
  const noticeText = $derived(notice ? notice() : null);
  /** Oda olmadan oynatıcı. */
  let solo = $state(false);
  /** Tek başına modda açılan kaynak ("Kaldığın yerden devam" için). */
  let soloMedia = $state<ResumeMedia | null>(null);
  /** Ana ekrandan "Yarım kalanlar"a tıklanınca oda kurulunca açılacak medya. */
  let pendingResume: ResumeEntry | null = null;
  let recent = $state(loadRecent());
  /** Son odalardan girilmeye çalışılan oda; sunucu kapattıysa aynı adla yeniden açılır. */
  let tryingRecent: RecentRoom | null = null;
  let reopen: string | null = null;
  /** Yeni kurulan odada açılacak sahne ("night" = Ne izlesek? ile başla). */
  let newMode: RoomView | "night" | null = null;
  /** Oda kurulunca çağrılacak arkadaşlar (etiketsiz ad ya da Ad#1234). */
  let pendingCalls: string[] = [];
  /** Odadan çıkarken: ayrıl / herkes için kapat penceresi. */
  let leaveAsk = $state(false);
  $effect(() => { if (!room) leaveAsk = false; });
  let acctOpen = $state(false);
  let settingsOpen = $state(false);
  /** Sağlayıcısız üye: oda ekranında kurulum açık mı. */
  let providerSetup = $state(false);
  /** Yönetim ekranı (yalnızca admin, p0.19). */
  let adminOpen = $state(false);
  /** "Hazır mısınız?" 3-2-1 (`sync://countdown`; 0 = bitti/iptal). */
  let countdown = $state(0);
  /** Odanın ortak modu (p0.13); tek başına modda yereldir. */
  let roomView = $state<RoomView>("browse");
  const VIEWS: { id: RoomView; label: string; icon: "search" | "film" | "music" }[] = [
    { id: "browse", label: "Keşfet", icon: "search" },
    { id: "watch", label: "Sinema", icon: "film" },
    { id: "listen", label: "Müzik", icon: "music" },
  ];
  /** Her sahne yalnızca kendi araçlarını gösterir (sağ ray, çekmece, kontrol şeridi). */
  const SCENE_TOOLS: Record<RoomView, Drawer[]> = {
    browse: ["people", "feed", "settings"],
    watch: ["add", "tracks", "discover", "people", "feed", "settings"],
    listen: ["queue", "people", "feed", "settings"],
  };
  const allowed = (d: Drawer, v: RoomView = roomView) => SCENE_TOOLS[v].includes(d);
  /** Çekmeceyi yalnızca o sahnede yeri varsa açar. */
  function show(d: Drawer) {
    if (allowed(d)) drawer = d;
  }
  /** "Ne izlesek?" penceresi (Sinema'nın içinde, pencere genişliğinde; açıkken video gizli). */
  let nightOpen = $state(false);
  /** Keşfet'teki "Ne izlesek?" kısayolu: Sinema'ya geçilince açılır. */
  let nightAfterView = false;
  function openNight() {
    if (!room || fullscreen) return;
    if (roomView !== "watch") applyView("watch");
    drawer = null;
    nightOpen = true;
  }
  $effect(() => {
    if (!room || roomView !== "watch") nightOpen = false;
  });
  /** Sahneye geçer: başka sahnenin çekmecesi kapanır, sahnenin varsayılanı açılır
   *  (Müzik → kuyruk; boş Sinema kendi lobisini çizer). */
  function applyView(v: RoomView) {
    roomView = v;
    if (v !== "watch") { nightOpen = false; fdOpen = false; }
    else if (nightAfterView) { nightAfterView = false; queueMicrotask(openNight); }
    if (drawer && allowed(drawer, v)) return;
    drawer = v === "listen" ? "queue" : null;
  }
  /** Sinema içinde Film & Dizi (mpv gizli; açılan kaynak ya da yeni oda medyası kapatır). */
  let fdOpen = $state(false);
  let fdQuery = $state("");
  function openTitles(q: string) {
    if (roomView !== "watch") applyView("watch");
    nightOpen = false;
    drawer = null;
    fdQuery = q;
    fdOpen = true;
  }
  function pickView(v: RoomView) {
    if (v === roomView) return;
    if (!room) return applyView(v);
    run("Mod", syncSetView(v), true);
  }

  // Sağ ray + çekmece (UI-PLAN §5.3, Yön C). Çekmece açıkken video DRAWER_DIP kadar daralır.
  type Drawer = "add" | "discover" | "queue" | "people" | "feed" | "settings" | "tracks";
  const DRAWER_DIP = 320;
  const DRAWERS: Record<Drawer, { eyebrow: string; title: string }> = {
    add: { eyebrow: "Oda", title: "Video ekle" },
    discover: { eyebrow: "Oda", title: "Ne izlesek?" },
    queue: { eyebrow: "Lounge", title: "Çalma sırası" },
    people: { eyebrow: "Kişiler", title: "Odadakiler" },
    feed: { eyebrow: "Oda", title: "Etkinlik" },
    settings: { eyebrow: "Oda", title: "Oda ayarları" },
    tracks: { eyebrow: "Oynatıcı", title: "Altyazı · ses · kalite" },
  };
  /** Odada Müzik çekmecesi: ortak Lounge kuyruğu ya da kendi çalma listelerin. */
  let musicTab = $state<"lounge" | "lists">("lounge");
  /** Çekmece açıkken iz listesi tazelenir (dosya açılınca izler sonradan gelir). */
  let tracks = $state<Tracks | null>(null);
  const loadTracks = () => playerTracks().then((t) => (tracks = t), () => {});
  $effect(() => {
    if (drawer !== "tracks") return;
    loadTracks();
    const id = setInterval(loadTracks, 1500);
    return () => clearInterval(id);
  });
  let trackTab = $state<TrackKind>("sub");
  /** Altyazı gecikmesi (sn; mpv'nin döndürdüğü), medya değişince sıfırlanır. */
  let subDelay = $state(0);
  const nudgeSub = (d: number) => playerSubDelay(d).then((v) => (subDelay = v), (err) => add(String(err), true));
  function pickTrack(kind: TrackKind, key: string, name: string) {
    const pre = kind === "sub" ? "Altyazı: {name}" : kind === "audio" ? "Ses: {name}" : "Kalite: {name}";
    run(pre.replace("{name}", name), playerSetTrack(kind, key).then(loadTracks), true, () => t(pre, { name }));
  }
  let drawer = $state<Drawer | null>(null);
  function toggle(d: Drawer) {
    if (d === "discover") return nightOpen ? (nightOpen = false) : openNight();
    drawer = drawer === d ? null : d;
  }
  // Kapanırken çekmece içeriği kaybolmasın: sütun daralırken son açılan gösterilir,
  // video da çekmece kayıp gittikten sonra genişler.
  const DRAWER_MS = 320;
  let lastDrawer: Drawer = "add";
  const shown = $derived.by(() => (drawer ? (lastDrawer = drawer) : lastDrawer));
  let drawerLive = $state(false);
  $effect(() => {
    if (drawer) { drawerLive = true; return; }
    const id = setTimeout(() => (drawerLive = false), DRAWER_MS);
    return () => clearTimeout(id);
  });
  /** Sağ raydaki sahne araçları. */
  type Tool = { id: Drawer; icon: "plus" | "spark" | "music" | "activity" | "gear"; title: string; badge?: string };
  const tools = $derived.by((): Tool[] => {
    const all: (Tool | false)[] = [
      { id: "add", icon: "plus", title: "Video ekle" },
      !!room && { id: "discover", icon: "spark", title: "Ne izlesek?" },
      { id: "queue", icon: "music", title: room ? "Lounge · kuyruk" : "Müzik · çalma listeleri", badge: room && fun.queue.length ? String(fun.queue.length) : undefined },
      { id: "feed", icon: "activity", title: "Etkinlik" },
      !!room && { id: "settings", icon: "gear", title: "Oda ayarları" },
    ];
    return all.filter((t): t is Tool => !!t && allowed(t.id));
  });
  /** Üstteki mod düğmelerinin altında kayan gösterge. */
  let modeBtns: HTMLButtonElement[] = $state([]);
  let thumb = $state({ x: 0, w: 0 });
  $effect(() => {
    const i = VIEWS.findIndex((v) => v.id === roomView);
    const b = modeBtns[i];
    if (!b) return;
    const place = () => (thumb = { x: b.offsetLeft, w: b.offsetWidth });
    place();
    const ro = new ResizeObserver(place);
    ro.observe(b);
    return () => ro.disconnect();
  });

  /** Bulunan yeni sürüm; `updating`: indirme yüzdesi (null = kurulum başlamadı). */
  let update = $state<UpdateInfo | null>(null);
  let updating = $state<number | null>(null);

  /** Açılışta (henüz odada değilken) yeni sürüm hemen kurulur ve uygulama yeniden
   *  açılır; sonraki denetimler odayı bölmemek için yalnızca "Güncelle" düğmesi gösterir. */
  async function checkUpdate(startup: boolean) {
    let u: UpdateInfo | null;
    try { u = await updateCheck(); } catch (e) { add(`Güncelleme denetimi: ${String(e)}`, false, () => t("Güncelleme denetimi: {e}", { e: tx(String(e)) })); return; }
    if (!u || update?.version === u.version) return;
    update = u;
    if (u.portable) note("KONİ {v} çıktı; taşınabilir sürümü elle güncelle", { v: u.version });
    else if (startup && !room && !connecting) installUpdate();
  }
  async function installUpdate() {
    if (updating !== null) return;
    updating = 0;
    note("KONİ {v} kuruluyor; uygulama yeniden açılacak", { v: update?.version ?? "" });
    try { await updateInstall(); } catch (e) { updating = null; add(String(e), true); }
  }

  async function refreshAccount() {
    try { account = await accountGet(serverUrl); } catch { account = null; }
    accountChecked = true;
    if (account) startSocial(serverUrl);
  }
  /** Giriş ekranında sunucu değişince: o sunucuda kayıtlı oturum varsa doğrudan girilir. */
  function setServer(input: string) {
    const url = normServer(input) || DEFAULT_SERVER;
    if (url === serverUrl) return;
    serverUrl = url;
    localStorage.setItem("koni.server", url);
    notice = null;
    refreshAccount();
  }
  /** Üye davet kodu (koni://invite/KOD ya da elle); kayıt formunu doldurur. */
  let signupCode = $state("");
  /** Yeni kayıt: ana ekranda profil fotoğrafı seçimi açılır. */
  let welcome = $state(false);
  async function doAuth(mode: "login" | "register", password: string, code: string) {
    if (authBusy) return;
    authBusy = true;
    notice = null;
    try {
      account = mode === "register"
        ? await accountRegister(serverUrl, code, username, password)
        : await accountLogin(serverUrl, username, password);
      note("Giriş yapıldı: {name}", { name: account ?? "" });
      localStorage.setItem("koni.server", serverUrl);
      localStorage.setItem("koni.username", username);
      setGuest(false);
      welcome = mode === "register";
      signupCode = "";
      startSocial(serverUrl);
    } catch (e) {
      add(String(e), true);
    } finally {
      authBusy = false;
    }
  }
  function setGuest(on: boolean) {
    guest = on;
    localStorage.setItem("koni.guest", on ? "1" : "0");
  }
  async function logout() {
    if (account) await run("Çıkış yapıldı", accountLogout(serverUrl));
    account = null;
    stopSocial();
    setGuest(false);
    settingsOpen = false;
  }

  /** Hangi ekran: giriş → ana ekran → oda / tek başına. */
  const view = $derived(
    room ? "room" : solo ? "solo" : account || (guest && username.trim()) ? "home" : accountChecked ? "login" : "boot",
  );
  /** koni://join/KOD ile gelen, henüz işlenmemiş davet kodu. */
  let pendingInvite = $state<string | null>(null);
  function onDeepLink(urls: string[] | null) {
    const link = urls?.find((u) => /^koni:\/\/invite\//i.test(u));
    const member = link?.match(/^koni:\/\/invite\/([^\s/?#]+)/i)?.[1];
    if (member) {
      if (account) note("Bu davet yeni üyeler için; sen zaten giriş yaptın");
      else {
        // Başka bir sunucunun daveti: koni://invite/KOD?s=sunucu
        const s = link?.match(/[?&]s=([^&#\s]+)/)?.[1];
        if (s) setServer(decodeURIComponent(s));
        signupCode = decodeURIComponent(member);
      }
    }
    const code = urls?.map((u) => (u.match(/^koni:\/\/join\//i) ? parseInvite(u) : "")).find(Boolean);
    if (code) pendingInvite = code;
  }
  // Giriş yapılmamışsa kod bekler; ana ekranda doğrudan katılır; odadayken önce çıkılır.
  $effect(() => {
    if (!pendingInvite || connecting) return;
    if (view === "home") {
      const code = pendingInvite;
      pendingInvite = null;
      settingsOpen = false;
      connect(true, code);
    } else if (view === "room") {
      run("Ayrıl", syncLeave(), true);
    } else if (view === "solo") {
      solo = false;
    }
  });
  /** Bu odadan çağrılan arkadaşlar (handle → true); oda değişince sıfırlanır. */
  let called = $state<Record<string, boolean>>({});
  $effect(() => { void room?.invite; called = {}; });
  /** Odada olmayan karşılıklı arkadaşlar: çevrimiçi olanlar önce. */
  const callable = $derived.by(() => {
    const inRoom = new Set(Object.values(members).map((m) => m.name.toLocaleLowerCase("tr-TR")));
    return (social.presence?.friends ?? [])
      .filter((f) => f.mutual && !inRoom.has(f.name.replace(/#\d+$/, "").toLocaleLowerCase("tr-TR")))
      .sort((a, b) => Number(b.online) - Number(a.online) || Number(!b.room) - Number(!a.room));
  });
  /** Odadaki üyenin profil fotoğrafı (kendin ya da arkadaşın; yoksa baş harf kalır). */
  function faceOf(name: string): string | undefined {
    const n = name.toLocaleLowerCase("tr-TR");
    if (social.profile && social.profile.name.toLocaleLowerCase("tr-TR") === n) return pic(social.profile.avatar);
    const f = social.presence?.friends.find((f) => f.name.replace(/#\d+$/, "").toLocaleLowerCase("tr-TR") === n);
    return pic(f?.avatar);
  }
  const callFriend = (user: string) => run("Çağır", syncFun({ t: "call", user }), true);
  // Video yüzeyi yalnızca oda ve tek başına modda görünür.
  $effect(() => {
    // Müzik sahnesi kendi ekranını çizer: mpv penceresi yalnızca Sinema'da görünür.
    const mode = (view === "room" || view === "solo") && roomView === "watch" && !nightOpen && !leaveAsk && !sceneOwned ? "cockpit" : "hidden";
    setLayout({ ...L, mode, rightColDip: L.rightColDip + (drawer || drawerLive ? DRAWER_DIP : 0) }).catch((e) => add(`Düzen: ${String(e)}`, true, () => t("Düzen: {e}", { e: tx(String(e)) })));
  });

  const inRoom = $derived(room !== null);
  /** Boş Sinema: lobi (oda medyası yoksa ya da açılamadıysa). */
  const cinemaEmpty = $derived(
    roomView === "watch" && st?.duration == null && !opening && (room ? !roomMedia || roomMedia.status === "error" : !soloMedia),
  );
  /** Oynayan medya (odada oda medyası). */
  const nowMedia = $derived(room ? roomMedia : soloMedia);
  /** "Başka video aç" (film bitti ekranından): medya yüklüyken de lobi. */
  let lobbyOpen = $state(false);
  /** Film bitti ekranı: bu medya için kapatıldıysa yeniden çıkmaz. */
  let endClosed = $state("");
  const endOn = $derived(
    roomView === "watch" && !!st?.eofReached && st.duration != null && !lobbyOpen && endClosed !== (nowMedia?.src ?? ""),
  );
  const lobbyOn = $derived(cinemaEmpty || (roomView === "watch" && lobbyOpen));
  /** Sinema sahnesini arayüz çiziyor (lobi, Film & Dizi, film bitti): mpv ve alt şerit gizli. */
  const sceneOwned = $derived(roomView === "watch" && (fdOpen || lobbyOn || endOn));
  // Yeni medya gelince lobi/bitti kapanır.
  let sceneSrc = "";
  $effect(() => {
    const k = nowMedia?.src ?? "";
    if (k === sceneSrc) return;
    sceneSrc = k;
    lobbyOpen = false;
    lobbyOpen = false;
    fdOpen = false;
    fun.subs = { lang: "", busy: false, items: [], error: "" };
  });
  /** Bu medyadaki mola sayısı (film bitti ekranı). */
  let pauses = $state(0);
  let pauseKey = "";
  let lastMode = "";
  $effect(() => {
    const k = nowMedia?.src ?? "";
    const mode = room ? (sync?.roomMode ?? "") : st?.duration == null ? "" : st.paused ? "paused" : "playing";
    const eof = !!st?.eofReached;
    untrack(() => {
      if (k !== pauseKey) { pauseKey = k; pauses = 0; lastMode = mode; return; }
      if (mode === lastMode) return;
      if (lastMode === "playing" && mode === "paused" && !eof) pauses++;
      lastMode = mode;
    });
  });
  function closeEnd(then?: () => void) {
    endClosed = nowMedia?.src ?? "";
    then?.();
  }
  const roomRunning = $derived(sync?.roomMode === "playing" || sync?.roomMode === "countdown");
  const amHost = $derived(room !== null && sync?.me === room.host);
  /** Host ya da yardımcı host: host_only'de kontrol, "olmadan devam et" teklifleri. */
  const amControl = $derived(amHost || (sync?.me != null && members[sync.me]?.cohost === true));
  const detached = $derived(sync?.localState === "Detached");
  /** Desteyi açma, çarkı çevirme, odaya altyazı: politika host_only ise yalnızca hostlar. */
  const funControl = $derived(room?.policy !== "host_only" || amControl);
  // Parça sonu: Santral kuyruğun başını açar (PROTOCOL §3.11). Herkes bildirir, ilki sayılır.
  // Yalnızca parça yüklendikten (ready) sonra eof bir kez false görüldüyse bildirilir: yeni parça
  // yüklenirken (yt-dlp birkaç sn sürer) mpv eski dosyanın sonunda durur ve eof hâlâ
  // true'dur; bu eski eof yeni parça için gönderilirse Santral onu da geçer, liste
  // zincirleme boşalırdı.
  let eofSent = "";
  let eofArmedSrc = "";
  $effect(() => {
    const src = roomMedia?.src ?? "";
    if (!st?.eofReached) {
      if (roomMedia?.status === "ready") {
        eofArmedSrc = src;
        eofSent = "";
      }
      return;
    }
    // Odadan ayrı (detached) çalanın sonu odanın sonu değil.
    if (!room || !src || eofArmedSrc !== src || eofSent === src || untrack(() => detached)) return;
    eofSent = src;
    syncFun({ t: "track_end", src }).catch(() => {});
  });
  // Oda medyası bende açılamadı (link ölü, yt-dlp hatası): Santral'a bildirilir. Ekleyen
  // ya da odanın yarısından fazlası açamıyorsa herkes için geçilir; yoksa oda bensiz
  // devam eder, sıradaki şarkıda yeniden katılırım (PROTOCOL §3.11).
  let failSent = "";
  $effect(() => {
    const m = roomMedia;
    if (!room || m?.status !== "error" || failSent === m.src) return;
    failSent = m.src;
    syncFun({ t: "track_end", src: m.src, failed: true }).catch(() => {});
  });
  $effect(() => {
    if (roomMedia?.status !== "error") failSent = "";
  });
  // Kaynağı yok (anahtar/eklenti eklememiş): hiç bağlanmaya çalışmaz, oda bensiz devam
  // eder. "Tekrar dene" odaya geri döndürürse ve hâlâ kaynak yoksa yine bildirilir.
  $effect(() => {
    const m = roomMedia;
    if (!room || m?.status !== "no_provider" || detached) return;
    syncFun({ t: "track_end", src: m.src, failed: true }).catch(() => {});
  });
  $effect(() => {
    if (roomMedia?.status !== "no_provider") providerSetup = false;
  });

  // Müzik modu: görüntülü parçada da video gizlenir, görselleştirici açılır (lounge.rs).
  $effect(() => {
    playerMusicView((view === "room" || view === "solo") && roomView === "listen").catch(() => {});
  });

  // Müzik sahnesi: çalan parça, kapak, sıradaki.
  const musicMedia = $derived(room ? roomMedia : soloMedia);
  const musicTitle = $derived(musicMedia?.title || (musicMedia?.src ? musicMedia.src.split(/[\\/]/).pop() ?? "" : ""));
  const musicSrc = $derived(musicMedia?.src ?? "");
  /** yt-dlp kapağı (YouTube dışı linkler); dosya yüklenince bir kez sorulur. */
  let fetchedCover = $state<string | null>(null);
  let coverSrc = "";
  let coverAsked = false;
  $effect(() => {
    const s = musicSrc;
    const loaded = st?.duration != null;
    if (s !== coverSrc) {
      coverSrc = s;
      coverAsked = false;
      fetchedCover = null;
    }
    if (!s || !loaded || coverAsked || artFor(s)) return;
    coverAsked = true;
    playerCover().then((c) => { if (coverSrc === s) fetchedCover = c; }).catch(() => {});
  });
  const musicCover = $derived(artFor(musicSrc) ?? fetchedCover);
  const upNext = $derived(
    room ? (fun.queue[0] ? splitTitle(fun.queue[0].media.title).song : null) : (listPlay.queue[listPlay.pos + 1]?.title ?? null),
  );
  const canNext = $derived(
    room ? funControl && fun.queue.length > 0 : listPlay.id !== "" && listPlay.pos < listPlay.queue.length - 1,
  );
  /** Odada: kuyruğun başına geç. Çalan şarkı da gider: aynı anda iki basış (iki kişi ya
   *  da çift tık) Santral'da tek atlama sayılır. */
  function musicNext() {
    if (room) {
      if (fun.queue.length > 0) run("Sıradaki", syncFun({ t: "queue_next", src: roomMedia?.src ?? "" }), true);
    } else {
      stepList(1);
    }
  }
  /** Spotify gibi: 3 sn'den sonra başa sarar, başta önceki şarkıya geçer (yalnızca kendi listende). */
  function musicPrev() {
    if (!room && listPlay.id && listPlay.pos > 0 && (st?.timePos ?? 0) < 3) stepList(-1);
    else seekTo(0);
  }

  /** Çalan şarkı (link ise): kalp ve "Çalanı ekle". */
  const musicNow = $derived.by((): Track | null => {
    const m = room ? roomMedia : soloMedia;
    if (!m || m.kind !== "url" || !/^https?:\/\//i.test(m.src)) return null;
    return { src: m.src, title: m.title || m.src, dur: st?.duration ?? null };
  });
  // Çalma listesi odaya girip çıkınca durur (tek başına yerel sıra ≠ Lounge besleme).
  $effect(() => {
    void inRoom;
    untrack(stopList);
  });
  // Tek başına: listede şarkı değişince (`gen`) açılır; açılamazsa sıradakine geçilir.
  $effect(() => {
    const gen = listPlay.gen;
    untrack(() => {
      const tk = nowTrack();
      if (!tk || room) return;
      if (roomView !== "listen") applyView("listen");
      playerPause().catch(() => {});
      soloMedia = { src: tk.src, kind: "url", title: tk.title };
      soloTitle = tk.title;
      playerLoad(tk.src)
        .then((title) => {
          if (title === null || listPlay.gen !== gen) return;
          soloMedia = { src: tk.src, kind: "url", title };
          soloTitle = title;
          return playerPlay();
        })
        .catch((e) => {
          // Çözülürken başka şarkıya geçildi: o şarkı zaten açılıyor.
          if (isSuperseded(e) || listPlay.gen !== gen) return;
          add(`${tk.title}: ${String(e)}`, true, () => `${tk.title}: ${tx(String(e))}`);
          stepList(1);
        });
    });
  });
  /** Parça bu kadar çaldıktan sonra arka plan işleri (ön çözüm, dalga formu) başlar. */
  const WARM_SEC = 6;
  // Sıradaki şarkı çalan bitmeden çözülür (yt-dlp birkaç sn): geçişte beklenmez. Çalan
  // parça oturduktan sonra; ilk tamponuyla yarışmasın.
  let prefetched = "";
  $effect(() => {
    const going = st?.duration != null && !st.paused && !st.eofReached && !st.pausedForCache
      && (st.timePos ?? 0) >= WARM_SEC && (room ? roomMedia?.status === "ready" : !!soloMedia);
    const next = room
      ? (roomView === "listen" ? fun.queue[0]?.media.src : undefined)
      : listPlay.id ? listPlay.queue[listPlay.pos + 1]?.src : undefined;
    if (!going || !next || next === prefetched || !isUrl(next)) return;
    prefetched = next;
    mediaPrefetch(next).catch(() => {});
  });
  // Tek başına: şarkı bitince sıradaki.
  let eofArmed = false;
  $effect(() => {
    const eof = st?.eofReached ?? false;
    const t = nowTrack();
    if (room || !t || soloMedia?.src !== t.src) return;
    if (!eof) {
      eofArmed = true;
      return;
    }
    if (!eofArmed) return;
    eofArmed = false;
    untrack(() => stepList(1));
  });
  // Odada listeden Lounge'a eklenenler: liste yeniden başlatılınca / durdurulunca
  // yalnızca bunlar kuyruktan çıkar (elle eklediğin şarkılar kalır).
  let fedSrcs = new Set<string>();
  function unfeed() {
    const me = sync?.me;
    if (me) {
      for (const it of fun.queue.filter((q) => q.by === me && fedSrcs.has(q.media.src))) {
        syncFun({ t: "queue_remove", queueId: it.id }).catch(() => {});
      }
    }
    fedSrcs = new Set();
  }
  /** Listelerim'de "Listeyi durdur": odada henüz çalmamış liste şarkıları da kuyruktan çıkar. */
  function stopPlaylist() {
    if (room) unfeed();
    stopList();
  }
  // Odada: Çalma listesi başlatıldığında / karıştırıldığında önceki liste beslemesi temizlenir ve Lounge'a geçilir.
  $effect(() => {
    const gen = listPlay.gen;
    if (!room || !listPlay.id) return;
    untrack(() => {
      if (roomView !== "listen") applyView("listen");
      unfeed();
      feedTick++;
    });
  });
  // Odada: Lounge'da bu listeden şarkılar sırayla eklenir ve tampon 5 şarkıda tutulur.
  let feeding = false;
  let feedTick = $state(0);
  $effect(() => {
    void feedTick;
    const q = fun.queue;
    const me = sync?.me;
    if (!room || !listPlay.id) return;
    untrack(() => {
      if (feeding) return;
      if (listPlay.fed >= listPlay.queue.length) {
        return;
      }
      const mine = q.filter((it) => it.by === me);
      if (mine.length >= 10 || mine.filter((it) => fedSrcs.has(it.media.src)).length >= 5) return;
      const tk = listPlay.queue[listPlay.fed++];
      // Zaten kuyrukta ya da şu an çalıyor: Santral reddederdi, sessizce atlanır.
      if (q.some((it) => it.media.src === tk.src) || (roomMedia?.src === tk.src && sync?.roomMode !== "paused")) {
        setTimeout(() => feedTick++, 0);
        return;
      }
      // Sıradaki denmez: oda boşsa / çalan bitmişse Santral ilk şarkıyı kendisi açar;
      // başkasının şarkısı çalıyorsa liste sırası gelince başlar (eskiden çalan geçiliyordu).
      feeding = true;
      fedSrcs.add(tk.src);
      queueAdd(tk.src, tk.title)
        .catch((e) => {
          fedSrcs.delete(tk.src);
          add(String(e), true);
        })
        .finally(() => {
          feeding = false;
          setTimeout(() => feedTick++, 250);
        });
    });
  });

  /** Etkinliğe satır ekler. `msg` Türkçe kalır (sorun bildirimi); `show` ekranda
   *  dile göre çevirir (varsayılan: sunucu/Rust metni için tx). */
  function add(msg: string, err = false, show: () => string = () => tx(msg)) {
    const time = new Date().toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });
    log = [{ t: time, msg, err, show }, ...log].slice(0, 200);
    if (err) notice = show;
  }
  /** Arayüzün kendi metni: t() anahtarı + yer tutucular. */
  function note(key: string, vars?: Record<string, string | number>, err = false) {
    const msg = vars ? key.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m)) : key;
    add(msg, err, () => t(key, vars));
  }
  /** "Sorun bildir": etkinlik logu eskiden yeniye gider. */
  const reportBug = (note: string) =>
    bugReport(serverUrl, account ?? username, note, [...log].reverse().map((l) => `${l.t}${l.err ? " !" : ""} ${l.msg}`));
  /** `label` Türkçe; `show` ekrandaki karşılığı (varsayılan t(label)). */
  async function run(label: string, p: Promise<unknown>, quiet = false, show: () => string = () => t(label)) {
    try { await p; if (!quiet) add(label, false, show); } catch (e) { add(`${label}: ${String(e)}`, true, () => `${show()}: ${tx(String(e))}`); }
  }

  function onFunMsg(m: FunMsg) {
    if (m.t === "called") {
      called = { ...called, [m.user]: true };
      note("{name} çağrıldı", { name: m.user.replace(/#\d+$/, "") });
      return;
    }
    if (m.t === "view") {
      applyView(m.view);
      return;
    }
    if (m.t === "subtitle") {
      run(`Altyazı (${m.by}): ${m.name}`, playerAddSubtitleText(m.name, m.text), false, () => t("Altyazı ({by}): {name}", { by: m.by, name: m.name }));
      return;
    }
    // yeni Ne izlesek? turu, deste ya da eşleşme: herkeste pencere açılsın
    const fresh = (m.t === "night" && !m.closed && fun.night?.id !== m.id) || (m.t === "deck" && fun.deck?.id !== m.deckId) || m.t === "match";
    if (m.t === "night" && m.closed) nightOpen = false;
    const r = onFun(m);
    if (r) add(r.text, r.err);
    if (fresh) openNight();
  }
  /** Ne izlesek? eşleşmesi: Sinema'daki Film & Dizi'de aranır. */
  function playMatch(mv: Movie) {
    openTitles(mv.year ? `${mv.title} ${mv.year}` : mv.title);
  }

  function fmt(s: number | null | undefined): string {
    if (s == null || !isFinite(s) || s < 0) return "--:--";
    const t = Math.floor(s), h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), sec = t % 60;
    const p = (n: number) => String(n).padStart(2, "0");
    return h > 0 ? `${h}:${p(m)}:${p(sec)}` : `${p(m)}:${p(sec)}`;
  }
  const ms = (v: number | null | undefined) => (v == null ? "–" : `${v > 0 ? "+" : ""}${Math.round(v)} ms`);
  function dlText(d: DlStatus | undefined): string {
    if (!d) return t("⬇ bekleniyor");
    if (d.state === "ready") return t("✓ indirdi");
    if (d.state === "failed") return t("✗ indiremedi");
    if (d.state === "queued") return t("⬇ hazırlanıyor");
    const speed = d.speed > 0 ? ` · ${(d.speed / 1e6).toFixed(1)} MB/s` : "";
    const eta = d.eta > 0 ? ` · ${fmt(d.eta)}` : "";
    return `⬇ %${Math.floor(d.pct)}${speed}${eta}`;
  }
  const isUrl = (s: string) => /^https?:\/\//i.test(s.trim());

  async function setFs(on: boolean) {
    fullscreen = on;
    await run(on ? "Tam ekran" : "Tam ekrandan çık", setFullscreen(on));
  }

  function togglePause() {
    if (!inRoom) return run("Oynat/Duraklat", playerTogglePause());
    return run("Oynat/Duraklat", roomRunning ? syncPause() : syncPlay(), true);
  }
  function seekTo(v: number) {
    if (inRoom) return run("Atla", syncSeek(v), true);
    return run("Atla " + fmt(v), playerSeek(v), false, () => t("Atla {pos}", { pos: fmt(v) }));
  }

  async function open(path?: string) {
    const s = (path ?? src).trim().replace(/^"(.*)"$/, "$1");
    if (!s || opening) return;
    if (SUB_EXT.test(s) && !/^https?:\/\//i.test(s)) {
      return run("Altyazı", playerAddSubtitle(s).then((n) => note("Altyazı: {name}", { name: n })), true);
    }
    opening = true;
    try {
      const title = await playerLoad(s);
      // null: yt-dlp siteyi tanımıyor; Keşfet'te sayfada oynatılır (tarayıcıda senkron).
      if (title === null) {
        if (/^https?:\/\//i.test(s)) {
          pageWait = s;
          drawer = null;
          if (roomView !== "browse") applyView("browse");
        }
      } else if (!inRoom) {
        note("Açıldı: {title}", { title });
        soloMedia = { src: s, kind: /^https?:\/\//i.test(s) ? "url" : "file", title };
        soloTitle = title;
        if (roomView === "browse") applyView("watch");
      }
    } catch (e) {
      if (!isSuperseded(e)) add(String(e), true);
    } finally {
      opening = false;
    }
  }

  /** Keşfet'in KONİ'de izle'si: yt-dlp ile aç; `null`: sayfada oynatılacak. */
  async function probe(url: string): Promise<string | null> {
    if (opening) throw new Error(t("başka bir video açılıyor"));
    opening = true;
    try {
      const title = await playerLoad(url);
      if (title !== null) {
        if (!inRoom) {
          soloMedia = { src: url, kind: "url", title };
          soloTitle = title;
          note("Açıldı: {title}", { title });
        }
        setTimeout(() => { if (roomView === "browse") applyView("watch"); }, 800);
      }
      return title;
    } finally {
      opening = false;
    }
  }

  /** Film & Dizi kaynağı: odadaysa torrent odaya gider, herkes kendi Real-Debrid'iyle çözer. */
  async function openDebrid(s: Source, title: string, alts: Source[] = []) {
    if (opening) return;
    opening = true;
    if (roomMedia?.status === "error") roomMedia = null;
    const candidates = [s, ...alts.filter((a) => (a.hash && a.hash !== s.hash) || (a.url && a.url !== s.url))].slice(0, 10);
    let lastErr = "";

    try {
      for (let idx = 0; idx < candidates.length; idx++) {
        const cur = candidates[idx];
        if (idx > 0) {
          note("Kaynak {i} açılmadı, alternatif deneniyor ({n}/{total})…", { i: idx, n: idx + 1, total: candidates.length });
        } else {
          note("Bağlantı hazırlanıyor…");
        }
        try {
          const got = await debridOpen(cur, title);
          lobbyOpen = false;
    fdOpen = false;
    fun.subs = { lang: "", busy: false, items: [], error: "" };
          if (!inRoom) {
            note("Açıldı: {title}", { title: got });
            soloMedia = { src: cur.hash ?? cur.url ?? "", kind: cur.hash ? "debrid" : "url", title: got, file: cur.file, idx: cur.idx ?? undefined };
            soloTitle = got;
            if (roomView === "browse") applyView("watch");
          } else {
            note("Odaya açıldı: {title}", { title: got });
            if (roomView === "browse") applyView("watch");
          }
          return;
        } catch (e) {
          lastErr = String(e);
          const msg = String(e).toLowerCase();
          // Rust hatası dile göre Türkçe ya da İngilizce gelir.
          const isFatal = msg.includes("anahtar") || msg.includes("premium") || /\bkey\b/.test(msg) || msg.includes("addon'unu ekle") || msg.includes("add your own addon");
          if (!isFatal) {
            continue;
          }
          break;
        }
      }
      if (lastErr) add(lastErr, true);
    } finally {
      opening = false;
    }
  }

  function onSniffer(e: SnifferEvent) {
    sniffing = e.status === "open" ? e.page : null;
  }

  function toggleMute() {
    if (volume > 0) { lastVolume = volume; setVolume(0); }
    else setVolume(lastVolume || 100);
  }
  function setVolume(v: number) {
    volume = v;
    localStorage.setItem("koni.volume", String(v));
    playerSetVolume(v).catch((e) => add(`Ses: ${String(e)}`, true, () => t("Ses: {e}", { e: tx(String(e)) })));
  }

  /** Telefon kumandası (p0.24): masaüstündeki düğmeye basılmış gibi; odada senkron korunur. */
  function onRemoteCmd(m: RemoteOp) {
    const dur = st?.duration;
    if (m.op === "toggle" && (dur != null || nowMedia)) togglePause();
    else if (m.op === "seek" && dur != null) seekTo(Math.max(0, Math.min(dur - 1, (st?.timePos ?? 0) + m.by)));
    else if (m.op === "vol") setVolume(Math.max(0, Math.min(130, volume + m.by)));
    setTimeout(remoteState, m.op === "status" ? 0 : 500);
  }
  function remoteState() {
    const dur = st?.duration ?? null;
    const title = nowMedia?.title || (dur != null ? soloTitle || t("Video") : "");
    lobbyRemoteState({ title, paused: room ? !roomRunning : (st?.paused ?? true), pos: st?.timePos ?? null, dur, vol: volume }).catch(() => {});
  }

  function continueWithout(id: string) {
    delete offers[id];
    run("Onsuz devam", syncContinueWithout(id), true);
  }

  function closeVideo() {
    lobbyOpen = false;
    fdOpen = false;
    opening = false;
    if (!room) {
      if (soloMedia) note("Video kapatıldı: {title}", { title: soloMedia.title || soloMedia.src });
      soloMedia = null;
      soloTitle = "";
    } else {
      if (roomMedia) note("Video kapatıldı: {title}", { title: roomMedia.title || roomMedia.src });
      lobbyOpen = false;
      if (st) st = { ...st, duration: null, timePos: null, eofReached: false };
      roomMedia = null;
    }
    if (st) st = { ...st, duration: null, timePos: null, eofReached: false };
    run("Kapat", syncClose(), true);
  }

  function onMedia(m: MediaEvent | null) {
    if (!m) {
      // Oda boşaldı: biri çalanı kapattı.
      if (roomMedia) note("Video kapatıldı: {title}", { title: roomMedia.title || roomMedia.src });
      lobbyOpen = false;
      if (st) st = { ...st, duration: null, timePos: null, eofReached: false };
      roomMedia = null;
      return;
    }
    roomMedia = m;
    if (m.status === "ready" && drawer === "add") drawer = null;
    if (m.kind === "download" && m.status === "downloading") show("people");
    if (m.kind === "download" && m.task !== dlTask) {
      dlTask = m.task ?? null;
      downloads = {};
    }
    const name = m.title || m.src;
    if (m.status === "resolving") note("Çözülüyor: {name}", { name });
    else if (m.status === "downloading") note("İndiriliyor: {name}", { name });
    else if (m.status === "ready") note("Yüklendi: {name}", { name });
    else if (m.status === "need_file") note("Oda dosyası: {name}. Kendi kopyanı Aç ile yükle.", { name });
    else if (m.status === "no_provider") note("{name}: izlemek için bir sağlayıcı ekle. Oda seni beklemeden devam ediyor.", { name }, true);
    else add(`${name}: ${m.error}`, true, () => `${name}: ${tx(m.error ?? "")}`);
  }

  /** Sinema lobisinden "Devam": medyayı bu odada / tek başına açar (konum ResumeBar'da). */
  function resumeHere(r: ResumeEntry) {
    if (r.kind === "file") return note("Dosyayı pencereye bırak: {title}", { title: r.title }, true);
    if (r.kind === "debrid") return openDebrid({ addon: "", name: "", title: "", hash: r.src, file: r.file ?? "", idx: r.idx ?? null }, r.title);
    open(r.src);
  }
  /** Yarım kalan filmi aynı oda adıyla yeniden kurup medyayı açar. */
  function resumeMovie(r: ResumeEntry) {
    pendingResume = r;
    pendingCalls = r.with ?? [];
    roomName = r.room || r.title;
    connect(false);
  }
  /** Ana ekrandan: oda kur (seçilen sahneyle), istenirse arkadaşları çağır. */
  function createRoom(mode: RoomView | "night" | null, calls: string[] = []) {
    newMode = mode;
    pendingCalls = calls;
    connect(false);
  }
  /** Oda kurulunca bekleyen sahneyi ve çağrıları uygular. */
  function afterCreate() {
    const mode = newMode, calls = pendingCalls;
    newMode = null;
    pendingCalls = [];
    if (mode === "night") { nightAfterView = true; pickView("watch"); }
    else if (mode && mode !== "browse") pickView(mode);
    const friends = social.presence?.friends ?? [];
    for (const c of calls) {
      const bare = c.replace(/#\d+$/, "").toLocaleLowerCase("tr-TR");
      const f = friends.find((f) => f.mutual && f.name.replace(/#\d+$/, "").toLocaleLowerCase("tr-TR") === bare);
      if (f) callFriend(f.name);
    }
  }

  async function connect(join: boolean, code?: string) {
    if (!account && !username.trim()) return note("Kullanıcı adı gerekli", undefined, true);
    if (code) inviteIn = code;
    tryingRecent = join ? recent.find((r) => r.invite === inviteIn.trim()) ?? null : null;
    connecting = true;
    notice = null;
    try {
      await syncConnect(serverUrl, username, join ? null : (roomName.trim() || "Oda"), join ? inviteIn.trim() : null);
      localStorage.setItem("koni.server", serverUrl);
      localStorage.setItem("koni.username", username);
    } catch (e) {
      connecting = false;
      add(String(e), true);
    }
  }

  function onServer(m: ServerMsg) {
    switch (m.t) {
      case "joined":
        connecting = false;
        tryingRecent = null;
        room = { name: m.snapshot.name, invite: m.invite, host: m.snapshot.host, policy: m.snapshot.policy ?? "free" };
        members = Object.fromEntries(m.snapshot.members.map((x) => [x.id, { ...x }]));
        offers = {};
        dlTask = m.snapshot.state.media?.task ?? null;
        downloads = Object.fromEntries((m.snapshot.downloads ?? []).map((d) => [d.user, d]));
        solo = false;
        recent = rememberRoom({ invite: m.invite, name: m.snapshot.name, at: Date.now(), members: m.snapshot.members.length });
        if (m.resumed) {
          note("Odaya geri dönüldü");
        } else {
          note("Odaya girildi: {name}", { name: m.snapshot.name });
          drawer = null;
          roomMedia = null;
          const pr = pendingResume;
          pendingResume = null;
          if (pr && pr.kind === "file") note("Dosyayı Aç ile yükle: {title}", { title: pr.title });
          else if (pr && pr.kind === "debrid") {
            openDebrid({ addon: "", name: "", title: "", hash: pr.src, file: pr.file ?? "", idx: pr.idx ?? null }, pr.title);
          } else if (pr) open(pr.src);
        }
        // eski Santral (p0.12 ve öncesi) modu bilmez: Sinema
        applyView(m.snapshot.view || "watch");
        if (!m.resumed && m.snapshot.members.length === 1) afterCreate();
        else { newMode = null; pendingCalls = []; }
        break;
      case "dl_status":
        if (m.taskId === dlTask) downloads[m.user] = m;
        break;
      case "member_joined":
        members[m.member.id] = { ...m.member };
        break;
      case "member_left":
        delete members[m.id];
        delete offers[m.id];
        if (m.host && room) {
          room.host = m.host;
          for (const x of Object.values(members)) {
            x.host = x.id === m.host;
            if (x.host) x.cohost = false;
          }
        }
        break;
      case "member_away":
        if (members[m.id]) members[m.id].away = true;
        break;
      case "member_back":
        if (members[m.id]) members[m.id].away = false;
        break;
      case "member_detached":
        if (members[m.id]) members[m.id].detached = m.detached;
        if (m.detached) delete offers[m.id];
        break;
      case "member_role":
        if (members[m.id]) members[m.id].cohost = m.cohost;
        break;
      case "policy":
        if (room) room.policy = m.policy;
        break;
      case "detach_offer":
        offers[m.user] = m.name;
        drawer = "people";
        note("{name} son 4 dk'da {n} kez takıldı. Beklemeden devam edebilirsin.", { name: m.name, n: m.stalls });
        break;
      case "detached":
        // Açılamayan şarkıda (track_end failed) sebep zaten gösterildi.
        if (roomMedia?.status !== "error" && roomMedia?.status !== "no_provider") note("Oda seni beklemeden devam ediyor. Bağlantın düzelince \"Odayla Senkronla\".", undefined, true);
        break;
      case "member_status":
        if (members[m.id]) Object.assign(members[m.id], { driftMs: m.driftMs, state: m.state, bufferPct: m.bufferPct, rttMs: m.rttMs });
        break;
      case "activity":
        add(m.text);
        break;
      case "denied":
        if (DENIED[m.reason]) note(DENIED[m.reason], undefined, true);
        else note("Reddedildi: {reason}", { reason: m.reason }, true);
        break;
      case "error":
        if (m.code === "bad_invite" && tryingRecent) {
          // oda sunucuda kapanmış: aynı adla yeniden aç (yeni davet kodu)
          recent = forgetRoom(tryingRecent.invite);
          reopen = tryingRecent.name;
          tryingRecent = null;
          note("\"{name}\" kapanmıştı, aynı adla yeniden açılıyor", { name: reopen });
          break;
        }
        if (m.code === "room_ended") { note("Host odayı herkes için kapattı"); break; }
        { const why = m.message || m.code; add(`Sunucu: ${why}`, true, () => t("Sunucu: {why}", { why: tx(why) })); }
        if (m.code === "auth_required") setGuest(false);
        if (m.code === "banned") {
          accountLogout(serverUrl).catch(() => {});
          account = null;
          stopSocial();
        }
        if (m.code === "bad_token") {
          accountLogout(serverUrl).catch(() => {});
          account = null;
        }
        break;
    }
  }

  function typing(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement | null)?.tagName;
    return tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA";
  }

  function onKey(e: KeyboardEvent) {
    if (nightOpen) return; // NightView kendi tuşlarını işler
    const tag = (e.target as HTMLElement | null)?.tagName;
    if (e.code === "Space" && !typing(e) && tag !== "BUTTON") {
      e.preventDefault();
      togglePause();
    } else if (e.key === "Escape") {
      if (drawer && !fullscreen) drawer = null;
      else if (fdOpen && !typing(e)) fdOpen = false;
      else setFs(false);
    } else if ((e.key === "f" || e.key === "F") && !typing(e) && !e.ctrlKey && !e.altKey && (view === "room" || view === "solo") && roomView === "watch") {
      setFs(!fullscreen);
    } else if ((e.key === "g" || e.key === "h") && !typing(e) && !e.ctrlKey && !e.altKey) {
      // altyazı gecikmesi: g = 50 ms erken, h = 50 ms geç
      nudgeSub(e.key === "g" ? -0.05 : 0.05);
    }
  }

  /** Odada/oynatıcıda bir kutuya yazmıyorken yapıştırılan link "Video ekle"ye düşer. */
  function onPaste(e: ClipboardEvent) {
    if ((view !== "room" && view !== "solo") || nightOpen) return;
    const tag = (e.target as HTMLElement | null)?.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA") return;
    const text = e.clipboardData?.getData("text")?.trim();
    if (!text || roomView !== "watch") return;
    src = text;
    if (!lobbyOn || fdOpen) drawer = "add";
  }

  onMount(() => {
    if (volume !== 100) playerSetVolume(volume).catch(() => {});
    if (relayOn()) setRelay(true).catch(() => {});
    refreshAccount();
    checkUpdate(true);
    // Uygulama kapalıyken tıklanan link (argv) ve çalışırken gelen linkler.
    getCurrent().then(onDeepLink).catch(() => {});
    const unDeep = onOpenUrl(onDeepLink);
    const updateTimer = setInterval(() => checkUpdate(false), 6 * 3600_000);
    const id = setInterval(async () => {
      try { st = await playerState(); } catch (e) {
        if (!pollErrLogged) { pollErrLogged = true; add(`player_state: ${String(e)}`, true); }
      }
    }, 250);
    onRemote(onRemoteCmd);
    const unlisten = [
      listen<SyncStatus>("sync://status", (e) => (sync = e.payload)),
      listen<ServerMsg>("sync://server", (e) => onServer(e.payload)),
      listen<string>("sync://local", (e) => add(e.payload)),
      listen<SnifferEvent>("sniffer://state", (e) => onSniffer(e.payload)),
      listen<[number, number | null]>("update://progress", (e) => {
        const [done, total] = e.payload;
        updating = total ? Math.round((done * 100) / total) : 0;
      }),
      listen<MediaEvent | null>("sync://media", (e) => onMedia(e.payload)),
      listen<FunMsg>("sync://fun", (e) => onFunMsg(e.payload)),
      listen<number>("sync://countdown", (e) => (countdown = e.payload)),
      listen<LinkEvent>("sync://link", (e) => {
        if (e.payload.state === "online") {
          reconnecting = null;
        } else {
          if (reconnecting === null) note("Bağlantı koptu, yeniden bağlanılıyor…", undefined, true);
          reconnecting = e.payload.attempt;
        }
      }),
      listen<string | null>("sync://closed", (e) => {
        if (e.payload) add(e.payload, true);
        else if (room) note("Odadan çıkıldı");
        connecting = false;
        room = null;
        members = {};
        sync = null;
        roomMedia = null;
        offers = {};
        reconnecting = null;
        drawer = null;
        dlTask = null;
        downloads = {};
        countdown = 0;
        roomView = "browse";
        resetFun();
        if (reopen) {
          roomName = reopen;
          reopen = null;
          connect(false);
        }
      }),
      // Dosya sürükle-bırak: video/altyazı dosyası açılır.
      getCurrentWebview().onDragDropEvent((e) => {
        if (e.payload.type !== "drop" || e.payload.paths.length === 0) return;
        const path = e.payload.paths[0];
        // Müzik modunda dosya bırakılmaz: sunucuya hiçbir dosya yüklenmez.
        if (roomView === "listen" && room) note("Lounge'a yalnızca link eklenir; dosya paylaşımı kapalı", undefined, true);
        else open(path);
      }),
    ];
    return () => {
      onRemote(null);
      clearInterval(id);
      clearInterval(updateTimer);
      unlisten.forEach((p) => p.then((f) => f()));
      unDeep.then((f) => f()).catch(() => {});
    };
  });

  const pos = $derived(seeking ? seekVal : (st?.timePos ?? 0));
  const playing = $derived(inRoom ? roomRunning : st?.paused === false);
  const seekPct = $derived(st?.duration ? Math.min(100, (pos / st.duration) * 100) : 0);
  // Müzikte seek çubuğunun arkasında dalga formu: parça başına bir kez istenir. İkinci bir
  // oynatıcı sesi baştan sona indirip çözer; şarkının ilk tamponuyla yarışıp onu
  // takıltmasın diye çalma oturduktan sonra.
  let wave = $state<number[] | null>(null);
  let waveKey = "";
  let waveAsked = "";
  $effect(() => {
    const d = st?.duration;
    const key = `${roomMedia?.src ?? soloMedia?.src ?? ""}|${d ? Math.round(d) : ""}`;
    if (key !== waveKey) {
      waveKey = key;
      wave = null;
    }
    if (!d || waveAsked === key || (st?.timePos ?? 0) < WARM_SEC || st?.pausedForCache) return;
    waveAsked = key;
    mediaWaveform().then((w) => { if (waveKey === key) wave = w; }).catch(() => {});
  });
  const MODE: Record<string, string> = { playing: "Birlikte izleniyor", paused: "Duraklatıldı", countdown: "Başlıyor…", hold: "Herkes bekleniyor" };
  const MEDIA_STATE: Record<MediaEvent["status"], string> = { resolving: "Çözülüyor…", downloading: "İndiriliyor…", ready: "Hazır", error: "Hata", need_file: "Dosyanı aç", no_provider: "Sağlayıcı yok" };
  const initial = (name: string) => name.slice(0, 1).toLocaleUpperCase(locale());
  function dotTone(m: Member & { bufferPct?: number }) {
    if (m.away) return "off";
    if (m.bufferPct != null && m.bufferPct < 100) return "warn";
    return "";
  }
  /** Başlıktaki durum pili: önce bağlantı, sonra bekleme, sonra oda modu. */
  const pill = $derived.by(() => {
    if (reconnecting !== null) return { tone: "bad", text: t("Bağlantı koptu"), sub: t("yeniden deneniyor ({n})", { n: reconnecting + 1 }) };
    if (countdown > 0) return { tone: "live", text: t("Hazır mısınız? {n}…", { n: countdown }), sub: "" };
    const w = Object.values(members).find((m) => !m.away && m.bufferPct != null && m.bufferPct < 100);
    if (w) return { tone: "warn", text: t("{name} bekleniyor", { name: w.name }), sub: `%${w.bufferPct}` };
    if (roomMedia?.kind === "download" && roomMedia.status === "downloading") {
      const ready = Object.values(downloads).filter((d) => d.state === "ready").length;
      return { tone: "", text: t("İndiriliyor"), sub: t("{ready}/{total} hazır", { ready, total: Object.keys(members).length }) };
    }
    const mode = sync?.roomMode ?? "";
    return { tone: mode === "playing" || mode === "countdown" ? "live" : mode === "hold" ? "warn" : "", text: t(MODE[mode] ?? "Bağlı"), sub: "" };
  });
  const KIND: Record<MediaEvent["kind"], string> = { url: "Akış", file: "Dosya", download: "İndirme", debrid: "Bağlantı", page: "Tarayıcıda" };
  /** Birlikte izleyenler: bağlı, ayrı izlemiyor, tamponu dolu, sapması yarım saniyeden az. */
  const together = $derived(Object.values(members).filter((m) =>
    !m.away && !m.detached && (m.bufferPct == null || m.bufferPct >= 100) &&
    (m.id === sync?.me ? !detached : m.driftMs == null || Math.abs(m.driftMs) < 500)));
  /** Tamponu dolmayanlar (bekleme bandı). */
  const waiting = $derived(Object.values(members).filter((m) => !m.away && m.id !== sync?.me && m.bufferPct != null && m.bufferPct < 100));
  const holdOn = $derived(!!room && roomView === "watch" && !sceneOwned && sync?.roomMode === "hold");
  /** Sürgüde tamponlanan kısım (oda: önbellek saniyesi). */
  const bufPct = $derived(st?.duration && sync ? Math.min(100, (((st.timePos ?? 0) + sync.cacheAhead) / st.duration) * 100) : 0);
  /** Seçili altyazı adı (şerit çipi); izler medya açılınca bir kez okunur. */
  const subLabel = $derived(tracks?.sub.find((t) => t.selected && t.key !== "no")?.label ?? "");
  const trackKey = $derived(st?.duration != null && roomView === "watch" ? (nowMedia?.src ?? "?") : "");
  $effect(() => {
    if (!trackKey) return;
    subDelay = 0;
    const id = setTimeout(loadTracks, 1200);
    return () => clearTimeout(id);
  });
  /** Kendi tamponun (%; hedef 12 s, sunucuyla aynı); oda oynamıyorsa yok. */
  const myBufferPct = $derived(
    sync && sync.roomMode && sync.roomMode !== "paused" ? Math.min(100, Math.round((sync.cacheAhead / 12) * 100)) : undefined,
  );

  /** Ada göre sabit avatar rengi. */
  function hue(name: string) {
    let h = 0;
    for (const c of name) h = (h * 53 + c.codePointAt(0)! * 7) % 360;
    return h;
  }
</script>


<svelte:window onkeydown={onKey} onpaste={onPaste} />

{#snippet endScreen()}
  {@const title = nowMedia?.title || soloTitle || t("Video")}
  {@const ep = title.match(/^(.*?)\s+S(\d{2})E(\d{2})$/)}
  {@const here = Object.values(members).filter((m) => !m.away).length}
  <div class="end" style="--h:{hue(title)}">
    <div class="end-bd"></div>
    <div class="end-poster"><span>{ep ? ep[1] : title}</span></div>
    <div class="end-info">
      <span class="end-k">{ep ? t("Bölüm bitti") : t("Bitti")} · {new Date().toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" })}</span>
      <h2>{title}</h2>
      <div class="end-stats">
        <div><b>{fmt(st?.duration)}</b><small>{room ? t("birlikte izlendi") : t("izlendi")}</small></div>
        {#if room}<div><b>{here}/{Object.keys(members).length}</b><small>{t("sonuna kadar kaldı")}</small></div>{/if}
        <div><b>{pauses}</b><small>{t("mola")}</small></div>
      </div>
      <div class="end-row">
        {#if ep}
          <button class="big w" onclick={() => closeEnd(() => openTitles(ep[1]))}><Icon name="next" size={16} /> {t("Sonraki bölümü bul")}</button>
        {:else if room}
          <button class="big w" onclick={() => closeEnd(openNight)}><Icon name="spark" size={16} /> {t("Sıradaki ne? Ne izlesek")}</button>
        {/if}
        <button class="big {ep || room ? 'g' : 'w'}" onclick={() => (lobbyOpen = true)}><Icon name="plus" size={16} /> {t("Başka video aç")}</button>
        {#if !room || room.policy !== "host_only" || amControl}<button class="big g" onclick={closeVideo}><Icon name="x" size={16} /> {t("Videoyu kapat")}</button>{/if}
        {#if room ? funControl : true}<button class="big g" onclick={() => closeEnd(() => pickView("listen"))}><Icon name="music" size={16} /> {t("Müziğe geç")}</button>{/if}
        <button class="big g" title={t("Videoya dön")} onclick={() => closeEnd()}><Icon name="x" size={16} /></button>
      </div>
    </div>
  </div>
{/snippet}



{#snippet face(name: string)}
  {@const src = faceOf(name)}
  {#if src}<img class="avimg" {src} alt="" draggable="false" />{/if}
{/snippet}

{#snippet updateChip()}
  {#if update && !update.portable}
    <button class="upd" disabled={updating !== null} onclick={installUpdate}
      title={room ? t("Uygulama yeniden açılır; odaya tekrar katılman gerekir") : t("Kur ve yeniden aç")}>
      {updating === null ? t("KONİ {v} · Güncelle", { v: update.version }) : t("Kuruluyor… %{p}", { p: updating })}
    </button>
  {/if}
{/snippet}

{#if view === "boot"}
  <div class="shell"><TitleBar>{#snippet right()}{@render updateChip()}{/snippet}</TitleBar></div>
{:else if view === "login"}
  <div class="shell">
    <TitleBar>{#snippet right()}{@render updateChip()}{/snippet}</TitleBar>
    <LoginView bind:username bind:code={signupCode} server={serverUrl} onServer={setServer} busy={authBusy} error={noticeText} onSubmit={doAuth} />
  </div>
{:else if view === "home" && adminOpen && social.profile?.admin}
  <div class="shell">
    <TitleBar crumb={t("Yönetim")}>{#snippet right()}{@render updateChip()}{/snippet}</TitleBar>
    <AdminView onBack={() => (adminOpen = false)} />
  </div>
{:else if view === "home" && settingsOpen}
  <div class="shell">
    <TitleBar crumb={t("Ayarlar")}>{#snippet right()}{@render updateChip()}{/snippet}</TitleBar>
    <SettingsView {account} {username} server={serverUrl} {update} {updating}
      onBack={() => (settingsOpen = false)} onLogout={logout}
      onCheckUpdate={() => checkUpdate(false)} onInstall={installUpdate} onBugReport={reportBug} />
  </div>
{:else if view === "home"}
  <div class="shell">
    <TitleBar>
      {#snippet right()}
        {@render updateChip()}
        <div class="acct-menu">
          <button class="me-chip" title={account ?? username} onclick={() => (acctOpen = !acctOpen)}>
            <Avatar id={social.profile?.avatar} name={account ?? username} size={24} />
            <b>{social.profile?.name ?? account ?? username}</b>{#if social.profile?.tag}<span class="mono">#{social.profile.tag}</span>{/if}{#if social.profile?.admin}<span class="atag">ADMIN</span>{/if}
          </button>
          {#if acctOpen}
            <div class="menu" role="menu">
              <div class="menu-head"><b>{account ?? username}</b><small>{account ? t("Hesapla giriş") : t("hesapsız")}</small></div>
              {#if social.profile?.admin}<button role="menuitem" class="adm-item" onclick={() => { acctOpen = false; settingsOpen = false; adminOpen = true; }}>🛡 {t("Yönetim")}</button>{/if}
              <button role="menuitem" onclick={() => { acctOpen = false; settingsOpen = true; }}>{t("Ayarlar")}</button>
              <button role="menuitem" onclick={() => { acctOpen = false; logout(); }}>{t("Çıkış yap")}</button>
            </div>
          {/if}
        </div>
      {/snippet}
    </TitleBar>
    <HomeView name={account ?? username} bind:roomName busy={connecting} error={noticeText} {welcome}
      onCreate={createRoom} onJoin={(code) => connect(true, code)} onSolo={() => { notice = null; solo = true; }} onResume={resumeMovie} />
  </div>
{:else if !fullscreen}
{@const barOn = roomView === "watch" ? !sceneOwned : roomView !== "browse" || st?.duration != null}
<div class="cockpit" data-scene={roomView}
  style="--top-bar:{L.topBarDip}px;--drawer:{drawer ? DRAWER_DIP : 0}px;--drawer-w:{DRAWER_DIP}px;--rail:{L.rightColDip}px;--bottom-bar:{roomView === "listen" || !barOn ? 0 : L.bottomBarDip}px;--drawer-ms:{DRAWER_MS}ms">
  {#key roomView}<div class="sweep" aria-hidden="true"></div>{/key}
  <TitleBar crumb={room?.name ?? t("Tek başına")} sub={room ? (roomMedia?.title || null) : (soloTitle || null)}>
    {#snippet center()}
      <div class="seg modes" role="tablist" aria-label={t("Oda modu")}>
        <i class="thumb" style="transform:translateX({thumb.x}px);width:{thumb.w}px" aria-hidden="true"></i>
        {#each VIEWS as v, i (v.id)}
          <button role="tab" bind:this={modeBtns[i]} aria-selected={roomView === v.id} class:on={roomView === v.id}
            disabled={room !== null && !funControl && roomView !== v.id}
            title={!room ? t("{mode} modu", { mode: t(v.label) }) : !funControl ? t("Modu yalnızca hostlar değiştirir") : t("{mode} modu (herkeste)", { mode: t(v.label) })}
            onclick={() => pickView(v.id)}><Icon name={v.icon} size={14} />{t(v.label)}</button>
        {/each}
      </div>
    {/snippet}
    {#snippet right()}
      {@render updateChip()}
      {#if room}
        <button class="pill {pill.tone}" title={t("Kişileri göster")} onclick={() => (drawer = "people")}>
          <i></i>{pill.text}{#if pill.sub}<span class="pill-sub">· {pill.sub}</span>{/if}
        </button>
      {/if}
    {/snippet}
  </TitleBar>

  {#if roomView === "browse"}
    <div class="scene" in:fade={{ duration: 260 }}>
    <BrowseView inRoom={room !== null} page={roomMedia?.kind === "page" ? { src: roomMedia.src, title: roomMedia.title } : null}
      people={Object.values(members).filter((m) => !m.away).map((m) => ({ id: m.id, name: m.name, me: m.id === sync?.me, ready: m.bufferPct == null || m.bufferPct >= 100 }))}
      pending={pageWait} onPendingTaken={() => (pageWait = null)} onProbe={probe} onClose={closeVideo}
      onDiscover={() => { nightAfterView = true; pickView("watch"); }} onError={(m) => add(m, true)} />
    </div>
  {:else if roomView === "listen"}
    <MusicView title={musicTitle} src={musicSrc} cover={musicCover} {playing} {upNext}
      source={room ? "Lounge" : listPlay.name || t("Tek başına")}
      empty={room
        ? t("Sağdaki sıraya şarkı linki ekle ya da Listelerim'den bir liste çal. Sıra bitince bir sonraki herkeste çalar.")
        : t("Sağdaki çalma listelerinden birini çal ya da yeni liste oluştur.")}>
      {#snippet controls()}
        {@const liked = isUrl(musicSrc) && isLiked(musicSrc)}
        <div class="mp" class:idle={!st?.duration}>
          <div class="mp-row">
            <button class="mp-ic" title={t("Önceki / başa sar")} onclick={musicPrev}><Icon name="prev" size={18} /></button>
            <button class="mp-play" title={playing ? t("Duraklat (Boşluk)") : t("Oynat (Boşluk)")} onclick={togglePause}>
              {#if playing}
                <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><rect x="6" y="4.5" width="4" height="15" rx="1.2" /><rect x="14" y="4.5" width="4" height="15" rx="1.2" /></svg>
              {:else}
                <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M8 5.3v13.4a1 1 0 0 0 1.5.9l10.6-6.7a1 1 0 0 0 0-1.8L9.5 4.4A1 1 0 0 0 8 5.3z" /></svg>
              {/if}
            </button>
            <button class="mp-ic" title={canNext ? t("Sonraki") : t("Sırada şarkı yok")} disabled={!canNext} onclick={musicNext}><Icon name="next" size={18} /></button>
            {#if isUrl(musicSrc)}
              <button class="mp-ic heart" class:on={liked} title={liked ? t("Beğendiklerim'den çıkar") : t("Beğendiklerim'e ekle")}
                onclick={() => toggleLike({ src: musicSrc, title: musicTitle, dur: st?.duration ?? null })}><Icon name="heart" size={18} /></button>
            {/if}
            {#if detached}
              <button class="white small" onclick={() => run("Odayla Senkronla", syncResync(), true)}>{t("Odayla senkronla")}</button>
            {/if}
            <div class="mp-vol" title={t("Ses %{v}", { v: volume })}>
              <button class="mute" title={volume > 0 ? t("Sessize al") : t("Sesi aç")} onclick={toggleMute}><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z" fill="currentColor" />{#if volume > 0}<path d="M15.5 9a4 4 0 0 1 0 6" />{/if}{#if volume > 60}<path d="M18.5 6.5a7.5 7.5 0 0 1 0 11" />{/if}{#if volume === 0}<path d="M16 9.5l5 5M21 9.5l-5 5" />{/if}</svg></button>
              <input class="vol" type="range" min="0" max="130" step="1" value={volume} style="--p:{(volume / 130) * 100}%"
                oninput={(e) => setVolume(+e.currentTarget.value)} />
            </div>
          </div>
          <div class="mp-seek">
            <span class="time">{fmt(hoverPos ?? st?.timePos)}</span>
            <div class="seekwrap" role="presentation"
              onpointermove={(e) => {
                if (!st?.duration) return;
                const r = e.currentTarget.getBoundingClientRect();
                hoverPos = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * st.duration;
              }}
              onpointerleave={() => (hoverPos = null)}>
              {#if wave}
                <svg class="wave" viewBox="0 0 {wave.length} 100" preserveAspectRatio="none" aria-hidden="true">
                  {#each wave as v, i}
                    <rect x={i + 0.15} width="0.7" y={50 - Math.max(3, v * 48)} height={Math.max(6, v * 96)}
                      class:played={i < (seekPct / 100) * wave.length} />
                  {/each}
                </svg>
              {/if}
              <input class="seek" type="range" min="0" max={st?.duration ?? 0} step="0.1" value={pos} style="--p:{seekPct}%"
                oninput={(e) => { seeking = true; seekVal = +e.currentTarget.value; }}
                onchange={(e) => { seekTo(+e.currentTarget.value); seeking = false; }} />
            </div>
            <span class="time">{fmt(st?.duration)}</span>
          </div>
        </div>
      {/snippet}
    </MusicView>
  {:else}
  <!-- mpv bu kutunun üstünde native pencerede çizer; buraya overlay yok (kokpit modeli). -->
  <div class="video">
    {#if fdOpen}
      <div class="stage" in:fade={{ duration: 200 }}>
        <TitlesPane withSearch inRoom={room !== null} initial={fdQuery} onPlay={openDebrid} onExit={() => (fdOpen = false)} onError={(m) => add(m, true)} />
      </div>
    {:else if endOn}
      <div class="stage" in:fade={{ duration: 300 }}>{@render endScreen()}</div>
    {:else if lobbyOn}
      <div class="stage" in:fly={{ y: 14, duration: 420, easing: cubicOut }}>
        <CinemaLobby bind:value={src} busy={opening} {hue}
          people={room ? Object.values(members).map((m) => ({ id: m.id, name: m.name })) : []} room={room?.name ?? null}
          error={roomMedia?.status === "error" ? (roomMedia.error ? tx(roomMedia.error) : roomMedia.title ?? roomMedia.src) : null}
          onOpen={(t) => open(t)} onTitles={openTitles}
          onNight={room && !fullscreen ? openNight : null}
          onDownload={room ? (u) => run("Herkes İndirsin", syncDownload(u), true) : null}
          onResume={resumeHere} />
      </div>
    {:else if st?.duration == null}
      {@const mediaName = roomMedia ? roomMedia.title || roomMedia.src : soloMedia?.title || soloMedia?.src || ""}
      {#key roomView}
      <div class="empty" in:fly={{ y: 14, duration: 420, easing: cubicOut }}>
        {#if roomMedia?.kind === "download" && roomMedia.status !== "error"}
          {@const total = Object.keys(members).length}
          {@const ready = Object.values(downloads).filter((d) => d.state === "ready").length}
          <div class="ring" style="--p:{total ? (ready / total) * 100 : 0}">
            <svg width="120" height="120" viewBox="0 0 120 120" aria-hidden="true">
              <circle cx="60" cy="60" r="52" fill="none" stroke="rgba(255,255,255,.08)" stroke-width="8" />
              <circle class="arc" cx="60" cy="60" r="52" fill="none" stroke="var(--accent)" stroke-width="8" stroke-linecap="round" pathLength="100" transform="rotate(-90 60 60)" />
            </svg>
            <span>{ready}/{total}</span>
          </div>
          <b>{t("Herkes indiriyor")}</b>
          <span>{mediaName}<br />{t("Herkes hazır olunca oda birlikte başlar.")}</span>
        {:else if roomMedia?.status === "resolving"}
          <div class="empty-logo spin"><Logo size={36} /></div>
          <b>{t("Hazırlanıyor…")}</b>
          <span>{mediaName}</span>
        {:else if roomMedia?.status === "need_file"}
          <div class="empty-logo"><Icon name="file" size={30} /></div>
          <b>{t("Bu odada bir dosya açıldı")}</b>
          <span>{mediaName}<br />{t("Kendi kopyanı buraya sürükle ya da yolunu \"Video ekle\"ye yapıştır.")}</span>
        {:else if roomMedia?.status === "no_provider"}
          <div class="empty-logo"><Icon name="gear" size={30} /></div>
          <b>{t("Bu film kendi kaynağınla açılıyor")}</b>
          <span>{mediaName}<br />{t("Sende henüz sağlayıcı ya da eklenti yok, o yüzden video açılmadı. Oda seni beklemeden devam ediyor.")}</span>
          {#if providerSetup}
            <div class="prov"><DebridSettings /></div>
          {/if}
          <div class="hints">
            {#if !providerSetup}<button class="small" onclick={() => (providerSetup = true)}>{t("Sağlayıcı ekle")}</button>{/if}
            <button class="small" onclick={() => run("Tekrar dene", syncResync(), true)}>{t("Tekrar dene")}</button>
          </div>
        {:else if roomMedia?.status === "error"}
          <div class="empty-logo bad"><Icon name="x" size={30} /></div>
          <b>{t("Video açılamadı")}</b>
          <span>{roomMedia.error ? tx(roomMedia.error) : mediaName}</span>
          <div class="hints"><button class="small" onclick={() => (drawer = "add")}>{t("Başka video aç")}</button></div>
        {:else}
          <div class="empty-logo spin"><Logo size={36} /></div>
          <b>{t("Açılıyor…")}</b>
          <span>{mediaName || src}</span>
        {/if}
        {#if (room ? roomMedia : soloMedia) && (!room || room.policy !== "host_only" || amControl)}
          <div class="hints"><button class="small" onclick={closeVideo}>{t("Videoyu kapat")}</button></div>
        {/if}
      </div>
      {/key}
    {/if}
  </div>
  {/if}

  {#if drawer || drawerLive}
    {@const head = shown === "queue" && !room ? { eyebrow: "Müzik", title: "Çalma listeleri" } : DRAWERS[shown]}
    <aside class="drawer" class:open={drawer !== null} inert={!drawer}>
     <div class="d-inner">
      {#key shown}
      <header class="d-head">
        <div in:fly={{ x: 18, duration: 300, easing: cubicOut }}><div class="eyebrow">{t(head.eyebrow)}</div><h2>{t(head.title)}</h2></div>
        <button class="icon" title={t("Kapat (Esc)")} onclick={() => (drawer = null)}><Icon name="x" size={14} /></button>
      </header>
      <div class="d-body">
        {#if shown === "add"}
          {#if (room ? roomMedia : soloMedia) && (!room || room.policy !== "host_only" || amControl)}
            <div class="now" title={nowMedia?.src}>
              <div class="eyebrow">{t("Şu an açık")}</div>
              <div class="now-title">{nowMedia?.title || nowMedia?.src}</div>
              <button class="small" style="margin-top:4px;margin-left:0;align-self:flex-start" onclick={closeVideo}>{t("Videoyu kapat")}</button>
            </div>
          {/if}
          <div class="row">
            <input placeholder={t("Link yapıştır ya da dosya yolu")} bind:value={src} onkeydown={(e) => e.key === "Enter" && open()} />
            {#if sniffing}
              <button title={t("Yakalayıcı açık: {page}", { page: sniffing })} onclick={() => run("Yakalayıcı", snifferClose(), true)}>{t("İptal")}</button>
            {:else}
              <button class="white" disabled={opening || !src.trim()} onclick={() => open()}>{opening ? t("Açılıyor…") : t("Aç")}</button>
            {/if}
          </div>
          {#if sniffing}
            <div class="banner blue">{t("Site doğrudan desteklenmiyor. Keşfet'te sayfadaki videoyu oynat, herkeste sayfada eşitlenir.")}</div>
          {:else}
            <div class="hint">{t("YouTube, Vimeo, .mp4/.m3u8 linki ya da dosya. Desteklenmeyen sitede video Keşfet'te sayfada oynar.")}</div>
          {/if}
          {#if room}
            <button class="opt" disabled={!isUrl(src)} onclick={() => run("Herkes İndirsin", syncDownload(src.trim()), true)}>
              <span class="opt-ic"><Icon name="down" /></span>
              <span class="opt-txt"><b>{t("Herkes indirsin")}</b><small>{t("Herkes kendine indirir, hazır olunca birlikte başlar")}</small></span>
            </button>
          {/if}
          <div class="opt static">
            <span class="opt-ic"><Icon name="file" /></span>
            <span class="opt-txt"><b>{t("Dosya aç")}</b><small>{t("Video dosyasını pencereye sürükle ya da yolunu yapıştır")}</small></span>
          </div>
          <div class="opt static">
            <span class="opt-ic"><Icon name="cc" /></span>
            <span class="opt-txt"><b>{t("Altyazı ekle")}</b><small>{t(".srt .ass .vtt dosyasını sürükle")} · <kbd>G</kbd>/<kbd>H</kbd> ±50 ms</small></span>
          </div>
          {#if room && Object.keys(members).length <= 1}
            <div class="banner blue">
              <b>{t("Arkadaşlarını çağır")}</b>
              <span><button class="chip" onclick={() => (drawer = "people")}><Icon name="users" size={13} />{t("Kişiler'den çağır")}</button></span>
            </div>
          {/if}
        {:else if shown === "tracks"}
          <div class="seg tabs" role="tablist">
            {#each [["sub", "Altyazı"], ["audio", "Ses"], ["video", "Kalite"]] as [k, l] (k)}
              <button role="tab" class:on={trackTab === k} aria-selected={trackTab === k} onclick={() => (trackTab = k as TrackKind)}>{t(l)}</button>
            {/each}
          </div>
          {@const list = tracks?.[trackTab] ?? []}
          {#each list as tr (tr.key)}
            <button class="topt" class:on={tr.selected} onclick={() => pickTrack(trackTab, tr.key, tr.label)}>
              <span class="rd"></span><span class="grow">{tr.key === "no" ? t("Kapalı") : tr.label}</span>
            </button>
          {:else}
            <div class="hint">{tracks ? (trackTab === "sub" ? t("Bu videoda altyazı yok.") : trackTab === "audio" ? t("Bu kaynakta başka ses ya da dublaj yok.") : t("Bu kaynakta başka kalite yok.")) : t("Okunuyor…")}</div>
          {/each}
          {#if trackTab === "sub"}
            {#if nowMedia}
              <SubsFinder inRoom={room !== null} canControl={funControl} title={nowMedia.title || nowMedia.src} file={nowMedia.file ?? ""} onAdded={loadTracks} onLog={(m) => add(m)} onError={(m) => add(m, true)} />
            {/if}
            <div class="opt static">
              <span class="opt-ic"><Icon name="cc" /></span>
              <span class="opt-txt"><b>{t("Altyazı dosyası ekle")}</b><small>{t(".srt .ass .vtt dosyasını pencereye sürükle")}</small></span>
            </div>
            <div class="eyebrow">{t("Zamanlama")}</div>
            <div class="delay">
              <span>{t("Gecikme")}</span>
              <button class="icon" title={t("50 ms erken (G)")} onclick={() => nudgeSub(-0.05)}>−</button>
              <b>{subDelay >= 0 ? "+" : ""}{subDelay.toLocaleString(locale(), { minimumFractionDigits: 3, maximumFractionDigits: 3 })} s</b>
              <button class="icon" title={t("50 ms geç (H)")} onclick={() => nudgeSub(0.05)}>+</button>
            </div>
            <div class="hint">{t("Kısayol:")} <kbd>G</kbd> {t("erken")} · <kbd>H</kbd> {t("geç (50 ms).")}</div>
          {/if}
          <div class="hint">{t("Seçimin yalnızca sende geçerli; odadaki herkes kendi kalitesini, sesini ve altyazısını seçer.")}</div>
        {:else if shown === "queue" && room}
          <div class="seg tabs" role="tablist" style="margin-bottom: 12px">
            <button role="tab" class:on={musicTab === "lounge"} aria-selected={musicTab === "lounge"} onclick={() => (musicTab = "lounge")}>Lounge</button>
            <button role="tab" class:on={musicTab === "lists"} aria-selected={musicTab === "lists"} onclick={() => (musicTab = "lists")}>{t("Listelerim")}</button>
          </div>
          {#if musicTab === "lounge"}
            <QueuePanel canControl={funControl} me={sync?.me ?? null} title={musicTitle} src={musicSrc} cover={musicCover}
              onError={(m) => add(m, true)} onLog={(m) => add(m)} />
          {:else}
            <PlaylistPanel inRoom current={musicNow} onError={(m) => add(m, true)} onLog={(m) => add(m)} onNext={musicNext} onPrev={musicPrev} onStop={stopPlaylist} />
          {/if}
        {:else if shown === "queue"}
          <PlaylistPanel inRoom={false} current={musicNow} onError={(m) => add(m, true)} onLog={(m) => add(m)} onNext={musicNext} onPrev={musicPrev} />
        {:else if shown === "people" && room}
          <ul class="members">
            {#each Object.values(members) as m (m.id)}
              {@const waiting = m.bufferPct != null && m.bufferPct < 100 && !m.away}
              <li class:away={m.away} class:warn={waiting}>
                <div class="member">
                  <span class="avatar" style="--h:{hue(m.name)}">{initial(m.name)}{@render face(m.name)}<i class="dot {dotTone(m)}"></i></span>
                  <span class="name">{m.name}{#if m.admin}<span class="atag">ADMIN</span>{/if}</span>
                  {#if m.id === room.host}<span class="badge host">Host</span>{/if}
                  {#if m.cohost}<span class="badge">{t("Yardımcı")}</span>{/if}
                  {#if m.id === sync?.me}<span class="you">{t("sen")}</span>{/if}
                  <span class="num" class:warn-t={waiting}>
                    {#if m.away}{t("bağlantı koptu")}
                    {:else if waiting}{t("Tamponluyor")}
                    {:else if m.id === sync?.me}{ms(sync?.driftMs)}
                    {:else if m.detached}{t("ayrı izliyor")}
                    {:else}{ms(m.driftMs)}{/if}
                  </span>
                  <!-- Diğerlerinin tamponu aşağıdaki çubukta; kendi tamponun yalnızca burada (sunucu sana kendi durumunu yollamaz). -->
                  {#if m.id === sync?.me}
                    <NetBadge rttMs={sync?.rttMs} state={sync?.localState.toLowerCase()} bufferPct={myBufferPct} />
                  {:else}
                    <NetBadge rttMs={m.rttMs} state={m.state} />
                  {/if}
                </div>
                {#if waiting}
                  <div class="sub"><div class="meter warn"><i style="width:{m.bufferPct}%"></i></div><span class="num">%{m.bufferPct}</span></div>
                {/if}
                {#if roomMedia?.kind === "download" && !m.away}
                  {@const d = downloads[m.id]}
                  <div class="sub" class:bad={d?.state === "failed"} title={tx(d?.error ?? "")}>
                    <div class="meter" class:ok={d?.state === "ready"}><i style="width:{d?.pct ?? 0}%"></i></div><span class="num">{dlText(d)}</span>
                  </div>
                {/if}
                {#if amControl && offers[m.id] && !m.detached}
                  <button class="block" onclick={() => continueWithout(m.id)}>{t("{name} olmadan devam et", { name: m.name })}</button>
                {/if}
              </li>
            {/each}
          </ul>
          {#if account}
            <div class="eyebrow call-h">{t("Arkadaşlarını çağır")}</div>
            <p class="call-ask">{t("Çağrı arkadaşının ana ekranına düşer; kabul edince odaya girer.")}</p>
            <ul class="callable">
              {#each callable as f (f.name)}
                <li class:off={!f.online}>
                  <Avatar id={f.avatar} name={f.name} size={30} />
                  <span class="cf"><b>{f.name.replace(/#\d+$/, "")}</b><small>{f.room ? t("{room} odasında", { room: f.room }) : f.online ? t("çevrimiçi") : t("çevrimdışı · açınca görür")}</small></span>
                  {#if called[f.name]}<span class="sent">{t("çağrıldı ✓")}</span>
                  {:else}<button class="sm" class:white={f.online} onclick={() => callFriend(f.name)}>{t("Çağır")}</button>{/if}
                </li>
              {:else}
                <li class="none">{social.presence ? t("Çağrılacak arkadaş yok. Ana ekrandan arkadaş ekleyebilirsin.") : t("Arkadaş listesi yükleniyor…")}</li>
              {/each}
            </ul>
          {/if}
        {:else if shown === "feed"}
          {#if roomMedia}
            <div class="now" class:bad={roomMedia.status === "error"} title={roomMedia.src}>
              <div class="eyebrow">{t("Şu an oynatılan")}</div>
              <div class="now-title">{roomMedia.title || roomMedia.src}</div>
              <span class="tag" class:ok={roomMedia.status === "ready"} class:bad={roomMedia.status === "error"}>{t(KIND[roomMedia.kind])} · {t(MEDIA_STATE[roomMedia.status])}</span>
              {#if room?.policy !== "host_only" || amControl}
                <button class="small" style="margin-top:4px;margin-left:0;align-self:flex-start" onclick={closeVideo}>{t("Videoyu kapat")}</button>
              {/if}
            </div>
          {/if}
          <div class="eyebrow">{t("Olaylar")}</div>
          <ul class="log">
            {#each log as l}
              <li class:err={l.err}><span>{l.t}</span>{l.show()}</li>
            {/each}
          </ul>
        {:else if shown === "settings" && room}
          <div class="eyebrow">{t("Kontrol")}</div>
          <label class="card switch">
            <span><b>{t("Yalnızca hostlar kontrol eder")}</b><small>{t("Oynat, durdur, atla ve video açma")}</small></span>
            <input type="checkbox" role="switch" checked={room.policy === "host_only"} disabled={!amHost}
              onchange={(e) => run("Politika", syncSetPolicy((e.currentTarget as HTMLInputElement).checked), true)} />
          </label>
          {#if amHost && Object.keys(members).length > 1}
            <div class="eyebrow">{t("Yardımcı hostlar")}</div>
            <ul class="members">
              {#each Object.values(members).filter((x) => x.id !== sync?.me) as m (m.id)}
                <li><div class="member">
                  <span class="avatar" style="--h:{hue(m.name)}">{initial(m.name)}</span>
                  <span class="name">{m.name}</span>
                  {#if m.cohost}<span class="badge">{t("Yardımcı")}</span>{/if}
                  <button class="small" onclick={() => run(m.cohost ? "Yardımcı hostluk alındı" : "Yardımcı host yapıldı", syncSetCohost(m.id, !m.cohost), true)}>
                    {m.cohost ? t("Kaldır") : t("Yardımcı yap")}</button>
                </div></li>
              {/each}
            </ul>
          {/if}
          {#if sync}
            <details class="card stats">
              <summary>{t("Senkron ayrıntıları")}</summary>
              <dl>
                <dt>{t("Durum")}</dt><dd>{sync.localState}</dd>
                <dt>{t("Hız")}</dt><dd>{sync.speed.toFixed(3)}x</dd>
                <dt>RTT</dt><dd>{sync.rttMs == null ? "–" : Math.round(sync.rttMs) + " ms"}</dd>
                <dt>{t("Tampon")}</dt><dd>{sync.cacheAhead.toFixed(1)} s</dd>
                <dt>{t("Geri seek engellendi")}</dt><dd class:bad={sync.backwardBlocked > 0}>{sync.backwardBlocked}</dd>
                <dt>{t("İleri atlama · Stall")}</dt><dd>{sync.forwardSkips} · {sync.stalls}</dd>
                {#if sync.csvPath}<dt>Log</dt><dd class="path" title={sync.csvPath}>{sync.csvPath}</dd>{/if}
              </dl>
            </details>
          {/if}
          <div class="card"><BugReport send={reportBug} /></div>
        {/if}
      </div>{/key}
     </div>
    </aside>
  {/if}

  {#if leaveAsk && room}
    <div class="leave-scrim" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) leaveAsk = false; }}
      onkeydown={(e) => { if (e.key === "Escape") leaveAsk = false; }} transition:fade={{ duration: 150 }}>
      <div class="leave" role="dialog" aria-label={t("Odadan çık")} in:scale={{ start: 0.94, duration: 220, easing: cubicOut }}>
        <small class="k">{room.name}</small>
        <h2>{t("Odadan çıkıyor musun?")}</h2>
        <button class="opt" onclick={() => { leaveAsk = false; run("Ayrıl", syncLeave(), true); }}>
          <span class="ic">↩</span><span><b>{t("Odadan ayrıl")}</b><small>{t("Oda açık kalır. Ana ekrandaki \"Açık odalar\"dan istediğin an geri dönersin.")}</small></span></button>
        {#if amHost}
          <button class="opt red" onclick={() => { leaveAsk = false; run("Kapat", syncFun({ t: "close_room" }), true); }}>
            <span class="ic">⏻</span><span><b>{t("Odayı herkes için kapat")}</b><small>{Object.keys(members).length > 1 ? t("Odadaki herkes ana ekrana döner.") : t("Oda listeden kalkar, geri dönülmez.")}</small></span></button>
        {/if}
        <button class="cancel" onclick={() => (leaveAsk = false)}>{t("Vazgeç")}</button>
      </div>
    </div>
  {/if}

  {#if nightOpen && room}
    <NightView me={sync?.me ?? null} canControl={funControl} {hue} onClose={() => (nightOpen = false)}
      people={Object.keys(members).length} away={reconnecting !== null || Object.values(members).some((m) => m.away)}
      onError={(m) => add(m, true)} onFind={playMatch} />
  {/if}
  <nav class="rail">
    {#each tools as tool, i (tool.id)}
      <button class="ri" class:on={tool.id === "discover" ? nightOpen : drawer === tool.id} title={t(tool.title)} onclick={() => toggle(tool.id)}
        in:scale={{ start: 0.4, duration: 380, delay: 60 + i * 45, easing: backOut }}
        out:scale={{ start: 0.6, duration: 160, easing: cubicOut }}
        animate:flip={{ duration: 320, easing: cubicOut }}>
        <Icon name={tool.icon} size={20} />{#if tool.badge}<span class="cnt">{tool.badge}</span>{/if}
      </button>
    {/each}
    {#if room}
      <div class="rail-sep"></div>
      <!-- Odadakiler dikey: tıklayınca Kişiler çekmecesi (ayrı ikon yok). -->
      <div class="stack" class:dim={reconnecting !== null}>
        {#each Object.values(members).slice(0, 6) as m (m.id)}
          {@const pct = m.away ? 0 : m.id === sync?.me ? (myBufferPct ?? 100) : (m.bufferPct ?? 100)}
          <!-- Halka: tampon (yeşil dolu = hazır, turuncu = bekleniyor). -->
          <button class="avatar" class:warn={pct < 100 && !m.away} class:off={m.away} style="--h:{hue(m.name)};--v:{pct}"
            title={m.away ? t("{name} · bağlantı koptu", { name: m.name }) : pct < 100 ? t("{name} · tampon %{pct}", { name: m.name, pct }) : m.name} onclick={() => (drawer = "people")}>{initial(m.name)}{@render face(m.name)}</button>
        {/each}
      </div>
      <button class="ri danger" title={t("Odadan ayrıl")} onclick={() => { drawer = null; leaveAsk = true; }}><Icon name="out" size={20} /></button>
    {:else}
      <button class="ri danger" title={t("Çıkış")} onclick={() => { drawer = null; solo = false; roomView = "browse"; stopList(); playerPause().catch(() => {}); }}><Icon name="out" size={20} /></button>
    {/if}
  </nav>

  {#if roomView !== "listen"}
  {@const total = Object.keys(members).length}
  {#if holdOn}
  <!-- Biri bekleniyor: şerit turuncu banda döner (video üstünde hiçbir şey yok). -->
  <footer class="bar hold" in:fade={{ duration: 200 }}>
    <span class="sp" aria-hidden="true"></span>
    <span class="h-t">
      <b>{waiting.length === 1 ? t("{name}'i bekliyoruz", { name: waiting[0].name }) : waiting.length ? t("{n} kişiyi bekliyoruz", { n: waiting.length }) : t("Herkes bekleniyor")}</b>
      <small>{waiting.length ? t("Bağlantısı yavaşladı · ") : ""}{t("film {pos}'ta duruyor", { pos: fmt(sync?.roomPos ?? st?.timePos) })}</small>
    </span>
    <div class="bufs">
      {#each Object.values(members).filter((m) => !m.away) as m (m.id)}
        {@const pct = m.id === sync?.me ? (myBufferPct ?? 100) : (m.bufferPct ?? 100)}
        <span class="bf"><span class="bring" class:warn={pct < 100} style="--v:{pct}"><span class="av" style="--h:{hue(m.name)}" title={m.name}>{initial(m.name)}</span></span>{#if pct < 100}<b>%{pct}</b>{:else}{t("hazır")}{/if}</span>
      {/each}
    </div>
    <div class="h-act">
      {#if !room || room.policy !== "host_only" || amControl}
        <button onclick={closeVideo}>{t("Videoyu kapat")}</button>
      {/if}
      {#each waiting.filter((m) => amControl && offers[m.id] && !m.detached) as m (m.id)}
        <button onclick={() => continueWithout(m.id)}>{t("{name} olmadan devam et", { name: m.name })}</button>
      {/each}
    </div>
  </footer>
  {:else}
  <footer class="bar" class:idle={!st?.duration} class:gone={!barOn} class:narrow={drawer !== null} inert={!barOn}>
    {#if roomView === "watch"}
      {@const nt = nowMedia?.title || soloTitle || ""}
      <div class="np" title={nt}>
        <span class="pst" style="--h:{hue(nt || "koni")}"></span>
        <span class="now-t"><b>{nt || t("Video")}</b><small>{[nowMedia ? t(KIND[nowMedia.kind]) : null, subLabel ? t("{sub} altyazı", { sub: subLabel }) : null].filter(Boolean).join(" · ")}</small></span>
      </div>
    {/if}
    <div class="mid">
      <div class="ctl">
        {#if roomView === "watch"}<button class="cb" title={t("10 sn geri")} disabled={!st?.duration} onclick={() => seekTo(Math.max(0, (st?.timePos ?? 0) - 10))}><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 3-6.7L3 8" /><path d="M3 3v5h5" /></svg><small>10</small></button>{/if}
        <button class="play" title={playing ? t("Duraklat (Boşluk)") : t("Oynat (Boşluk)")} onclick={togglePause}>
          {#if playing}
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><rect x="6" y="4.5" width="4" height="15" rx="1.2" /><rect x="14" y="4.5" width="4" height="15" rx="1.2" /></svg>
          {:else}
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M8 5.3v13.4a1 1 0 0 0 1.5.9l10.6-6.7a1 1 0 0 0 0-1.8L9.5 4.4A1 1 0 0 0 8 5.3z" /></svg>
          {/if}
        </button>
        {#if roomView === "watch"}<button class="cb" title={t("10 sn ileri")} disabled={!st?.duration} onclick={() => seekTo(Math.min(st?.duration ?? 0, (st?.timePos ?? 0) + 10))}><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 12a9 9 0 1 1-3-6.7L21 8" /><path d="M21 3v5h-5" /></svg><small>10</small></button>{/if}
        {#if roomView === "watch" && (room ? roomMedia || st?.duration != null : soloMedia || st?.duration != null)}
          <button class="cb" title={room ? t("Videoyu kapat (herkes için)") : t("Videoyu kapat")} disabled={room?.policy === "host_only" && !amControl}
            onclick={closeVideo}><svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><rect x="6" y="6" width="12" height="12" rx="2" /></svg></button>
        {/if}
      </div>
      <span class="time" class:hover={hoverPos !== null}>{fmt(hoverPos ?? st?.timePos)}</span>
      <div class="seekwrap" role="presentation"
        onpointermove={(e) => {
          if (!st?.duration) return;
          const r = e.currentTarget.getBoundingClientRect();
          hoverPos = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)) * st.duration;
        }}
        onpointerleave={() => (hoverPos = null)}>
        {#if room && st?.duration && together.length > 1}
          <!-- Herkes çizgide: birlikte izleyenler oynatma noktasında. -->
          <span class="who" style="left:{seekPct}%" title={together.map((m) => m.name).join(", ")}>
            {#each together.slice(0, 5) as m (m.id)}<span class="av" style="--h:{hue(m.name)}">{initial(m.name)}</span>{/each}
          </span>
        {/if}
        <input class="seek" type="range" min="0" max={st?.duration ?? 0} step="0.1" value={pos} style="--p:{seekPct}%;--b:{bufPct}%"
          oninput={(e) => { seeking = true; seekVal = +e.currentTarget.value; }}
          onchange={(e) => { seekTo(+e.currentTarget.value); seeking = false; }} />
      </div>
      <span class="time">{fmt(st?.duration)}</span>
    </div>
    <div class="right">
      {#if roomView === "watch"}
      <ResumeBar media={nowMedia?.kind === "page" ? null : (nowMedia as ResumeMedia | null)} pos={st?.timePos ?? null} duration={st?.duration ?? null}
        roomPos={inRoom ? (sync?.roomPos ?? null) : null} room={room?.name}
        mates={Object.values(members).filter((x) => x.id !== sync?.me).map((x) => x.name)}
        canSeek={inRoom ? amControl || room?.policy !== "host_only" : true} onSeek={seekTo} />
      {/if}
      {#if detached}
        <span class="capsule warn">
          <Icon name="sync" size={14} />{t("{time} geridesin", { time: fmt(Math.max(0, (sync?.roomPos ?? 0) - (sync?.localPos ?? 0))) })}
          <button class="white small" onclick={() => run("Odayla Senkronla", syncResync(), true)}>{t("Odayla senkronla")}</button>
        </span>
      {:else if room && st?.duration}
        <button class="chip2 info" class:sync={together.length === total} title={t("Odadakilerden kaçı birlikte izliyor")} onclick={() => (drawer = "people")}>
          <Icon name={together.length === total ? "check" : "sync"} size={14} />{t("Senkron {n}/{total}", { n: together.length, total })}
        </button>
      {/if}
      {#if roomView === "watch" && st?.duration}
        <button class="chip2 info" class:on={drawer === "tracks"} title={t("Altyazı · ses · kalite")} onclick={() => toggle("tracks")}>
          {#if subLabel}<Icon name="cc" size={14} /><b class="sl">{subLabel}</b>{:else}<Icon name="cc" size={14} />{t("Altyazı")}{/if}
        </button>
      {/if}
      {#if room}<Reactions compact={roomView === "watch"} />{/if}
      {#if !inRoom}
        <select value={st?.speed ?? 1} title={t("Hız")} onchange={(e) => { const v = e.currentTarget.value; run("Hız " + v, playerSetSpeed(+v), false, () => t("Hız {v}", { v })); }}>
          {#each speeds as s}<option value={s}>{s.toFixed(2)}x</option>{/each}
        </select>
      {/if}
      <div class="volume" title={t("Ses %{v}", { v: volume })}>
        <button class="mute" title={volume > 0 ? t("Sessize al") : t("Sesi aç")} onclick={toggleMute}><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z" fill="currentColor" />{#if volume > 0}<path d="M15.5 9a4 4 0 0 1 0 6" />{/if}{#if volume > 60}<path d="M18.5 6.5a7.5 7.5 0 0 1 0 11" />{/if}{#if volume === 0}<path d="M16 9.5l5 5M21 9.5l-5 5" />{/if}</svg></button>
        <input class="vol" type="range" min="0" max="130" step="1" value={volume} style="--p:{(volume / 130) * 100}%"
          oninput={(e) => setVolume(+e.currentTarget.value)} />
      </div>
      {#if !isMac}<button class="icon" title={t("TV'ye yansıt")} onclick={() => run("TV'ye yansıt", openCastPanel())}><Icon name="cast" size={15} /></button>{/if}
      {#if roomView === "watch"}
        <button class="icon" title={t("Tam ekran (F)")} onclick={() => setFs(true)}><Icon name="max" size={15} /></button>
      {/if}
    </div>
  </footer>
  {/if}
  {/if}
</div>
{/if}

<style>
  .shell { display: grid; height: 100vh; width: 100vw; grid-template-rows: var(--top-bar) 1fr; grid-template-areas: "title" "main"; }
  .acct-menu { position: relative; }
  .modes { position: relative; }
  .modes button { position: relative; z-index: 1; display: inline-flex; align-items: center; gap: 6px; padding: 4px 12px; font-size: 12.5px; font-weight: 550; transition: color 0.25s; }
  .modes button.on { background: transparent; box-shadow: none; }
  .modes button.on :global(svg) { color: var(--scene); filter: drop-shadow(0 0 6px var(--scene-glow)); transition: color 0.3s; }
  /* Seçili modun altında kayan cam pil; sahne rengiyle parlar. */
  .modes .thumb {
    position: absolute; left: 0; top: 3px; bottom: 3px; border-radius: 7px; pointer-events: none;
    background: rgba(255, 255, 255, 0.13); box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3), 0 0 18px -4px var(--scene-glow), inset 0 0 0 1px rgba(255, 255, 255, 0.06);
    transition: transform 0.42s cubic-bezier(0.34, 1.4, 0.5, 1), width 0.42s cubic-bezier(0.34, 1.4, 0.5, 1), box-shadow 0.4s;
  }
  .modes button:disabled { opacity: 0.35; }
  .upd { height: 26px; padding: 0 12px; border-radius: 99px; font-size: 12px; font-weight: 600; background: var(--accent); border: 0; color: #fff; }
  .upd:hover { background: #3395ff; }
  .call-h { margin-top: 18px; }
  .call-ask { margin: 2px 0 8px; color: var(--text-3); font-size: 12px; }
  .callable { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .callable li { display: flex; align-items: center; gap: 10px; padding: 6px 8px; border-radius: 12px; }
  .callable li:hover { background: var(--glass); }
  .callable li.off { opacity: 0.55; }
  .callable li.none { color: var(--text-3); font-size: 12px; }
  .cf { flex: 1; min-width: 0; display: flex; flex-direction: column; line-height: 1.25; }
  .cf b { font-weight: 600; }
  .cf small { color: var(--text-3); font-size: 11px; }
  .callable .sm { height: 28px; padding: 0 12px; border-radius: 9px; font-size: 12px; font-weight: 650; }
  .callable .sent { color: var(--green); font-size: 12px; font-weight: 700; }
  .me-chip {
    display: flex; align-items: center; gap: 8px; padding: 3px 10px 3px 3px; border-radius: 99px;
    border: 1px solid var(--line-2); background: var(--glass); font-size: 12.5px;
  }
  .me-chip b { font-weight: 600; }
  .adm-item { color: #ffd60a !important; font-weight: 650; }
  .me-chip .mono { color: var(--text-3); font-size: 11px; }
  .menu {
    position: absolute; right: 0; top: 34px; z-index: 10; width: 220px; padding: 5px; border-radius: 12px;
    background: rgba(36, 37, 41, 0.92); border: 1px solid var(--line-2); box-shadow: 0 20px 50px rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(30px); display: flex; flex-direction: column;
  }
  .menu-head { padding: 8px 10px; display: flex; flex-direction: column; border-bottom: 1px solid var(--line-2); margin-bottom: 4px; }
  .menu-head small { color: var(--text-3); }
  .menu button { text-align: left; background: transparent; border: 0; border-radius: 7px; padding: 7px 10px; }
  .menu button:hover { background: var(--accent); }
  .cockpit {
    --scene: #0a84ff; --scene-glow: rgba(10, 132, 255, 0.55);
    display: grid; height: 100vh; width: 100vw; position: relative; overflow: hidden;
    grid-template-columns: minmax(0, 1fr) var(--drawer) var(--rail);
    grid-template-rows: var(--top-bar) minmax(0, 1fr) var(--bottom-bar);
    grid-template-areas: "title title title" "video drawer rail" "bar drawer rail";
    transition: grid-template-columns var(--drawer-ms) cubic-bezier(0.32, 0.72, 0, 1), grid-template-rows 0.36s cubic-bezier(0.32, 0.72, 0, 1);
  }
  /* Sahne renkleri: Keşfet yeşil, Sinema mavi, Müzik mor. */
  .cockpit[data-scene="browse"] { --scene: #30d158; --scene-glow: rgba(48, 209, 88, 0.5); }
  .cockpit[data-scene="listen"] { --scene: #bf5af2; --scene-glow: rgba(191, 90, 242, 0.55); }
  /* Sahne değişince başlık çubuğunun altından geçen ışık. */
  .sweep {
    grid-area: title; align-self: end; height: 2px; z-index: 5; pointer-events: none;
    background: linear-gradient(90deg, transparent, var(--scene) 45%, #fff 50%, var(--scene) 55%, transparent);
    background-size: 40% 100%; background-repeat: no-repeat;
    animation: sweep 0.9s cubic-bezier(0.4, 0, 0.2, 1) forwards;
  }
  @keyframes sweep {
    from { background-position: -60% 0; opacity: 1; }
    80% { opacity: 1; }
    to { background-position: 160% 0; opacity: 0; }
  }
  .scene { grid-area: video; display: grid; grid-template: "video" minmax(0, 1fr) / minmax(0, 1fr); min-width: 0; min-height: 0; }
  .video { grid-area: video; background: #000; display: grid; place-items: center; min-width: 0; position: relative; overflow: hidden; }
  .stage { position: absolute; inset: 0; }
  /* Film bitti: afiş yoksa addan renk; mpv gizli. */
  .end { position: absolute; inset: 0; display: grid; grid-template-columns: minmax(160px, 280px) minmax(0, 1fr); gap: 48px; align-items: center; padding: 0 64px; overflow: hidden; background: #08090b; }
  .end-bd { position: absolute; inset: -40px; background: linear-gradient(165deg, hsl(var(--h) 60% 45%), hsl(var(--h) 60% 18%) 55%, #0d0605); filter: blur(70px) saturate(1.3); opacity: 0.6; }
  .end-bd::after { content: ""; position: absolute; inset: 0; background: linear-gradient(90deg, rgba(8, 9, 11, 0.3), rgba(8, 9, 11, 0.9) 55%); }
  .end-poster {
    position: relative; aspect-ratio: 2 / 3; border-radius: 14px; display: flex; align-items: flex-end; padding: 24px;
    background: linear-gradient(165deg, hsl(var(--h) 60% 45%), hsl(var(--h) 60% 18%) 55%, #0d0605);
    box-shadow: 0 40px 90px rgba(0, 0, 0, 0.7), 0 0 0 1px rgba(255, 255, 255, 0.1);
    font-weight: 900; font-size: 34px; letter-spacing: -0.03em; line-height: 0.95; overflow-wrap: anywhere;
  }
  .end-info { position: relative; display: flex; flex-direction: column; gap: 16px; min-width: 0; }
  .end-k { color: var(--text-2); font-size: 12px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em; }
  .end-info h2 { margin: 0; font-size: 46px; font-weight: 900; letter-spacing: -0.04em; line-height: 1.05; overflow-wrap: anywhere; }
  .end-stats { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; max-width: 520px; }
  .end-stats div { padding: 12px 14px; border-radius: 14px; background: rgba(255, 255, 255, 0.045); border: 1px solid var(--line); }
  .end-stats b { display: block; font-size: 22px; font-weight: 850; letter-spacing: -0.02em; }
  .end-stats small { color: var(--text-2); font-size: 11.5px; }
  .end-row { display: flex; flex-wrap: wrap; gap: 10px; margin-top: 4px; }
  .big { height: 52px; padding: 0 22px; border-radius: 15px; border: 0; font-weight: 800; font-size: 14.5px; display: inline-flex; align-items: center; gap: 10px; white-space: nowrap; }
  .big.w { background: #fff; color: #000; }
  .big.w:hover { background: #e8e8ed; }
  .big.g { background: var(--glass-2); border: 1px solid var(--line-2); }
  .empty { display: flex; flex-direction: column; align-items: center; gap: 6px; text-align: center; padding: 24px; }
  .empty-logo {
    width: 72px; height: 72px; border-radius: 20px; display: grid; place-items: center; margin-bottom: 10px;
    background: linear-gradient(180deg, #1b1c20, #111215); border: 1px solid var(--line-2); box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
  }
  .empty b { font-size: 16px; font-weight: 600; letter-spacing: -0.01em; }
  .empty > span { color: var(--text-2); max-width: 340px; }
  .ring { position: relative; width: 120px; height: 120px; margin-bottom: 12px; }
  .ring .arc { stroke-dasharray: calc(var(--p) * 1) 100; transition: stroke-dasharray 0.4s; }
  .ring span { position: absolute; inset: 0; display: grid; place-items: center; font-size: 26px; font-weight: 650; letter-spacing: -0.02em; color: var(--text); max-width: none; }
  .empty-logo.bad { color: var(--red); }
  .empty-logo.spin { animation: pulse 1.4s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.5; } }
  .hints { display: flex; gap: 8px; margin-top: 14px; }
  .prov { width: min(520px, 100%); max-height: 46vh; overflow: auto; margin-top: 12px; text-align: left; }
  .hints span { display: inline-flex; align-items: center; gap: 6px; height: 26px; padding: 0 12px; border-radius: 99px; border: 1px solid var(--line-2); color: var(--text-2); font-size: 12px; }
  kbd { font: 500 11px var(--font-mono); padding: 1px 5px; border-radius: 5px; border: 1px solid var(--line-2); color: var(--text-2); }

  .pill {
    display: inline-flex; align-items: center; gap: 7px; height: 24px; padding: 0 12px; border-radius: 99px;
    font-size: 12px; font-weight: 500; background: var(--glass); border: 1px solid var(--line-2); white-space: nowrap;
  }
  .pill i { width: 6px; height: 6px; border-radius: 50%; background: var(--text-3); }
  .pill.live i { background: var(--green); box-shadow: 0 0 8px var(--green); }
  .pill.warn i { background: var(--yellow); box-shadow: 0 0 8px var(--yellow); }
  .pill.bad i { background: var(--red); box-shadow: 0 0 8px var(--red); }
  .pill-sub { color: var(--text-2); }
  .chip { display: inline-flex; align-items: center; gap: 6px; height: 28px; padding: 0 9px; border-radius: 8px; font-size: 12px; background: var(--glass); border: 1px solid var(--line-2); }

  /* Sağ ray ve çekmece (Yön C). */
  .rail {
    grid-area: rail; background: var(--panel); border-left: 1px solid var(--line);
    display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 12px 0; min-height: 0;
  }
  .ri {
    width: 40px; height: 40px; border-radius: 12px; padding: 0; border: 0; background: transparent;
    display: grid; place-items: center; color: var(--text-2); position: relative; flex: none;
  }
  .ri { transition: background 0.2s, color 0.2s, transform 0.15s; }
  .ri:hover { background: var(--glass-2); color: var(--text); }
  .ri:active { transform: scale(0.92); }
  .ri.on { background: var(--glass-3); color: var(--text); }
  /* Açık aracın solunda sahne renginde ışıklı çizgi. */
  .ri::before {
    content: ""; position: absolute; left: -12px; top: 50%; width: 3px; height: 0; border-radius: 0 3px 3px 0;
    background: var(--scene); box-shadow: 0 0 10px var(--scene-glow); transform: translateY(-50%);
    transition: height 0.3s cubic-bezier(0.34, 1.4, 0.5, 1);
  }
  .ri.on::before { height: 22px; }
  .ri.on :global(svg) { color: var(--scene); filter: drop-shadow(0 0 6px var(--scene-glow)); }
  .ri.danger::before, .ri.end::before { display: none; }
  .ri.danger { margin-top: auto; color: #ff6961; }
  .ri.end { margin-top: auto; }
  .cnt {
    position: absolute; top: 2px; right: 0; min-width: 16px; height: 16px; padding: 0 4px; border-radius: 8px;
    background: var(--accent); color: #fff; font-size: 10px; font-weight: 700; display: grid; place-items: center;
  }
  .cnt.warn { background: var(--yellow); color: #000; }
  .rail-sep { width: 32px; height: 1px; background: var(--line); margin: 4px 0; }
  .stack { display: flex; flex-direction: column; gap: 10px; min-height: 0; overflow: hidden; padding: 4px; }
  .stack .avatar::before {
    content: ""; position: absolute; inset: -4px; border-radius: 50%; --c: var(--green);
    background: conic-gradient(var(--c) calc(var(--v) * 1%), rgba(255, 255, 255, 0.12) 0);
    -webkit-mask: radial-gradient(farthest-side, transparent calc(100% - 2.5px), #000 calc(100% - 2px)); mask: radial-gradient(farthest-side, transparent calc(100% - 2.5px), #000 calc(100% - 2px));
  }
  .stack .avatar.warn::before { --c: #ff9f0a; }
  .stack .avatar.off { opacity: 0.45; }
  .stack.dim { opacity: 0.45; }
  .stack .avatar { border: 0; padding: 0; cursor: pointer; }

  .drawer {
    grid-area: drawer; background: var(--panel); border-left: 1px solid var(--line);
    min-height: 0; min-width: 0; overflow: hidden; position: relative;
    opacity: 0; transition: opacity var(--drawer-ms) ease;
  }
  .drawer.open { opacity: 1; }
  /* Sütun 0→320 açılırken içerik sabit genişlikte kalır, raydan kayarak çıkar. */
  .d-inner {
    width: var(--drawer-w); height: 100%; display: flex; flex-direction: column;
    transform: translateX(24px); transition: transform var(--drawer-ms) cubic-bezier(0.32, 0.72, 0, 1);
  }
  .drawer.open .d-inner { transform: none; }
  .d-head { display: flex; align-items: center; gap: 8px; padding: 16px; border-bottom: 1px solid var(--line); }
  .d-head > div { flex: 1; min-width: 0; }
  .d-body { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 10px; padding: 16px; animation: d-in 0.18s ease-out; }
  @keyframes d-in { from { opacity: 0; transform: translateX(14px); } }

  .card { background: var(--glass); border: 1px solid var(--line); border-radius: 14px; padding: 12px; display: flex; flex-direction: column; gap: 8px; flex: none; }
  .eyebrow { font-size: 11px; font-weight: 600; color: var(--text-3); text-transform: uppercase; letter-spacing: 0.06em; }
  h2 { margin: 2px 0 0; font-size: 17px; font-weight: 650; letter-spacing: -0.015em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mono { font-family: var(--font-mono); }
  .hint { font-size: 12px; color: var(--text-3); }
  .tsec { display: flex; flex-direction: column; gap: 8px; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .tchip { height: 28px; padding: 0 12px; border-radius: 99px; border: 1px solid var(--line-2); background: transparent; color: var(--text-2); font-size: 12.5px; }
  .tchip:hover { background: var(--glass-2); }
  .topt { display: flex; align-items: center; gap: 10px; padding: 9px 11px; border-radius: 12px; text-align: left; height: auto; }
  .topt .rd { width: 16px; height: 16px; border-radius: 50%; border: 2px solid var(--text-3); flex: none; transition: border 0.15s; }
  .topt.on { border-color: rgba(10, 132, 255, 0.6); background: rgba(10, 132, 255, 0.12); }
  .topt.on .rd { border: 5px solid var(--accent); }
  .delay { display: flex; align-items: center; gap: 6px; padding: 10px 12px; border-radius: 12px; background: var(--glass); border: 1px solid var(--line); }
  .delay > span { flex: 1; }
  .delay b { font: 600 13px var(--font-mono); min-width: 72px; text-align: center; }
  .tchip.on { background: var(--text); color: #000; border-color: transparent; font-weight: 550; }
  .row { display: flex; gap: 8px; align-items: center; min-width: 0; }
  .row input { flex: 1; min-width: 0; }
  .block { width: 100%; }
  button.small { padding: 4px 10px; font-size: 12px; border-radius: 8px; margin-left: auto; }

  .opt {
    display: flex; align-items: center; gap: 12px; padding: 8px 10px; border-radius: 12px; text-align: left;
    background: var(--glass); border: 1px solid var(--line); flex: none;
  }
  .opt.static { cursor: default; }
  .opt-ic { width: 30px; height: 30px; border-radius: 9px; display: grid; place-items: center; background: var(--glass-2); color: var(--text-2); flex: none; }
  .opt-txt { display: flex; flex-direction: column; min-width: 0; }
  .opt-txt b { font-weight: 550; }
  .opt-txt small { color: var(--text-3); font-size: 11.5px; }

  .switch { flex-direction: row; align-items: center; gap: 10px; cursor: pointer; }
  .switch span { flex: 1; display: flex; flex-direction: column; }
  .switch small { color: var(--text-3); font-size: 12px; }

  .banner { padding: 12px; border-radius: 12px; display: flex; flex-direction: column; gap: 8px; font-size: 12.5px; flex: none; }
  .banner.blue { background: rgba(10, 132, 255, 0.08); border: 1px solid rgba(10, 132, 255, 0.25); }

  .now { display: flex; flex-direction: column; gap: 4px; min-width: 0; padding: 12px; border-radius: 14px; background: var(--glass); border: 1px solid var(--line); }
  .now-title { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tag { align-self: flex-start; font-size: 11px; font-weight: 600; padding: 2px 7px; border-radius: 6px; background: var(--glass-2); color: var(--text-2); }
  .tag.ok { background: rgba(48, 209, 88, 0.12); color: var(--green); }
  .tag.bad { background: rgba(255, 69, 58, 0.12); color: var(--red); }

  ul { list-style: none; margin: 0; padding: 0; }
  .members { display: flex; flex-direction: column; gap: 6px; }
  .members li { display: flex; flex-direction: column; gap: 8px; padding: 8px 10px; border-radius: 12px; background: var(--glass); border: 1px solid var(--line); }
  .members li.warn { border-color: rgba(255, 214, 10, 0.25); }
  .member { display: flex; align-items: center; gap: 8px; min-width: 0; min-height: 28px; }
  .avatar {
    flex: none; width: 28px; height: 28px; border-radius: 9px; display: grid; place-items: center; position: relative;
    font-weight: 650; font-size: 12px; color: #fff; background: hsl(var(--h) 55% 45% / 0.9);
  }
  .stack .avatar { border-radius: 50%; }
  .avimg { position: absolute; inset: 0; width: 100%; height: 100%; border-radius: inherit; object-fit: cover; }
  .dot { position: absolute; right: -2px; bottom: -2px; width: 9px; height: 9px; border-radius: 50%; background: var(--green); border: 2px solid var(--panel); }
  .dot.warn { background: var(--yellow); }
  .dot.off { background: var(--text-3); }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 550; min-width: 0; }
  .badge { flex: none; font-size: 10px; font-weight: 600; padding: 1px 6px; border-radius: 5px; background: var(--glass-2); color: var(--text-2); border: 1px solid var(--line); }
  .badge.host { color: #ffcc66; background: rgba(255, 204, 102, 0.1); border-color: rgba(255, 204, 102, 0.2); }
  .you { flex: none; font-size: 11px; color: var(--text-3); }
  .member .num { margin-left: auto; white-space: nowrap; font-size: 12px; }
  .warn-t { color: var(--yellow) !important; }
  .members li.away .name { color: var(--text-3); text-decoration: line-through; }
  .sub { display: flex; align-items: center; gap: 8px; font-size: 11.5px; }
  .sub .meter { flex: 1; }
  .sub.bad .num { color: var(--red); }
  .meter { height: 4px; border-radius: 4px; background: rgba(255, 255, 255, 0.08); overflow: hidden; }
  .meter i { display: block; height: 100%; border-radius: 4px; background: var(--accent); transition: width 0.3s; }
  .meter.ok i { background: var(--green); }
  .meter.warn i { background: var(--yellow); }
  .num { font-variant-numeric: tabular-nums; color: var(--text-2); }

  .stats { padding: 0; gap: 0; }
  .stats summary { padding: 10px 12px; cursor: pointer; font-size: 12px; color: var(--text-2); list-style: none; }
  .stats summary::before { content: "›"; display: inline-block; width: 12px; transition: transform 0.2s; }
  .stats[open] summary::before { transform: rotate(90deg); }
  .stats dl { margin: 0; padding: 0 12px 12px; display: grid; grid-template-columns: auto 1fr; gap: 4px 12px; font-size: 12px; font-family: var(--font-mono); }
  .stats dt { color: var(--text-2); }
  .stats dd { margin: 0; text-align: right; font-variant-numeric: tabular-nums; }
  .stats .bad { color: var(--red); }
  .path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .log { display: flex; flex-direction: column; gap: 8px; font-size: 12px; color: var(--text-2); }
  .log li { word-break: break-word; }
  .log li span { font: 500 11px var(--font-mono); color: var(--text-3); margin-right: 8px; }
  .log li.err { color: var(--red); }

  /* Kontrol şeridi: video altında tam genişlik, siyah. */
  .bar {
    grid-area: bar; background: #000; border-top: 1px solid rgba(255, 255, 255, 0.05);
    display: flex; align-items: center; gap: 12px; padding: 0 16px; min-width: 0; min-height: 0; overflow: hidden;
    transition: opacity 0.3s, transform 0.36s cubic-bezier(0.32, 0.72, 0, 1);
  }
  .bar.gone { opacity: 0; transform: translateY(16px); }
  .bar { gap: 18px; background: linear-gradient(180deg, #0c0d10, #08090b); }
  .np { flex: 0 1 240px; min-width: 0; display: flex; align-items: center; gap: 10px; }
  .pst { width: 30px; height: 44px; border-radius: 5px; flex: none; background: linear-gradient(165deg, hsl(var(--h) 60% 45%), hsl(var(--h) 60% 12%)); box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.1); }
  .now-t { display: flex; flex-direction: column; min-width: 0; line-height: 1.3; }
  .now-t b { font-size: 13px; font-weight: 650; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .now-t small { color: var(--text-2); font-size: 11.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .mid { flex: 1; min-width: 160px; display: flex; align-items: center; gap: 12px; }
  .ctl { display: flex; align-items: center; gap: 4px; flex: none; }
  .cb { position: relative; width: 34px; height: 34px; padding: 0; border: 0; border-radius: 10px; background: none; display: grid; place-items: center; color: var(--text-2); }
  .cb:hover:not(:disabled) { color: var(--text); background: var(--glass-2); }
  .cb small { position: absolute; top: 12.5px; font-size: 7.5px; font-weight: 800; }
  .right { flex: none; display: flex; align-items: center; gap: 6px; }
  .chip2 { height: 30px; padding: 0 10px; border-radius: 9px; display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-2); white-space: nowrap; }
  .chip2 b { color: var(--text); font-weight: 650; }
  .chip2 .sl { max-width: 90px; overflow: hidden; text-overflow: ellipsis; }
  .chip2.sync { color: var(--green); border-color: rgba(48, 209, 88, 0.35); background: rgba(48, 209, 88, 0.08); }
  .chip2.on { border-color: rgba(10, 132, 255, 0.6); background: rgba(10, 132, 255, 0.12); }
  /* Çekmece açıkken şerit sadeleşir: durum çipleri gizlenir. */
  .bar.narrow .info, .bar.narrow .np { display: none; }
  .bar .seek { --b: 0%; }
  .bar .seek::-webkit-slider-runnable-track {
    background: linear-gradient(to right, #fff var(--p), rgba(255, 255, 255, 0.3) var(--p), rgba(255, 255, 255, 0.3) max(var(--p), var(--b)), rgba(255, 255, 255, 0.14) max(var(--p), var(--b)));
  }
  .who { position: absolute; bottom: 19px; display: flex; transform: translateX(-50%); pointer-events: none; padding-left: 4px; transition: left 0.25s linear; }
  .who .av, .bring .av {
    width: 16px; height: 16px; margin-left: -4px; border-radius: 50%; display: grid; place-items: center; font-size: 8px; font-weight: 700; color: #fff;
    background: hsl(var(--h) 55% 45%); border: 1.5px solid #0a0b0d;
  }
  /* Bekleme bandı */
  .bar.hold { gap: 16px; padding: 0 18px; background: linear-gradient(90deg, rgba(255, 159, 10, 0.2), rgba(255, 159, 10, 0.06) 60%, #0b0c0e); border-top: 1px solid rgba(255, 159, 10, 0.4); }
  .sp { flex: none; width: 26px; height: 26px; border-radius: 50%; border: 3px solid rgba(255, 159, 10, 0.25); border-top-color: #ff9f0a; animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .h-t { display: flex; flex-direction: column; min-width: 0; line-height: 1.3; }
  .h-t b { font-size: 14px; white-space: nowrap; }
  .h-t small { color: var(--text-2); font-size: 12px; white-space: nowrap; }
  .bufs { display: flex; gap: 14px; margin-left: 18px; min-width: 0; overflow: hidden; }
  .bf { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-2); white-space: nowrap; }
  .bf b { color: #ff9f0a; font-size: 12px; }
  .bring { --v: 100; --c: var(--green); width: 30px; height: 30px; border-radius: 50%; display: grid; place-items: center; flex: none; background: conic-gradient(var(--c) calc(var(--v) * 1%), rgba(255, 255, 255, 0.1) 0); }
  .bring.warn { --c: #ff9f0a; }
  .bring .av { width: 25px; height: 25px; margin: 0; font-size: 10px; border: 2px solid #1a140a; }
  .h-act { margin-left: auto; display: flex; gap: 8px; flex: none; }
  /* Müzikte Lounge çekmecesi sahnenin renkli zeminine oturan buzlu cam. */
  .cockpit[data-scene="listen"] .drawer {
    z-index: 1; background: rgba(255, 255, 255, 0.035); border-left-color: rgba(255, 255, 255, 0.06);
    backdrop-filter: blur(28px) saturate(1.3);
  }
  .cockpit[data-scene="listen"] .d-head { border-bottom-color: transparent; }
  .cockpit[data-scene="listen"] .drawer :global(.seg) { background: rgba(0, 0, 0, 0.25); border-color: rgba(255, 255, 255, 0.06); }
  /* Müzikte panel başlıkları renkli zeminde kaybolmasın. */
  .cockpit[data-scene="listen"] .drawer :global(.eyebrow) { color: rgba(255, 255, 255, 0.72); font-weight: 750; letter-spacing: 0.1em; }
  .cockpit[data-scene="listen"] .drawer h2 { font-size: 20px; font-weight: 800; letter-spacing: -0.025em; }

  /* KONİ müzik çalar: kapağın yanında, dalga formlu. */
  .mp { display: flex; flex-direction: column; gap: 14px; margin-top: 18px; width: min(560px, 100%); }
  .mp-row { display: flex; align-items: center; gap: 14px; }
  .mp-ic { flex: none; width: 40px; height: 40px; padding: 0; border: 0; border-radius: 50%; background: rgba(255, 255, 255, 0.08); display: grid; place-items: center; color: #fff; transition: background 0.2s, transform 0.15s, opacity 0.2s; }
  .mp-ic:hover:not(:disabled) { background: rgba(255, 255, 255, 0.16); transform: scale(1.06); }
  .mp-ic:disabled { opacity: 0.3; }
  .mp-ic.heart.on { color: #ff4f8b; }
  .mp-ic.heart.on :global(path) { fill: currentColor; }
  .mp-play {
    flex: none; width: 60px; height: 60px; border-radius: 50%; border: 0; padding: 0; display: grid; place-items: center;
    background: #fff; color: #000; box-shadow: 0 10px 30px rgba(0, 0, 0, 0.45), 0 0 34px -6px var(--tint, #fff);
    transition: transform 0.2s cubic-bezier(0.34, 1.5, 0.5, 1);
  }
  .mp-play:hover { transform: scale(1.08); }
  .mp-play:active { transform: scale(0.94); }
  .mp-vol { margin-left: auto; display: flex; align-items: center; gap: 8px; color: rgba(255, 255, 255, 0.75); }
  .mp-vol .vol { width: 90px; }
  .mp-seek { display: flex; align-items: center; gap: 10px; }
  .mp-seek .seekwrap { height: 44px; }
  .mp-seek .wave { height: 40px; }
  .mp-seek .wave rect { fill: rgba(255, 255, 255, 0.25); transition: fill 0.2s; }
  .mp-seek .wave rect.played { fill: #fff; }
  .mp-seek .wave + .seek { opacity: 0; }
  .mp-seek .time { font-size: 11.5px; color: rgba(255, 255, 255, 0.7); }
  .mp.idle .mp-seek { opacity: 0.35; pointer-events: none; }
  .play {
    flex: none; width: 36px; height: 36px; border-radius: 50%; padding: 0; display: grid; place-items: center;
    background: #f5f5f7; color: #000; border: none;
  }
  .play:hover { background: #fff; transform: scale(1.05); }
  .time { font: 500 12px var(--font-mono); white-space: nowrap; color: var(--text-2); min-width: 5ch; text-align: center; transition: color 0.12s; }
  .time.hover { color: #409cff; }
  .bar.idle .seekwrap, .bar.idle .time { opacity: 0.35; pointer-events: none; }
  .mute { background: none; border: 0; padding: 0; margin: 0; display: grid; place-items: center; color: inherit; cursor: pointer; }
  .mute:hover { color: var(--text); }
  .seekwrap { flex: 1; min-width: 60px; position: relative; display: flex; align-items: center; }
  .seek { flex: 1; min-width: 0; position: relative; }
  .wave { position: absolute; left: 0; right: 0; top: 50%; height: 26px; width: 100%; transform: translateY(-50%); pointer-events: none; }
  .wave rect { fill: rgba(255, 255, 255, 0.22); }
  .wave rect.played { fill: rgba(255, 255, 255, 0.6); }
  .capsule {
    display: inline-flex; align-items: center; gap: 8px; height: 34px; padding: 0 6px 0 12px; border-radius: 10px; font-size: 12px; white-space: nowrap; flex: none;
  }
  .capsule.warn { background: rgba(255, 214, 10, 0.1); border: 1px solid rgba(255, 214, 10, 0.25); color: var(--yellow); }
  .capsule button { margin: 0; }
  .volume { display: flex; align-items: center; gap: 8px; height: 30px; padding: 0 10px; border-radius: 99px; background: var(--glass-2); color: var(--text-2); flex: none; }
  .vol { width: 64px; }
  select { padding: 5px 8px; border-radius: 8px; font-size: 12px; flex: none; }
  .icon { flex: none; width: 30px; height: 30px; padding: 0; display: grid; place-items: center; border-radius: 9px; color: var(--text-2); }
  /* Odadan çıkarken (ana-ekran-v2 mockup) */
  .leave-scrim { position: fixed; z-index: 60; inset: var(--top-bar) 0 0 0; background: rgba(0, 0, 0, 0.6); backdrop-filter: blur(6px); display: grid; place-items: center; }
  .leave { width: 460px; padding: 24px; border-radius: 22px; background: #16171a; border: 1px solid var(--line-2); box-shadow: 0 40px 100px rgba(0, 0, 0, 0.7); display: flex; flex-direction: column; gap: 12px; }
  .leave .k { color: var(--text-2); font-size: 11.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em; }
  .leave h2 { margin: 0 0 4px; font-size: 24px; font-weight: 900; letter-spacing: -0.03em; }
  .leave .opt { display: flex; align-items: center; gap: 12px; padding: 12px 14px; height: auto; border-radius: 14px; background: var(--glass); border: 1px solid var(--line-2); text-align: left; }
  .leave .opt:hover { border-color: rgba(48, 209, 88, 0.5); background: rgba(48, 209, 88, 0.08); }
  .leave .opt.red:hover { border-color: rgba(255, 105, 97, 0.5); background: rgba(255, 105, 97, 0.08); }
  .leave .opt .ic { width: 36px; height: 36px; border-radius: 11px; display: grid; place-items: center; font-size: 17px; background: var(--glass-2); flex: none; }
  .leave .opt b { display: block; font-size: 14px; }
  .leave .opt small { color: var(--text-2); font-size: 12px; font-weight: 500; }
  .leave .opt.red b { color: #ff6961; }
  .leave .cancel { align-self: flex-end; }
</style>
