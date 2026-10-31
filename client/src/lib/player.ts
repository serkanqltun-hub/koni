import { invoke } from "@tauri-apps/api/core";

export interface PlayerState {
  timePos: number | null;
  duration: number | null;
  paused: boolean;
  speed: number;
  seeking: boolean;
  pausedForCache: boolean;
  eofReached: boolean;
}

export type LayoutMode = "cockpit" | "fullscreen" | "hidden";

export interface LayoutConfig {
  mode: LayoutMode;
  rightColDip: number;
  bottomBarDip: number;
  gapDip: number;
  /** Özel başlık çubuğunun yüksekliği; video bunun altından başlar. */
  topBarDip: number;
}

export const COCKPIT_LAYOUT: LayoutConfig = {
  mode: "cockpit",
  rightColDip: 64,
  bottomBarDip: 64,
  gapDip: 0,
  topBarDip: 40,
};

/**
 * Döner: gösterilecek başlık. Odadayken web adresi odaya gönderilir.
 * `null`: yt-dlp siteyi tanımıyor; Keşfet sayfada oynatır (tarayıcıda senkron).
 */
export const playerLoad = (src: string) => invoke<string | null>("player_load", { src });
/** Sanal Yakalayıcıyı kapat (vazgeç). */
export const snifferClose = () => invoke<void>("sniffer_close");

/** Yeni sürüm (`update_check`); `portable`: zip kopya, kendini güncelleyemez. */
export interface UpdateInfo { version: string; notes: string | null; portable: boolean }
export const updateCheck = () => invoke<UpdateInfo | null>("update_check");
/** İndirir, doğrular, kurar; başarılıysa uygulama kapanıp yeni sürümle açılır. */
export const updateInstall = () => invoke<void>("update_install");

/** Arkadaşlar (PROTOCOL §3.0c). `name` "Ad#1234"; `mutual`: o da seni ekledi. */
export interface Friend { name: string; avatar: string; mutual: boolean; admin?: boolean }
export interface Friends { friends: Friend[]; requests: Friend[] }
/** Hesap yoksa `null`. `decline`: sana gelen isteği sil. */
export const friendsCall = (url: string, op: "list" | "add" | "remove" | "decline", user = "") =>
  invoke<Friends | null>("friends", { url, op, user });

/** Profil (PROTOCOL §3.0d, p0.18). `daily`: son 7 günün izleme saniyesi, bugün en sonda. */
export interface InviteInfo { expires: number; usedBy?: string; avatar?: string }
export interface Profile {
  name: string; tag: string; avatar: string; avatars: string[]; invites: InviteInfo[]; daily: number[];
  /** p0.19: admin mi, davet hakkı var mı, adminlerin kullandığı (seçilemeyen) fotoğraflar. */
  admin?: boolean; canInvite?: boolean; taken?: string[];
  /** Yalnızca `op: "invite"` cevabında: yeni kod. */
  code?: string;
}
/** Hesap yoksa `null`. */
export const profileCall = (url: string, op: "get" | "avatar" | "invite" | "upload" = "get", avatar?: string, image?: string) =>
  invoke<Profile | null>("profile", { url, op, avatar, image });

/** Yönetim (PROTOCOL §3.0f, p0.19, yalnızca admin). */
export interface AdminSeat { name: string; avatar?: string; host?: boolean; admin?: boolean }
export interface AdminRoom { id: string; name: string; view: string; title?: string; since: number; members: AdminSeat[] }
export interface AdminMember {
  name: string; avatar: string; admin?: boolean; canInvite: boolean; invitedBy?: string;
  created: number; friends: number; online: boolean; room?: string;
}
export interface AdminState { rooms: AdminRoom[]; members: AdminMember[]; now: number }
export const adminCall = (url: string, op: "list" | "close" | "ban" | "invite_right" = "list", o: { room?: string; user?: string; on?: boolean } = {}) =>
  invoke<AdminState>("admin", { url, op, room: o.room ?? null, user: o.user ?? null, on: o.on ?? null });
/** Profil fotoğrafı `data:` adresi (oturum ister, uygulama içinde önbelleklenir). */
export const avatarGet = (url: string, id: string) => invoke<string>("avatar_get", { url, id });

