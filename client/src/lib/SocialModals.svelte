<script lang="ts">
  // Ana ekran pencereleri (mockup h1–h3): arkadaşını davet et, arkadaş ekle, profil fotoğrafı.
  import Avatar from "./Avatar.svelte";
  import { friendsCall, profileCall, type Friends } from "./player";
  import { pic, refreshProfile, social, splitHandle } from "./social.svelte";
  import { t, tx } from "./i18n.svelte";
  import { inviteLink } from "./server";

  let { kind, welcome = false, onClose, onFriends }: {
    kind: "invite" | "add" | "avatar";
    /** Yeni kayıt: fotoğraf seçimi karşılama olarak açılır. */
    welcome?: boolean;
    onClose: () => void;
    onFriends?: () => void;
  } = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);

  // ---- davet
  // Kodun kendisi sunucuda saklanmaz: son oluşturulan, kullanılmadıkça yeniden gösterilir.
  const LAST = "koni.lastInvite";
  function lastCode(): string | null {
    try {
      const v = JSON.parse(localStorage.getItem(LAST) ?? "null") as { code: string; expires: number } | null;
      const top = social.profile?.invites[0];
      return v && top && !top.usedBy && top.expires === v.expires && v.expires > Date.now() ? v.code : null;
    } catch { return null; }
  }
  let code = $state<string | null>(lastCode());
  async function newInvite() {
    busy = true;
    error = null;
    try {
      const p = await profileCall(social.server, "invite");
      if (p) {
        social.profile = p;
        code = p.code ?? null;
        try { localStorage.setItem(LAST, JSON.stringify({ code, expires: p.invites[0]?.expires ?? 0 })); } catch { /* yalnızca kolaylık */ }
      }
    } catch (e) { error = tx(String(e)); } finally { busy = false; }
  }
  let tried = false;
  const noRight = $derived(social.profile?.canInvite === false);
  $effect(() => { if (kind === "invite" && code === null && !tried && !noRight) { tried = true; newInvite(); } });
  let copied = $state(false);
  function copy() {
    if (!code) return;
    const me = social.profile ? `${social.profile.name}#${social.profile.tag}` : "";
    const link = inviteLink(code, social.server);
    navigator.clipboard.writeText(me ? t("{me} seni KONİ'ye davet ediyor: {link}", { me, link }) : t("KONİ'ye davet: {link}", { link }))
      .then(() => { copied = true; setTimeout(() => (copied = false), 1800); });
  }
  function left(ms: number) {
    const d = Math.ceil((ms - Date.now()) / 86400000);
    return d <= 0 ? t("süresi doldu") : t("{d} gün kaldı", { d });
  }

  // ---- arkadaş ekle
  let handle = $state("");
  let list = $state<Friends | null>(null);
  $effect(() => {
    if (kind === "add") friendsCall(social.server, "list").then((l) => (list = l), (e) => (error = tx(String(e))));
  });
  const pending = $derived(list?.friends.filter((f) => !f.mutual) ?? []);
  async function friendOp(op: "add" | "remove", user: string) {
    if (busy || !user.trim()) return;
    busy = true;
    error = null;
    try {
      list = await friendsCall(social.server, op, user.trim());
      if (op === "add") handle = "";
      onFriends?.();
    } catch (e) { error = tx(String(e)); } finally { busy = false; }
  }

  // ---- fotoğraf
  let chosen = $state(social.profile?.avatar ?? "");
  const all = $derived(social.profile?.avatars ?? []);
  const taken = $derived(new Set(social.profile?.taken ?? []));
  const admin = $derived(!!social.profile?.admin);
  let file = $state<HTMLInputElement | null>(null);
  /** Admin: görseli 256 px kareye kırpıp JPEG olarak yükler; seçiciye eklenir ve fotoğrafın olur. */
  async function upload(f: File | undefined) {
    if (!f || busy) return;
    busy = true;
    error = null;
    try {
      const bmp = await createImageBitmap(f);
      const side = Math.min(bmp.width, bmp.height);
      const c = document.createElement("canvas");
      c.width = c.height = 256;
      c.getContext("2d")!.drawImage(bmp, (bmp.width - side) / 2, (bmp.height - side) / 2, side, side, 0, 0, 256, 256);
      const data = c.toDataURL("image/jpeg", 0.88).split(",")[1];
      const p = await profileCall(social.server, "upload", undefined, data);
      if (p) { social.profile = p; chosen = p.avatar; }
    } catch (e) { error = tx(String(e)); } finally { busy = false; if (file) file.value = ""; }
  }
  async function save() {
    if (!chosen || chosen === social.profile?.avatar) return onClose();
    busy = true;
    try {
      const p = await profileCall(social.server, "avatar", chosen);
      if (p) social.profile = p;
      onClose();
    } catch (e) { error = tx(String(e)); } finally { busy = false; }
  }
  $effect(() => { if (kind === "avatar" && !all.length) refreshProfile(); });

  function key(e: KeyboardEvent) { if (e.key === "Escape") onClose(); }
