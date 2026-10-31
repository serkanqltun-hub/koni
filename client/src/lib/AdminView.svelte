<script lang="ts">
  // Yönetim (admin mockup ad1–ad3, PROTOCOL §3.0f): açık odalar ve tüm üyeler. Yalnızca
  // adminlerin hesap menüsünde açılır; sunucu her işlemde yetkiyi yeniden denetler.
  import { untrack } from "svelte";
  import Avatar from "./Avatar.svelte";
  import { adminCall, type AdminMember, type AdminState } from "./player";
  import { social, splitHandle } from "./social.svelte";
  import { t, tx, locale } from "./i18n.svelte";

  let { onBack }: { onBack: () => void } = $props();

  let tab = $state<"rooms" | "members">("rooms");
  let data = $state<AdminState | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let query = $state("");
  let banning = $state<AdminMember | null>(null);
  let confirm = $state("");

  async function run(op: "list" | "close" | "ban" | "invite_right" = "list", o: { room?: string; user?: string; on?: boolean } = {}) {
    if (busy) return;
    busy = true;
    try {
      data = await adminCall(social.server, op, o);
      error = null;
    } catch (e) {
      error = tx(String(e));
    } finally {
      busy = false;
    }
  }
  $effect(() => {
    untrack(() => run());
    const iv = setInterval(() => { if (!banning) run(); }, 10000);
    return () => clearInterval(iv);
  });

  const VIEW: Record<string, string> = { browse: "Keşfet", watch: "Sinema", listen: "Müzik" };
  const members = $derived.by(() => {
    const q = query.trim().toLocaleLowerCase(locale());
    const all = data?.members ?? [];
    return q ? all.filter((m) => m.name.toLocaleLowerCase(locale()).includes(q)) : all;
  });
  function since(ts: number): string {
    const m = Math.floor(((data?.now ?? Date.now()) - ts) / 60000);
    return m < 60 ? t("{n} dk", { n: m }) : t("{n} sa", { n: Math.floor(m / 60) });
  }
  function date(ts: number): string {
    const d = Math.floor((Date.now() - ts) / 86400000);
    if (d <= 0) return t("bugün");
    if (d === 1) return t("dün");
    return new Date(ts).toLocaleDateString(locale(), { day: "numeric", month: "long" });
  }
  async function ban() {
    if (!banning || confirm.trim().toLocaleLowerCase(locale()) !== banning.name.toLocaleLowerCase(locale())) return;
    const user = banning.name;
    await run("ban", { user });
    if (!error) { banning = null; confirm = ""; }
  }
  const inviterIsMember = $derived(banning?.invitedBy ? data?.members.find((m) => m.name === banning!.invitedBy) : undefined);
</script>