/** Lobi (PROTOCOL §3.0e): arkadaşların durumu ve odaya çağrılar `lobby://msg` ile gelir. */
export interface PresenceFriend extends Friend { online: boolean; room?: string; title?: string; /** p0.22 */ invite?: string }
export interface RoomCall { from: string; avatar: string; room: string; invite: string; at: number }
/** p0.22 ana ekran "Açık odalar": geri dönebileceğin ya da arkadaşının içinde olduğu oda. */
export interface OpenRoom {
  invite: string; room: string; title?: string; view: RoomView; night?: boolean; playing?: boolean;
  pos?: number; dur?: number; members: string[]; mine?: boolean; left?: number;
}
export interface Presence { friends: PresenceFriend[]; requests: Friend[]; calls: RoomCall[]; rooms?: OpenRoom[] }
export const lobbyStart = (url: string) => invoke<void>("lobby_start", { url });
export const lobbyDecline = (invite: string) => invoke<void>("lobby_decline", { invite });

/** Karşılıklı arkadaşların toplamları (PROTOCOL §3.0 `stats`, docs/design/accounts.md §5.4). */
export interface UserStats { user: string; avatar?: string; days?: number; watchSec: number; nightSec: number; longestSec: number; pauses: number; seeks: number; holds: number; listenSec?: number }
export interface Badge { id: string; title: string; rule: string; users: string[] | null }
export interface Stats { period: string; you: string; users: UserStats[] | null; badges: Badge[] }
export type StatsPeriod = "week" | "month" | "all";
/** Hesap yoksa `null`. */
export const statsGet = (url: string, period: StatsPeriod) => invoke<Stats | null>("stats_get", { url, period });
/** Ana ekran "Etkinlik" (p0.22). */
export interface FeedItem { kind: "watch" | "joined"; at: number; users: string[]; title?: string; room?: string; sec?: number; by?: string }
export const feedGet = (url: string) => invoke<{ you: string; items: FeedItem[] } | null>("feed_get", { url });
/** Profil listeleri (p0.23, PROTOCOL §3.0g): "want" İzleyeceklerim, "seen" İzlediklerim. */
export type ListName = "want" | "seen";
export interface ListEntry {
  ref: string; kind: "movie" | "series"; title: string; year?: number; poster?: string; at?: number;
  /** seen: 1 ❤️, -1 👎 */ rating?: number; /** seen: birlikte izlenenler */ with?: string[]; source?: string;
}
export interface Holder { name: string; avatar?: string; rating?: number }
export interface WantEntry extends ListEntry { mates?: Holder[] }
export interface Lists { user: string; avatar?: string; want: WantEntry[]; seen: ListEntry[]; common?: string[]; together?: number }
export interface ListWho { ref: string; want: Holder[] | null; seen: Holder[] | null; mine: { want?: boolean; seen?: boolean; rating?: number } }
const listsCall = <T>(url: string, op: string, o: { user?: string; list?: ListName; reference?: string; rating?: number; entry?: ListEntry } = {}) =>
  invoke<T | null>("lists_call", { url, op, user: o.user ?? null, list: o.list ?? null, reference: o.reference ?? null, rating: o.rating ?? null, entry: o.entry ?? null });
/** Kendi listelerin (`user` boş) ya da karşılıklı arkadaşınınkiler. Hesap yoksa `null`. */
export const listsGet = (url: string, user?: string) => listsCall<Lists>(url, "", { user });
export const listsAdd = (url: string, list: ListName, entry: ListEntry) => listsCall<Lists>(url, "add", { list, entry });
export const listsRemove = (url: string, list: ListName, ref: string) => listsCall<Lists>(url, "remove", { list, reference: ref });
export const listsRate = (url: string, ref: string, rating: number) => listsCall<Lists>(url, "rate", { reference: ref, rating });
export const listsWho = (url: string, ref: string) => listsCall<ListWho>(url, "who", { reference: ref });
/** "Sorun bildir": not + etkinlik logu (eskiden yeniye) + son senkron CSV'si sunucuya gider; döner: rapor adı. */
export const bugReport = (url: string, username: string, note: string, log: string[]) =>
  invoke<string>("bug_report", { url, username, note, log });

