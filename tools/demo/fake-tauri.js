// A tiny stand-in for the Tauri backend: answers the app's commands with sample data.
(() => {
  const now = Math.floor(Date.now() / 1000);
  const cbs = new Map();
  let cbId = 1;
  const listeners = {};
  const emit = (event, payload) =>
    (listeners[event] || []).forEach((id) => cbs.get(id)?.({ event, id, payload }));
  const av = (n) => `av${String(n).padStart(2, "0")}`;
  const friends = [
    { name: "Mina#2040", avatar: av(3), mutual: true, online: true, room: "Friday classics", title: "Night of the Living Dead (1968)" },
    { name: "Kaan#7712", avatar: av(9), mutual: true, online: true },
    { name: "Deniz#3301", avatar: av(14), mutual: true, online: false },
  ];
  const presence = {
    t: "presence",
    friends,
    requests: [],
    calls: [],
    rooms: [{ invite: "R1", room: "Friday classics", title: "Night of the Living Dead (1968)", view: "cinema", playing: true, pos: 1500, dur: 5760, members: ["Mina#2040", "Kaan#7712"] }],
  };
  const profile = { name: "Ada", tag: "4821", avatar: av(5), avatars: [], invites: [], daily: [0, 3600, 7200, 0, 5400, 9000, 2400], canInvite: true };
  const handlers = {
    account_get: () => "Ada#4821",
    profile: () => profile,
    friends: () => ({ friends, requests: [] }),
    lobby_start: () => { setTimeout(() => emit("lobby://msg", presence), 300); },
    feed_get: () => ({ you: "Ada#4821", items: [
      { kind: "watch", at: now - 3600, users: ["Mina#2040", "Kaan#7712"], title: "Nosferatu (1922)", sec: 5400 },
      { kind: "joined", at: now - 86400, users: ["Deniz#3301"] },
    ] }),
    stats_get: () => ({ period: "week", you: "Ada#4821",
      users: [
        { user: "Ada#4821", days: 5, watchSec: 27000, nightSec: 3600, longestSec: 7200, pauses: 9, seeks: 2, holds: 1 },
        { user: "Mina#2040", days: 4, watchSec: 21600, nightSec: 9000, longestSec: 6000, pauses: 14, seeks: 1, holds: 0 },
      ],
      badges: [{ id: "snack", title: "Snack Break Champion", rule: "paused the most", users: ["Mina#2040"] }] }),
    avatar_get: () => "",
  };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main", windowLabel: "main" } },
    transformCallback: (fn) => { const id = cbId++; cbs.set(id, fn); return id; },
    unregisterCallback: (id) => cbs.delete(id),
    convertFileSrc: (p) => p,
    invoke: async (cmd, args) => {
      if (cmd === "plugin:event|listen") { (listeners[args.event] ||= []).push(args.handler); return args.handler; }
      if (cmd.startsWith("plugin:")) return null;
      const h = handlers[cmd];
      return h ? h(args) : null;
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  localStorage.setItem("koni.server", "wss://koni.example.com/ws");
  localStorage.setItem("koni.username", "Ada");
  localStorage.setItem("koni.lang", "en");
})();
