// Hesabın sosyal durumu (PROTOCOL §3.0d–e, p0.18): profil, arkadaşların anlık durumu,
// odaya çağrılar ve profil fotoğrafları. Ana ekran ve oda çekmecesi aynı veriyi okur.
import { listen } from "@tauri-apps/api/event";
import { avatarGet, lobbyStart, profileCall, type Presence, type Profile } from "./player";

export const social = $state<{ server: string; profile: Profile | null; presence: Presence | null; online: boolean }>({
  server: "",
  profile: null,
  presence: null,
  online: false,
});

/** Fotoğraf kimliği → `data:` adresi (yüklenince dolar). */
export const pics = $state<Record<string, string>>({});
const asked = new Set<string>();

export function pic(id: string | undefined | null): string | undefined {
  if (!id) return undefined;
  if (!asked.has(id) && social.server) {
    asked.add(id);
    avatarGet(social.server, id).then((u) => (pics[id] = u), () => asked.delete(id));
  }
  return pics[id];
}

let unlisten: (() => void) | null = null;

/** Telefon kumandası (p0.24): komut `remote` lobiden gelir, App oynatıcıya uygular. */
export type RemoteOp = { op: "toggle" | "seek" | "vol" | "status"; by: number };
let remoteHandler: ((m: RemoteOp) => void) | null = null;
export function onRemote(fn: ((m: RemoteOp) => void) | null) {
  remoteHandler = fn;
}

/** Giriş yapılınca: profili al, lobiyi aç. Tekrar çağrılabilir. */
export async function startSocial(server: string) {
  social.server = server;
  if (!unlisten) {
    unlisten = await listen<Record<string, unknown>>("lobby://msg", (e) => {
      const m = e.payload;
      if (m.t === "presence") {
        social.presence = m as unknown as Presence;
        social.online = true;
      } else if (m.t === "remote") {
        remoteHandler?.(m as unknown as RemoteOp);
      } else if (m.t === "hello") {
        social.online = true;
      } else if (m.t === "offline" || m.t === "error") {
        social.online = false;
      }
    });
  }
  lobbyStart(server).catch(() => {});
  await refreshProfile();
}

export async function refreshProfile() {
  try {
    social.profile = await profileCall(social.server);
  } catch {
    // eski sunucu (p0.17) profil tanımaz: ana ekran fotoğrafsız çalışır
  }
}

export function stopSocial() {
  social.profile = null;
  social.presence = null;
  social.online = false;
}

/** "Ad#1234" → ["Ad", "1234"]. */
export function splitHandle(h: string): [string, string] {
  const i = h.lastIndexOf("#");
  return i > 0 ? [h.slice(0, i), h.slice(i + 1)] : [h, ""];
}