// ---- Keşif, çark, altyazı, kuyruk (PROTOCOL §3.9–3.11, §4.12–4.16) -------------
export interface Ratings { tmdb?: number; imdb?: string; rt?: string; meta?: string }
export interface Movie {
  id: number; title: string; original?: string; year?: number; overview?: string; poster?: string;
  runtime?: number; genres?: string[]; imdb?: string; trailer?: string; ratings: Ratings;
  /** p0.17: "tv" dizi; `runtime` bölüm süresi, `director` yaratıcı. */
  kind?: "tv"; seasons?: number; votes?: number; director?: string; cast?: string[]; countries?: string[];
}
/** "Ne izlesek?" oturumu (PROTOCOL §3.9a). Tür id'leri TMDB film türleri. */
export interface NightFilter {
  genres?: number[]; without?: number[]; keywords?: string[];
  yearFrom?: number; yearTo?: number; runtimeMin?: number; runtimeMax?: number;
  voteMin?: number; voteCountMin?: number; languages?: string[]; hideSeen?: boolean;
}
export type NightKind = "movie" | "tv";
export type NightPhase = "setup" | "pick" | "vote" | "done";
export interface NightMember { id: string; name: string; ready: boolean; mood?: string; picks: number; done: boolean }
export interface Night {
  id: string; kind: NightKind; perUser: number; by: string; phase: NightPhase; deckId?: string;
  members: NightMember[]; pool: number; mine: Movie[]; pending: number;
}
export interface NightPreview { tag: string; total: number; posters: string[] }
export interface Mood { id: string; label: string; emoji: string }
export interface Proposal { id: string; by?: string; byName: string; item: Movie }
export interface WheelResult { winner: string; index: number; spinMs: number; turns: number; at: number }
export interface SubResult { fileId?: number; id?: string; url?: string; name: string; lang: string; downloads: number; hashMatch?: boolean; hi?: boolean; machine?: boolean }
export interface QueueItem { id: string; media: { kind: string; src: string; title: string }; by: string; byName: string; voters: string[] }
export type FunMsg =
  | { t: "moods"; moods: Mood[] }
  | { t: "deck_loading"; mood: string; by: string }
  | { t: "deck"; deckId: string; mood: string; by: string; items: Movie[]; over: boolean; progress: { done: number; of: number }; match?: number; mine?: Record<string, boolean> }
  | { t: "deck_progress"; deckId: string; done: number; of: number; over: boolean }
  | { t: "match"; deckId: string; item: Movie }
  | { t: "discover_failed"; message: string }
  | { t: "wheel"; proposals: Proposal[]; result?: WheelResult }
  | { t: "fun_error"; message: string }
  | { t: "subs_results"; lang: string; items?: SubResult[] | null; error?: string }
  | { t: "subtitle"; name: string; lang: string; text: string; by: string }
  | { t: "queue"; items: QueueItem[] }
  | { t: "view"; view: RoomView; by?: string }
  | { t: "reaction"; user: string; name: string; emoji: string }
  | ({ t: "night"; closed?: false } & Night) | { t: "night"; closed: true }
  | { t: "night_previews"; items: NightPreview[] }
  | { t: "night_suggestions"; items: Movie[]; total: number; error?: string }
  | { t: "night_results"; query: string; items: Movie[]; error?: string }
  | { t: "night_detail"; item: Movie; error?: string }
  | { t: "called"; user: string };
/** Hızlı tepkiler (PROTOCOL p0.14); sunucu başka değeri yok sayar. */
export const REACTIONS = ["😂", "😱", "😍", "👏", "😭", "🔥"] as const;
/** Odaya keşif/çark/altyazı mesajı. */
export const syncFun = (msg: Record<string, unknown>) => invoke<void>("sync_fun", { msg });
/** Oynatılan yerel dosyanın OpenSubtitles özeti; akışta `null`. */
export const subsHash = () => invoke<string | null>("subs_hash");
export const subsSearchEpisode = (file: string | null, title: string, lang: string) =>
  invoke<SubResult[]>("subs_search_episode", { file, title, lang });
export const subsApplyUrl = (url: string, name: string) =>
  invoke<string>("subs_apply_url", { url, name });
export const playerAddSubtitleText = (name: string, text: string) => invoke<string>("player_add_subtitle_text", { name, text });
/** Lounge kuyruğuna web adresi: yerelde çözülür, başlığı döner. `title` verilirse
 *  (çalma listesindeki şarkı) yeniden çözülmez. */