</script>

<svelte:window onkeydown={key} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
  <div class="modal" class:wide={kind === "avatar"} role="dialog" aria-modal="true">
    {#if kind === "invite"}
      <span class="k">{t("Arkadaşını KONİ'ye davet et")}</span>
      {#if noRight}
        <h2>{t("Davet hakkın yok")}</h2>
        <p>{t("Davet ettiğin biri bir admin tarafından banlandığı için davet hakkın alındı. Bir admin geri verebilir.")}</p>
      {:else}
      <h2>{code ? t("Davet linkin hazır") : t("Davet linkin hazırlanıyor")}</h2>
      <p>{t("Linki arkadaşına gönder: tıklayınca KONİ sunucu dahil her şeyi kendisi doldurur. Bu linkle kaydolan doğrudan arkadaşın olur, link tek kişiliktir.")}</p>
      <div class="code"><b class="mono link-text">{code ? inviteLink(code, social.server) : "koni://invite/····-····"}</b>
        <button class="white" disabled={!code} onclick={copy}>{copied ? t("Kopyalandı ✓") : t("Linki kopyala")}</button></div>
      {#if code}<button class="link newc" disabled={busy} onclick={newInvite}>{t("Yeni link oluştur")}</button>{/if}
      <div class="facts"><span><b>{t("7 gün")}</b>{t("geçerli")}</span><span><b>{t("1 kişi")}</b>{t("kullanabilir")}</span><span><b>{t("Otomatik")}</b>{t("arkadaş olursunuz")}</span></div>
      {/if}
      {#if social.profile?.invites.length}
        <div class="lab">{t("Önceki davetlerin")}</div>
        <div class="old">
          {#each social.profile.invites.slice(0, 4) as inv, i (i)}
            <div>
              {#if inv.usedBy}
                <Avatar id={inv.avatar} name={inv.usedBy} size={24} />
                <span><b>{splitHandle(inv.usedBy)[0]}</b> {t("kaydoldu")}</span><em class="ok">{t("arkadaşın ✓")}</em>
              {:else}
                <span class="ring"></span><span>{t("kullanılmadı")}</span><em>{left(inv.expires)}</em>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    {:else if kind === "add"}
      <span class="k">{t("Arkadaş ekle")}</span>
      <h2>{t("Kullanıcı adı ve etiketi")}</h2>
      <p>{t("Adlar aynı olabilir; etiket kişiyi ayırır. Arkadaşından profilindeki \"Ad#1234\"ü iste.")}</p>
      <form class="row" onsubmit={(e) => { e.preventDefault(); friendOp("add", handle); }}>
        <input class="lg mono" placeholder={t("Ad#1234")} bind:value={handle} maxlength="29" spellcheck="false" />
        <button class="white lg" type="submit" disabled={busy || !handle.trim()}>{t("İstek gönder")}</button>
      </form>
      <p class="small">{t("İstek kabul edilince birbirinizi odaya çağırabilir, istatistiklerinizi görebilirsiniz.")}</p>
      {#if pending.length}
        <div class="lab">{t("Gönderdiğin istekler")}</div>
        <div class="old">
          {#each pending as f (f.name)}
            <div><Avatar id={f.avatar} name={f.name} size={24} /><code>{f.name}</code> {t("bekliyor")}
              <em><button class="link" disabled={busy} onclick={() => friendOp("remove", f.name)}>{t("geri al")}</button></em></div>
          {/each}
        </div>
      {/if}
    {:else}
      <span class="k">{welcome ? t("Hoş geldin!") : t("Profil fotoğrafı")}</span>
      <div class="prev">
        {#if pic(chosen)}<img class="big" src={pic(chosen)} alt="" />{:else}<span class="big empty"></span>{/if}
        <span><b>{social.profile?.name ?? ""}<span class="tag"> #{social.profile?.tag ?? ""}</span></b><br />
          <span class="muted">{welcome ? t("Bir profil fotoğrafı seç. ") : ""}{t("Aşağıdakilerden birini seç. Arkadaşların seni bu resimle görür.")}</span></span>
      </div>
      <div class="pick">
        {#if admin}
          <button class="pc up" disabled={busy} onclick={() => file?.click()} title={t("Görsel ekle: seçiciye eklenir, sen kullandıkça başkası seçemez")}>＋<br />{t("Görsel ekle")}</button>
          <input type="file" accept="image/jpeg,image/png,image/webp" hidden bind:this={file} onchange={(e) => upload((e.currentTarget as HTMLInputElement).files?.[0])} />
        {/if}
        {#each all as id (id)}
          <button class="pc" class:on={chosen === id} class:taken={taken.has(id)} disabled={taken.has(id)}
            title={taken.has(id) ? t("Bunu bir admin kullanıyor") : undefined} onclick={() => (chosen = id)} aria-label={id}>
            {#if pic(id)}<img src={pic(id)} alt="" draggable="false" />{/if}
          </button>
        {/each}
      </div>
    {/if}
    {#if error}<div class="err">{error}</div>{/if}
    <div class="foot">
      {#if kind === "avatar"}
        <button class="lg" onclick={onClose}>{welcome ? t("Sonra") : t("Vazgeç")}</button>
        <button class="white lg" disabled={busy || !chosen} onclick={save}>{t("Kaydet")}</button>
      {:else}
        <button class="lg" onclick={onClose}>{t("Kapat")}</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: var(--top-bar) 0 0 0; background: rgba(0, 0, 0, 0.6); backdrop-filter: blur(6px); display: grid; place-items: center; z-index: 50; }
  .modal { width: 520px; max-width: calc(100vw - 32px); max-height: calc(100vh - 80px); overflow: auto; border-radius: 22px; background: var(--raise); border: 1px solid var(--line-2); box-shadow: 0 40px 100px rgba(0, 0, 0, 0.7); padding: 24px; display: flex; flex-direction: column; gap: 14px; }
  .modal.wide { width: 720px; }
  .k { color: var(--text-2); font-size: 11.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em; }
  h2 { margin: 0; font-size: 24px; font-weight: 900; letter-spacing: -0.03em; }
  p { margin: 0; color: var(--text-2); font-size: 13px; }
  p.small { font-size: 12px; }
  .muted { color: var(--text-2); }
  .code { display: flex; align-items: center; gap: 12px; padding: 16px; border-radius: 16px; background: rgba(0, 0, 0, 0.4); border: 1px dashed rgba(255, 255, 255, 0.22); }
  .code b { font-size: 26px; font-weight: 700; letter-spacing: 0.12em; user-select: text; }
  .code b.link-text { flex: 1; min-width: 0; font-size: 13px; letter-spacing: 0; word-break: break-all; }
  .code button { margin-left: auto; }
  .newc { align-self: flex-start; font-size: 12px; margin-top: -6px; }
  .facts { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }
  .facts span { padding: 10px 12px; border-radius: 12px; background: var(--glass); border: 1px solid var(--line); font-size: 12px; color: var(--text-2); }
  .facts b { display: block; color: #fff; font-size: 13px; }
  .lab { color: var(--text-2); font-size: 11.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; }
  .old { display: flex; flex-direction: column; gap: 6px; }
  .old > div { display: flex; align-items: center; gap: 10px; font-size: 12.5px; color: var(--text-2); padding: 8px 10px; border-radius: 10px; background: var(--glass); }
  .old b { color: var(--text); }
  .old code { font: 600 12px var(--font-mono); color: var(--text); }
  .old em { font-style: normal; margin-left: auto; font-size: 11.5px; }
  .old em.ok { color: var(--green); }
  .old .link { font-size: 11.5px; }
  .ring { width: 24px; height: 24px; border-radius: 50%; border: 1.5px dashed rgba(255, 255, 255, 0.3); flex: none; }
  .row { display: flex; gap: 8px; }
  .row input { flex: 1; min-width: 0; }
  .prev { display: flex; align-items: center; gap: 16px; }
  .prev .big { width: 72px; height: 72px; border-radius: 50%; object-fit: cover; flex: none; box-shadow: 0 0 0 3px var(--raise), 0 0 0 5px #fff; }
  .prev .empty { background: var(--glass-2); }
  .prev b { font-size: 18px; }
  .tag { color: var(--text-3); font: 500 0.88em var(--font-mono); }
  .pick { display: grid; grid-template-columns: repeat(9, 1fr); gap: 10px; }
  .pc { padding: 0; border: 0; border-radius: 50%; aspect-ratio: 1; overflow: hidden; background: var(--glass-2); box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.08); transition: transform 0.12s, box-shadow 0.12s; }
  .pc img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .pc.taken { opacity: 0.3; }
  .pc.up { display: grid; place-items: center; text-align: center; line-height: 1.2; font-size: 10.5px; font-weight: 800; color: #ffd60a; background: rgba(255, 214, 10, 0.06); box-shadow: inset 0 0 0 1.5px rgba(255, 214, 10, 0.5); }
  .pc.on { box-shadow: 0 0 0 3px var(--raise), 0 0 0 5px #fff; transform: scale(1.04); }
  .foot { display: flex; gap: 8px; justify-content: flex-end; }
  .err { color: #ff6961; font-size: 12.5px; }
</style>