<div class="ad">
  <nav class="nav">
    <span class="k">{t("Yönetim")}</span>
    <button class:on={tab === "rooms"} onclick={() => (tab = "rooms")}>🎬 {t("Odalar")} <em>{t("{n} açık", { n: data?.rooms.length ?? "…" })}</em></button>
    <button class:on={tab === "members"} onclick={() => (tab = "members")}>👥 {t("Üyeler")} <em>{data?.members.length ?? "…"}</em></button>
    <button class="back" onclick={onBack}>{t("← Ana ekran")}</button>
    <span class="note">{t("Bu ekranı yalnızca adminler görür. Yapılan her işlem sunucu loguna yazılır.")}</span>
  </nav>

  <div class="main">
    {#if tab === "rooms"}
      <h1>{t("Odalar")}</h1>
      <p class="sub">{t("Şu an sunucuda açık olan bütün odalar. Kapatılan odadaki herkes ana ekrana döner.")}</p>
      {#if error}<div class="err">{error}</div>{/if}
      <div class="tbl rooms">
        <div class="tr h"><span>{t("Oda")}</span><span>{t("İçeridekiler")}</span><span>{t("Oynayan")}</span><span>{t("Süre")}</span><span></span></div>
        {#each data?.rooms ?? [] as r (r.id)}
          {@const host = r.members.find((m) => m.host)}
          <div class="tr">
            <span class="who"><span><b>{r.name}</b><small>{host ? t("host {name} · ", { name: host.name }) : ""}{VIEW[r.view] ? t(VIEW[r.view]) : r.view}</small></span></span>
            <span class="faces">
              {#if r.members.length}
                {#each r.members.slice(0, 5) as m (m.name)}<span class="face"><Avatar id={m.avatar} name={m.name} size={24} /></span>{/each}
                <span class="n">{t("{n} kişi", { n: r.members.length })}</span>
              {:else}<span class="n">{t("boş · birazdan kapanır")}</span>{/if}
            </span>
            <span class="mut">{r.title || "—"}</span>
            <span class="mut">{since(r.since)}</span>
            <span><button class="red sm" disabled={busy} onclick={() => run("close", { room: r.id })}>{t("Odayı kapat")}</button></span>
          </div>
        {:else}
          <div class="tr empty">{data ? t("Şu an açık oda yok.") : t("Yükleniyor…")}</div>
        {/each}
      </div>
    {:else}
      <h1>{t("Üyeler")}</h1>
      <p class="sub">{t("Tüm hesaplar. Banlanan hesap silinir; onu davet eden admin değilse davet hakkını kaybeder.")}</p>
      <input class="lg search" placeholder={t("Ad ya da Ad#1234 ile ara")} bind:value={query} spellcheck="false" />
      {#if error}<div class="err">{error}</div>{/if}
      <div class="tbl users">
        <div class="tr h"><span>{t("Üye")}</span><span>{t("Davet eden")}</span><span>{t("Katıldı")}</span><span>{t("Durum")}</span><span></span></div>
        {#each members as m (m.name)}
          {@const [n, tag] = splitHandle(m.name)}
          <div class="tr">
            <span class="who"><Avatar id={m.avatar} name={m.name} size={34} />
              <span><b>{n}<span class="tag">#{tag}</span>{#if m.admin}<span class="atag">ADMIN</span>{/if}</b>
                <small>{t("{n} arkadaş", { n: m.friends })}{#if !m.canInvite && !m.admin} · <span class="noinv">{t("davet hakkı yok")}</span>{/if}</small></span></span>
            <span class="mut">{m.invitedBy || "—"}</span>
            <span class="mut">{date(m.created)}</span>
            <span class={m.room ? "live" : m.online ? "mut" : "off"}>{m.room ? t("odada") : m.online ? t("çevrimiçi") : t("çevrimdışı")}</span>
            <span>
              {#if !m.admin}
                {#if !m.canInvite}<button class="sm" disabled={busy} onclick={() => run("invite_right", { user: m.name, on: true })}>{t("Hakkı geri ver")}</button>
                {:else}<button class="red sm" disabled={busy} onclick={() => { banning = m; confirm = ""; }}>{t("Banla")}</button>{/if}
              {/if}
            </span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

{#if banning}
  {@const [n] = splitHandle(banning.name)}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="scrim" onclick={(e) => { if (e.target === e.currentTarget) banning = null; }}>
    <div class="modal" role="dialog" aria-modal="true">
      <span class="k red-t">{t("Üyeyi banla")}</span>
      <div class="found"><Avatar id={banning.avatar} name={banning.name} size={46} />
        <span><b>{banning.name}</b><small>{banning.invitedBy ? t("davet eden: {name} · ", { name: banning.invitedBy }) : ""}{t("{date} katıldı", { date: date(banning.created) })}</small></span></div>
      <div class="warn">
        <b>{t("Bu işlem geri alınmaz.")}</b>
        <span class="li">{t("{n} adlı üyenin hesabı, oturumları, arkadaşlıkları ve istatistikleri silinir.", { n })}</span>
        <span class="li">{t("Odadaysa hemen çıkarılır; yeni bir davet kodu olmadan geri dönemez.")}</span>
        {#if banning.invitedBy && !inviterIsMember?.admin}
          <span class="li">{t("{name} admin değil:", { name: banning.invitedBy })} <b>{t("davet hakkını kaybeder")}</b>{t(", açık davet kodları iptal olur.")}</span>
        {/if}
      </div>
      <label class="fld"><span>{t("Onaylamak için \"{name}\" yaz", { name: banning.name })}</span>
        <input class="lg mono" bind:value={confirm} spellcheck="false" /></label>
      {#if error}<div class="err">{error}</div>{/if}
      <div class="foot">
        <button class="lg" onclick={() => (banning = null)}>{t("Vazgeç")}</button>
        <button class="lg solid" disabled={busy || confirm.trim().toLocaleLowerCase(locale()) !== banning.name.toLocaleLowerCase(locale())} onclick={ban}>{t("Banla ve hesabı sil")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .ad { grid-area: main; min-height: 0; display: grid; grid-template-columns: 220px 1fr; }
  .nav { border-right: 1px solid var(--line); padding: 18px 12px; display: flex; flex-direction: column; gap: 4px; background: var(--panel); }
  .nav .k { font-size: 11px; color: var(--text-3); font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em; padding: 4px 10px 8px; }
  .nav button { display: flex; align-items: center; gap: 10px; padding: 9px 10px; border-radius: 10px; color: var(--text-2); font-weight: 600; background: none; border: 0; text-align: left; }
  .nav button.on { background: var(--glass-2); color: #fff; }
  .nav button em { margin-left: auto; font-style: normal; font-size: 11px; color: var(--text-3); font-weight: 600; }
  .nav .back { margin-top: 10px; color: var(--text-3); }
  .nav .note { margin-top: auto; color: var(--text-3); font-size: 11.5px; padding: 10px; }
  .main { min-height: 0; overflow: auto; padding: 22px 28px; display: flex; flex-direction: column; gap: 14px; background: radial-gradient(60% 50% at 20% 0%, rgba(255, 159, 10, 0.1), transparent 70%); }
  h1 { margin: 0; font-size: 26px; font-weight: 900; letter-spacing: -0.03em; }
  .sub { margin: -8px 0 0; color: var(--text-2); }
  .err { color: #ff6961; font-size: 12.5px; }
  .search { max-width: 320px; }
  .tbl { border: 1px solid var(--line); border-radius: 16px; overflow: hidden; background: var(--glass); }
  .tr { display: grid; align-items: center; gap: 12px; padding: 10px 14px; border-top: 1px solid var(--line); }
  .tr:first-child { border-top: 0; }
  .tr.h { font-size: 11px; color: var(--text-3); font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; background: rgba(0, 0, 0, 0.2); }
  .tr.empty { color: var(--text-3); display: block; }
  .rooms .tr { grid-template-columns: 1.3fr 1.6fr 1.2fr 70px 120px; }
  .users .tr { grid-template-columns: 1.6fr 1.1fr 0.9fr 90px 130px; }
  .who { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .who b { font-size: 13.5px; font-weight: 650; }
  .who small { display: block; color: var(--text-3); font-size: 11.5px; }
  .tag { color: var(--text-3); font: 500 0.85em var(--font-mono); }
  .faces { display: flex; align-items: center; }
  .face { margin-left: -6px; border-radius: 50%; box-shadow: 0 0 0 2px #15161a; display: flex; }
  .face:first-child { margin-left: 0; }
  .faces .n { margin-left: 8px; color: var(--text-2); font-size: 12px; }
  .mut { color: var(--text-2); font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .live { color: var(--green); font-size: 12px; font-weight: 700; }
  .off { color: var(--text-3); font-size: 12px; }
  .noinv { color: #ff9f0a; font-weight: 700; font-size: 10.5px; }
  button.sm { height: 30px; padding: 0 12px; border-radius: 9px; font-size: 12px; font-weight: 650; }
  button.red { background: rgba(255, 69, 58, 0.14); border: 1px solid rgba(255, 69, 58, 0.35); color: #ff6961; }
  .scrim { position: fixed; inset: var(--top-bar) 0 0 0; background: rgba(0, 0, 0, 0.6); backdrop-filter: blur(6px); display: grid; place-items: center; z-index: 50; }
  .modal { width: 520px; max-width: calc(100vw - 32px); border-radius: 22px; background: var(--raise); border: 1px solid var(--line-2); box-shadow: 0 40px 100px rgba(0, 0, 0, 0.7); padding: 24px; display: flex; flex-direction: column; gap: 14px; }
  .k { font-size: 11.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em; }
  .red-t { color: #ff6961; }
  .found { display: flex; align-items: center; gap: 12px; padding: 12px; border-radius: 14px; background: var(--glass); border: 1px solid rgba(255, 69, 58, 0.4); }
  .found span:last-child { display: flex; flex-direction: column; }
  .found small { color: var(--text-2); }
  .warn { padding: 12px 14px; border-radius: 14px; background: rgba(255, 69, 58, 0.08); border: 1px solid rgba(255, 69, 58, 0.3); color: var(--text-2); font-size: 12.5px; display: flex; flex-direction: column; gap: 6px; }
  .warn b { color: #fff; }
  .warn .li::before { content: "•"; color: #ff6961; margin-right: 8px; }
  .fld { display: flex; flex-direction: column; gap: 6px; }
  .fld > span { font-size: 12px; color: var(--text-2); font-weight: 600; }
  .foot { display: flex; gap: 8px; justify-content: flex-end; }
  button.solid { background: #ff453a; color: #fff; border: 0; }
</style>