export const queueAdd = (src: string, title?: string) => invoke<string>("queue_add", { src, title: title ?? null });
/** Müzik: sıradaki şarkıyı şimdiden çöz (yt-dlp), sırası gelince beklemeden açılsın. */
export const mediaPrefetch = (src: string) => invoke<void>("media_prefetch", { src });
/** `playerLoad` çözülürken başka şey açıldı: yüklenmedi (hata gösterilmez). */
export const isSuperseded = (e: unknown) => String(e) === "superseded";
/** Müzikte seek çubuğu dalga formu (0–1 sütunlar); görüntülü medyada `null`. */
export const mediaWaveform = () => invoke<number[] | null>("media_waveform");
export const playerPlay = () => invoke<void>("player_play");
export const playerPause = () => invoke<void>("player_pause");
export const playerTogglePause = () => invoke<void>("player_toggle_pause");
export const playerSeek = (seconds: number) => invoke<void>("player_seek", { seconds });
export const playerSetSpeed = (speed: number) => invoke<void>("player_set_speed", { speed });
export const playerSetVolume = (volume: number) => invoke<void>("player_set_volume", { volume });
/** Döner: altyazı dosyasının adı. */
export const playerAddSubtitle = (path: string) => invoke<string>("player_add_subtitle", { path });
/** Döner: yeni altyazı gecikmesi (s). */
export const playerSubDelay = (delta: number) => invoke<number>("player_sub_delay", { delta });
/** Kalite · ses · altyazı seçeneği; `key`: mpv iz kimliği, `alt:N` ya da `no` (kapalı). */
export interface TrackItem { key: string; label: string; selected: boolean }
export interface Tracks { video: TrackItem[]; audio: TrackItem[]; sub: TrackItem[] }
export type TrackKind = "video" | "audio" | "sub";
export const playerTracks = () => invoke<Tracks>("player_tracks");
/** Müzik modu: çalan görüntülü dosyada video yerine görselleştirici. */
export const playerMusicView = (on: boolean) => invoke<void>("player_music_view", { on });
/** Seçim kişiseldir, odaya gitmez. */
export const playerSetTrack = (kind: TrackKind, key: string) => invoke<void>("player_set_track", { kind, key });
export const playerState = () => invoke<PlayerState>("player_state");
export const setLayout = (layout: LayoutConfig) => invoke<void>("set_layout", { layout });
export const setFullscreen = (on: boolean) => invoke<void>("set_fullscreen", { on });

// ---------------------------------------------------------------- Oda senkronu

export type RoomMode = "paused" | "countdown" | "playing" | "hold";

/** `sync://status` (4 Hz). */
export interface SyncStatus {
  me: string | null;
  localState: string;
  roomMode: RoomMode | null;
  localPos: number;
  roomPos: number | null;
  driftMs: number | null;
  speed: number;
  rttMs: number | null;
  offsetMs: number | null;
  cacheAhead: number;
  backwardBlocked: number;
  forwardSkips: number;
  stalls: number;
  csvPath: string | null;
}

export interface Member { id: string; name: string; host?: boolean; cohost?: boolean; away?: boolean; detached?: boolean; admin?: boolean }
export type Policy = "free" | "host_only";

/** `sync://link`: Santral bağlantısı koptu / geri geldi. */
export interface LinkEvent { state: "online" | "reconnecting"; attempt: number }

/** `sync://media`: odanın medyasının yerel durumu. */
export interface MediaEvent {
  /** `no_provider`: kişisel kaynaklı (`debrid`) medya ama bu bilgisayarda anahtar da eklenti de yok. */
  status: "resolving" | "downloading" | "ready" | "error" | "need_file" | "no_provider";
  /** `debrid`: `src` torrentin infoHash'i; herkes kendi Real-Debrid hesabıyla çözer. */
  kind: "url" | "file" | "download" | "debrid" | "page";
  src: string;
  title: string;
  /** `download`: görev kimliği (`dl_status.taskId`). */
  task?: string;
  /** `debrid`: torrentteki dosya ipucu. */
  file?: string;
  idx?: number;
  error?: string;
}

/** `sniffer://state`: Sanal Yakalayıcı penceresi. */
export interface SnifferEvent {
  status: "open" | "captured" | "closed" | "error";
  /** Yakalayıcıda açılan sayfa. */
  page: string;
  title?: string;
  error?: string;
}

