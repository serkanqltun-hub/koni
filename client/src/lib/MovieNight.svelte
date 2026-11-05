<script lang="ts">
  // Film gecesi planla (p0.28, PROTOCOL §3.0p): ana ekranda yaklaşan geceler + planlama
  // penceresi. Saatinde odayı sunucu açar ve davetlileri çağırır (mevcut çağrı akışı).
  import { untrack } from "svelte";
  import Avatar from "./Avatar.svelte";
  import { eventOp, type EventQuery, type MovieNight, type PresenceFriend, type Rsvp } from "./player";
  import { social, splitHandle } from "./social.svelte";
  import { t, tx, locale } from "./i18n.svelte";

  let { friends, busy, onJoin }: { friends: PresenceFriend[]; busy: boolean; onJoin: (code: string) => void } = $props();

  // Lobi bağlıyken `presence.events` anlık gelir; değilse bir kez liste. Op cevabı bir
  // sonraki presence'a kadar listeyi tutar.
  let local = $state<MovieNight[] | null>(null);
  let fetched = false;
  $effect(() => {
    const ev = social.presence?.events;
    untrack(() => {
      local = null;
      if (ev || fetched || !social.server) return;
      fetched = true;
      eventOp(social.server).then((r) => { if (r && !social.presence?.events) local = r.events; }, () => {});
    });
  });
  const list = $derived([...(local ?? social.presence?.events ?? [])].sort((a, b) => a.at - b.at).slice(0, 4));

  let now = $state(Date.now());
  $effect(() => { const i = setInterval(() => (now = Date.now()), 30_000); return () => clearInterval(i); });

  const bare = (n: string) => splitHandle(n)[0].toLocaleLowerCase(locale());
  const myRsvp = (e: MovieNight): Rsvp | null => {
    const me = social.profile?.name;
    const g = me ? e.guests.find((x) => bare(x.name) === bare(me)) : undefined;
    return g ? g.rsvp : null;
  };

  function when(at: number): string {
    const d = new Date(at), today = new Date(now);
    const day = Math.round((new Date(d).setHours(0, 0, 0, 0) - today.setHours(0, 0, 0, 0)) / 86_400_000);
    const hm = d.toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });
    if (day === 0) return t("Bugün {hm}", { hm });
    if (day === 1) return t("Yarın {hm}", { hm });
    return `${d.toLocaleDateString(locale(), { weekday: "short", day: "numeric", month: "short" })} ${hm}`;
  }
  function soon(e: MovieNight): string | null {
    if (e.state || e.room) return null;
    const min = Math.ceil((e.at - now) / 60_000);
    if (min > 60) return null;
    return min <= 0 ? t("Başlıyor") : t("{n} dk sonra", { n: min });
  }
  const MARK: Record<Rsvp, string> = { yes: "✓", maybe: "?", no: "✕", "": "·" };
  const RSVPS: [Exclude<Rsvp, "">, string][] = [["yes", "Evet"], ["maybe", "Belki"], ["no", "Hayır"]];

  let error = $state<string | null>(null);
  let working = $state(false);
  const ERR: Record<string, string> = {
    bad_entry: "Saat geçmiş ya da 60 günden ileride olamaz",
    not_friends: "Yalnızca karşılıklı arkadaşlarını davet edebilirsin",
    unknown_user: "Bu adla bir hesap yok",
    too_many_events: "En fazla 10 planlanmış film gecen olabilir",
    not_joined: "Sunucu film gecesini desteklemiyor (güncelleme gerekli)",
    bad_message: "Sunucu film gecesini desteklemiyor (güncelleme gerekli)",
  };
  async function op(q: EventQuery): Promise<boolean> {
    if (!social.server || working) return false;
    working = true;
    error = null;
    try {
      const r = await eventOp(social.server, q);
      if (r) local = r.events;
      return true;
    } catch (e) {
      const c = String(e);
      error = ERR[c] ? t(ERR[c]) : tx(c);
      return false;
    } finally { working = false; }
  }

  // ---- planlama penceresi
  let open = $state(false);
  let title = $state("");
  let at = $state("");
  let picked = $state<string[]>([]);
  const pad = (n: number) => String(n).padStart(2, "0");
  function plan() {
    const d = new Date();
    if (d.getHours() >= 20) d.setDate(d.getDate() + 1);
    d.setHours(21, 0, 0, 0);
    at = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T21:00`;
    title = "";
    picked = [];
    error = null;
    open = true;
  }
  function toggle(n: string) {
    picked = picked.includes(n) ? picked.filter((x) => x !== n) : picked.length < 7 ? [...picked, n] : picked;
  }
  async function create(e: SubmitEvent) {
    e.preventDefault();
    const ms = new Date(at).getTime();
    if (!ms || ms <= Date.now()) { error = t(ERR.bad_entry); return; }
    if (await op({ op: "create", title: title.trim(), at: ms, guests: picked })) open = false;
  }
  function key(e: KeyboardEvent) { if (open && e.key === "Escape") open = false; }
</script>

<svelte:window onkeydown={key} />

{#if list.length || friends.length}
  <div class="lab">{t("Film geceleri")}
    <button class="sm" onclick={plan}>＋ {t("Film gecesi planla")}</button>
  </div>
  {#if error && !open}<div class="err">{error}</div>{/if}
  {#if list.length}
    <div class="nights">
      {#each list as e (e.id)}
        {@const mine = myRsvp(e)}
        {@const s = soon(e)}
        <div class="ev" class:hot={!!s || !!e.room} class:off={e.state === "missed"}>
          <span class="t">
            <b>🍿 {e.title}</b>
            <small>{when(e.at)} · {e.mine ? t("sen planladın") : t("{name} planladı", { name: splitHandle(e.host.name)[0] })}
              {#if s}<em class="soon">{s}</em>{/if}
              {#if e.room}<em class="soon">{t("Oda açık")}</em>{/if}
              {#if e.state === "missed"}<em>{t("kaçtı")}</em>{/if}</small>
          </span>
          <span class="ppl">
            {#each e.guests.slice(0, 6) as g (g.name)}
              <span class="g r-{g.rsvp || 'none'}" title="{splitHandle(g.name)[0]} · {MARK[g.rsvp]}"><Avatar id={g.avatar} name={g.name} size={24} /><i>{MARK[g.rsvp]}</i></span>
            {/each}
          </span>
          <span class="go">
            {#if e.room}
              <button class="sm primary" disabled={busy} onclick={() => onJoin(e.room!.invite)}>{t("Odaya katıl")}</button>
            {:else if e.mine && !e.state}
              <button class="sm danger" disabled={working} onclick={() => op({ op: "cancel", id: e.id })}>{t("İptal")}</button>
            {:else if mine !== null && !e.state}
              <span class="seg">
                {#each RSVPS as [r, label] (r)}
                  <button class="sm" class:on={mine === r} disabled={working} onclick={() => op({ op: "rsvp", id: e.id, rsvp: r })}>{t(label)}</button>
                {/each}
              </span>
            {/if}
          </span>
        </div>
      {/each}
    </div>
  {/if}
{/if}

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="scrim" onclick={(e) => { if (e.target === e.currentTarget) open = false; }}>
    <div role="dialog" aria-modal="true"><form class="modal" onsubmit={create}>
      <h2>{t("Film gecesi planla")}</h2>
      <p>{t("Saati gelince oda kendiliğinden açılır ve davetlilere çağrı gider.")}</p>
      <label><span>{t("Başlık")}</span><input bind:value={title} maxlength="80" placeholder={t("Film gecesi")} /></label>
      <label><span>{t("Tarih ve saat")}</span><input type="datetime-local" bind:value={at} required /></label>
      <span class="k">{t("Kimler gelsin?")} <small>{picked.length}/7</small></span>
      <div class="pick">
        {#each friends as f (f.name)}
          <button type="button" class:on={picked.includes(f.name)} onclick={() => toggle(f.name)}>
            <Avatar id={f.avatar} name={f.name} size={22} /><span>{splitHandle(f.name)[0]}</span>
          </button>
        {:else}
          <p class="muted">{t("Davet etmek için karşılıklı arkadaşın olmalı.")}</p>
        {/each}
      </div>
      {#if error}<div class="err">{error}</div>{/if}
      <div class="row">
        <button type="button" onclick={() => (open = false)}>{t("Vazgeç")}</button>
        <button class="primary" type="submit" disabled={working || !at}>{working ? "…" : t("Planla")}</button>
      </div>
    </form></div>
  </div>
{/if}

<style>
  .lab { display: flex; justify-content: space-between; align-items: center; color: var(--text-2); font-size: 11.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; margin-top: 4px; }
  .lab button { text-transform: none; letter-spacing: 0; }
  button.sm { height: 32px; padding: 0 12px; border-radius: 10px; font-size: 12px; font-weight: 650; }
  .err { color: #ff6961; font-size: 12.5px; }
  .muted { margin: 0; color: var(--text-2); font-size: 12.5px; }
  .nights { display: flex; flex-direction: column; gap: 8px; }
  .ev { display: flex; align-items: center; gap: 14px; padding: 9px 12px; border-radius: 16px; background: var(--raise); border: 1px solid var(--line-2); }
  .ev.hot { background: linear-gradient(90deg, rgba(255, 159, 10, 0.2), rgba(255, 159, 10, 0.05)); border-color: rgba(255, 159, 10, 0.45); }
  .ev.off { opacity: 0.55; }
  .ev .t { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .ev .t b { font-size: 14px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ev .t small { color: var(--text-2); font-size: 12px; }
  .ev em { font-style: normal; margin-left: 6px; color: var(--text-3); }
  .ev em.soon { color: #ffb340; font-weight: 650; }
  .ppl { display: flex; gap: 2px; }
  .g { position: relative; display: inline-flex; }
  .g i { position: absolute; right: -3px; bottom: -3px; font-style: normal; font-size: 9px; font-weight: 800; width: 13px; height: 13px; border-radius: 50%; display: grid; place-items: center; background: var(--line-2); color: var(--text-2); }
  .g.r-yes i { background: var(--green); color: #000; }
  .g.r-maybe i { background: #ffb340; color: #000; }
  .g.r-no i { background: #ff6961; color: #000; }
  .g.r-no { opacity: 0.5; }
  .go { display: flex; gap: 8px; }
  .seg { display: flex; gap: 4px; }
  .seg button.on { background: var(--accent); color: #fff; border-color: transparent; }
  .scrim { position: fixed; inset: var(--top-bar) 0 0 0; background: rgba(0, 0, 0, 0.6); backdrop-filter: blur(6px); display: grid; place-items: center; z-index: 50; }
  .modal { width: 480px; max-width: calc(100vw - 32px); max-height: calc(100vh - 80px); overflow: auto; border-radius: 22px; background: var(--raise); border: 1px solid var(--line-2); box-shadow: 0 40px 100px rgba(0, 0, 0, 0.7); padding: 24px; display: flex; flex-direction: column; gap: 12px; }
  .modal h2 { margin: 0; }
  .modal p { margin: 0; color: var(--text-2); font-size: 13px; }
  .modal label { display: flex; flex-direction: column; gap: 6px; font-size: 12px; color: var(--text-2); }
  .k { font-size: 12px; color: var(--text-2); }
  .k small { color: var(--text-3); margin-left: 6px; }
  .pick { display: flex; flex-wrap: wrap; gap: 6px; }
  .pick button { display: inline-flex; align-items: center; gap: 6px; height: 32px; padding: 0 10px 0 5px; border-radius: 16px; font-size: 12.5px; }
  .pick button.on { background: var(--accent); color: #fff; border-color: transparent; }
  .row { display: flex; justify-content: flex-end; gap: 8px; }
</style>