/** "Herkes İndirsin": bir üyenin indirme durumu. */
export interface DlStatus {
  user: string;
  name: string;
  state: "queued" | "downloading" | "ready" | "failed";
  pct: number;
  speed: number;
  eta: number;
  error: string;
}

/** `sync://server`: docs/PROTOCOL.md §4 mesajları (pong hariç). */
export type ServerMsg =
  | { t: "room_created"; room: string; name: string; invite: string; you: string }
  | { t: "joined"; you: string; invite: string; resumed?: boolean; snapshot: { room: string; name: string; host: string; policy: Policy; view?: RoomView | ""; members: Member[]; downloads?: DlStatus[]; state: { media?: { task?: string } } } }
  | ({ t: "dl_status"; taskId: string } & DlStatus)
  | { t: "state"; mode: RoomMode; by?: { id: string; name: string } }
  | { t: "activity"; kind: string; text: string }
  | { t: "member_joined"; member: Member }
  | { t: "member_left"; id: string; host?: string }
  | { t: "member_away"; id: string }
  | { t: "member_back"; id: string }
  | { t: "member_detached"; id: string; detached: boolean }
  | { t: "member_role"; id: string; cohost: boolean }
  | { t: "policy"; policy: Policy }
  | { t: "detach_offer"; user: string; name: string; stalls: number }
  | { t: "detached"; by?: string }
  | { t: "member_status"; id: string; driftMs: number; state: string; bufferPct: number; rttMs?: number }
  | { t: "buffer_status"; user: string; name?: string; bufferPct: number }
  | { t: "denied"; ack: string; reason: string }
  | { t: "error"; code: string; message: string }
  | { t: "unknown" };

export const syncConnect = (url: string, username: string, roomName: string | null, invite: string | null) =>
  invoke<void>("sync_connect", { url, username, roomName, invite });
export const syncPlay = () => invoke<void>("sync_play");
export const syncPause = () => invoke<void>("sync_pause");
export const syncSeek = (seconds: number) => invoke<void>("sync_seek", { seconds });
/** Odada çalanı herkes için kapat (oda açık kalır, p0.21). */
export const syncClose = () => invoke<void>("sync_close");
/** Host: üyeyi yardımcı host yap / al. */
export const syncSetCohost = (user: string, on: boolean) => invoke<void>("sync_set_cohost", { user, on });
/** Host: yalnızca host (ve yardımcı hostlar) kontrol eder. */
export const syncSetPolicy = (hostOnly: boolean) => invoke<void>("sync_set_policy", { hostOnly });
export const syncLeave = () => invoke<void>("sync_leave");
export const syncContinueWithout = (user: string) => invoke<void>("sync_continue_without", { user });
export const syncResync = () => invoke<void>("sync_resync");
export const syncDownload = (url: string) => invoke<void>("sync_download", { url });

// ---------------------------------------------------------------- Hesap (PROTOCOL §3.0)

/** Bu sunucuda kayıtlı hesabın adı (oturum belirteci anahtar zincirinde). */
export const accountGet = (url: string) => invoke<string | null>("account_get", { url });
export const accountLogin = (url: string, username: string, password: string) =>
  invoke<string>("account_login", { url, username, password });
export const accountRegister = (url: string, code: string, username: string, password: string) =>
  invoke<string>("account_register", { url, code, username, password, avatar: null });
export const accountLogout = (url: string) => invoke<void>("account_logout", { url });

// ---------------------------------------------------------------- Oda modu, Keşfet (p0.13)

/** Odanın ortak ekranı: Keşfet (ne izlesek), Sinema, Müzik. */
export type RoomView = "browse" | "watch" | "listen";
export const syncSetView = (view: RoomView) => invoke<void>("sync_set_view", { view });
export interface SearchHit { url: string; title: string; channel?: string; durationSecs?: number; thumbnail?: string; live?: boolean }
/** YouTube araması (yt-dlp). */
export const webSearch = (query: string) => invoke<SearchHit[]>("web_search", { query });
/** Linkteki şarkılar: YouTube/SoundCloud çalma listesi ya da tek video. */
export const mediaList = (src: string) => invoke<SearchHit[]>("media_list", { src });
/** "Siteye git": sayfa yakalayıcıda açılır (Keşfet alanı ayarlıysa video alanına gömülü). */
export const browseOpen = (page: string) => invoke<void>("browse_open", { page });
export interface EmbedRect { x: number; y: number; w: number; h: number }
/** Gömülü tarayıcının alanı (CSS px); `null`: ayrı pencere. */
export const browseEmbed = (rect: EmbedRect | null) => invoke<void>("browse_embed", { rect });
export const browseNav = (action: "back" | "forward" | "reload") => invoke<void>("browse_nav", { action });
/** Keşfet tarayıcısı: açık sayfa, sayfadaki ana video ve bu sayfada engellenen reklam. */
export interface BrowseState { url: string | null; title: string; video: { pos: number; dur: number | null; paused: boolean } | null; blocked: number }
export const browseState = () => invoke<BrowseState>("browse_state");
/** Tarayıcıda senkron: odaya bu sayfayı aç, video sitenin oynatıcısında herkeste eşitlenir. */
export const syncPage = (src: string, title: string) => invoke<void>("sync_page", { src, title });

// ---------------------------------------------------------------- Real-Debrid + addon'lar (Faz 5, p0.15)

export interface DebridUser { username: string; type: string; expiration: string }
export interface DebridStatus { configured: boolean; user: DebridUser | null; error: string | null; key_page: string }
export const debridStatus = () => invoke<DebridStatus>("debrid_status");
/** Anahtarı Real-Debrid'de doğrular, anahtar zincirine yazar. */
export const debridSetKey = (key: string) => invoke<DebridUser>("debrid_set_key", { key });
export const debridClearKey = () => invoke<void>("debrid_clear_key");

/** Ayarlar: bağlantıyı sunucu üzerinden yönlendir (bu bilgisayarda saklanır, varsayılan kapalı). */
const RELAY_KEY = "koni.relay";
export const relayOn = () => { try { return localStorage.getItem(RELAY_KEY) === "1"; } catch { return false; } };
export function setRelay(on: boolean) {
  try { localStorage.setItem(RELAY_KEY, on ? "1" : "0"); } catch { /* depolama kapalı */ }
  return invoke<void>("set_relay", { on });
}
/** Kullanıcının eklediği addon (manifest adresi yalnızca bu bilgisayarda durur). */
export interface Addon { url: string; name: string }
export const addonsList = () => invoke<Addon[]>("addons_list");
export const addonAdd = (url: string) => invoke<Addon[]>("addon_add", { url });
export const addonRemove = (url: string) => invoke<Addon[]>("addon_remove", { url });
/** Cinemeta arama sonucu; `id` IMDb kimliği. */
export interface CatalogTitle {
  id: string; type: "movie" | "series"; name: string; poster?: string; releaseInfo?: string;
  background?: string | null; imdbRating?: string | null; genres?: string[] | null;
}
export interface Episode { id: string; season: number; episode: number; name: string; thumbnail?: string | null }
export const catalogSearch = (query: string) => invoke<CatalogTitle[]>("catalog_search", { query });
export const catalogPopular = () => invoke<CatalogTitle[]>("catalog_popular");
export const catalogEpisodes = (id: string) => invoke<Episode[]>("catalog_episodes", { id });
/** `hash` yoksa kaynak addon'un kendi hesabıyla çözülmüştür: odada paylaşılamaz. */
export interface Source { addon: string; name: string; title: string; hash: string | null; url?: string | null; idx: number | null; file: string }
export const addonStreams = (kind: "movie" | "series", id: string) =>
  invoke<{ sources: Source[]; errors: string[] }>("addon_streams", { kind, id });
/** Odadaysa yalnızca torrent odaya gider, herkes kendi hesabıyla çözer. Döner: başlık. */
export const debridOpen = (s: Source, title: string) =>
  invoke<string>("debrid_open", { hash: s.hash ?? null, url: s.url ?? null, name: s.name, rawTitle: s.title, file: s.file, idx: s.idx, title });
/** Otomatik kaynak seçimi: torrent Real-Debrid'de açılıyor mu (oynatmadan). */
export const debridCheck = (s: Source) =>
  invoke<void>("debrid_check", { hash: s.hash ?? null, url: s.url ?? null, name: s.name, rawTitle: s.title, file: s.file, idx: s.idx });
/** Çalan akışın kapak görseli (yt-dlp); yoksa `null`. */
export const playerCover = () => invoke<string | null>("player_cover");
