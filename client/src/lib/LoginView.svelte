<script lang="ts">
  // Giriş / kayıt (ana-ekran mockup h4, h6, h7). KONİ'ye yalnızca bir üyenin davet
  // koduyla girilir; kodla kaydolan, davet edenle doğrudan arkadaş olur.
  import Logo from "./Logo.svelte";
  import LangSwitch from "./LangSwitch.svelte";
  import { t } from "./i18n.svelte";
  import { DEFAULT_SERVER, serverLabel } from "./server";

  let {
    username = $bindable(),
    code = $bindable(),
    server,
    onServer,
    busy,
    error,
    onSubmit,
  }: {
    username: string;
    /** koni://invite/KOD linkinden gelen ya da elle yazılan üye davet kodu. */
    code: string;
    /** Santral adresi (wss://…/ws); kendi sunucusunu kuran değiştirir. */
    server: string;
    onServer: (input: string) => void;
    busy: boolean;
    error: string | null;
    onSubmit: (mode: "login" | "register", password: string, code: string) => void;
  } = $props();

  const known = (() => { try { return !!localStorage.getItem("koni.username"); } catch { return false; } })();
  let mode = $state<"start" | "register" | "login">(code ? "register" : known ? "login" : "start");
  let password = $state("");
  /** İlk ekranda yazılan kod; "Devam et" ile `code`'a geçer. */
  let typed = $state("");
  // Linkten kod gelirse kayıt formu açılır.
  $effect(() => { if (code && mode === "start") mode = "register"; });

  /** "abcd efgh", "ABCDEFGH", link… → "ABCD-EFGH". */
  function normCode(t: string): string {
    const m = t.match(/koni:\/\/invite\/([^\s/?#)]+)/i);
    const raw = (m ? decodeURIComponent(m[1]) : t).toUpperCase().replace(/[^A-Z0-9]/g, "");
    return raw.length === 8 ? `${raw.slice(0, 4)}-${raw.slice(4)}` : t.trim();
  }
  const valid = (t: string) => /^[A-Z0-9]{4}-[A-Z0-9]{4}$/.test(normCode(t)) || t.trim().length > 12;
  const codeOk = $derived(valid(code));
  const bare = $derived(username.trim().replace(/#.*$/, ""));
  const ready = $derived(!!username.trim() && password.length > 0 && (mode === "login" || codeOk));

  let editing = $state(false);
  /** Varsayılan sunucunun adresi ekranda gösterilmez; yalnızca kendi sunucusu yazılınca görünür. */
  const custom = $derived(server !== DEFAULT_SERVER);
  let serverIn = $state("");
  function saveServer(input: string) {
    onServer(input);
    editing = false;
  }

  function submit(e: Event) {
    e.preventDefault();
    if (busy) return;
    if (mode === "start") {
      // Kendi sunucusundaki bir arkadaşın linki: koni://invite/KOD?s=sunucu
      const s = typed.match(/koni:\/\/invite\/[^\s?#]+\?s=([^&#\s)]+)/i)?.[1];
      if (s) onServer(decodeURIComponent(s));
      if (valid(typed)) code = normCode(typed);
      return;
    }
    if (ready) onSubmit(mode, password, normCode(code));
  }
</script>

<div class="reg">
  <div class="left">
    <span class="brand"><Logo size={18} />KONİ</span>
    {#if mode === "login"}
      <h1>{t("Tekrar hoş geldin.")}</h1>
      <p>{t("Arkadaşların seni bekliyor.")}</p>
    {:else if mode === "register"}
      <h1>{t("Seni bekleyen bir koltuk var.")}</h1>
      <div class="host"><b>{t("Bir arkadaşın seni davet etti")}</b><small>{t("Kaydolunca doğrudan arkadaş olursunuz.")}</small></div>
      <div class="perks"><span>{t("Birlikte film, dizi, YouTube; herkeste aynı saniye")}</span><span>{t("Ne izlesek? ile hep birlikte karar")}</span><span>{t("Arkadaşlarınla istatistik ve rozetler")}</span></div>
    {:else}
      <h1>{t("Arkadaşlarınla aynı saniyede izle.")}</h1>
      <p>{t("Film, dizi, YouTube ya da müzik: biri oynatınca herkeste aynı anda oynar. Duraklatan, saran, bekleten herkes görünür.")}</p>
    {/if}
  </div>

  <form class="right" onsubmit={submit}>
    {#if mode === "start"}
      <span class="k">{t("KONİ davetle çalışır")}</span>
      <h2>{t("Bir arkadaşın seni çağırsın")}</h2>
      <p class="muted">{t("KONİ'ye yalnızca üyelerin davetiyle girilir. KONİ kullanan bir arkadaşından davet linki iste: linke tıklayınca bu ekran kendiliğinden dolar.")}</p>
      <label class="fld"><span>{t("Davet kodu ya da linki")}</span>
        <input class="lg mono" placeholder="ABCD-EFGH" bind:value={typed} spellcheck="false" />
        <small>{t("Arkadaşın profilinde \"Davet et\"e basınca 7 gün geçerli bir kod alır.")}</small></label>
      <button class="white lg big" type="submit" disabled={!valid(typed)}>{t("Devam et")}</button>
      <div class="hr"></div>
      <button type="button" class="lg" onclick={() => (mode = "login")}>{t("Zaten hesabım var · Giriş yap")}</button>
    {:else}
      <h2>{mode === "login" ? t("Giriş yap") : t("Hesabını aç")}</h2>
      {#if mode === "register"}
        <label class="fld"><span>{t("Davet kodu")}</span>
          <input class="lg mono" placeholder="ABCD-EFGH" bind:value={code} spellcheck="false" /></label>
      {/if}
      <label class="fld"><span>{t("Kullanıcı adı")}</span>
        <span class="uname">
          <input class="lg" bind:value={username} maxlength={mode === "login" ? 29 : 24} autocomplete="username" spellcheck="false"
            placeholder={mode === "login" ? t("Ad ya da Ad#1234") : t("Kullanıcı adı")} />
          {#if mode === "register" && bare}<em>#····</em>{/if}
        </span>
        <small>{mode === "login"
          ? t("Adını aynı kullanan başka biri yoksa etiketi yazmana gerek yok.")
          : t("Adın sonradan değişmez. Etiket otomatik verilir, adı aynı olanları ayırır.")}</small></label>
      <label class="fld"><span>{t("Şifre")}</span>
        <input class="lg" type="password" bind:value={password} maxlength="128"
          autocomplete={mode === "login" ? "current-password" : "new-password"} /></label>
      {#if error}<div class="err">{error}</div>{/if}
      <button class="white lg big" type="submit" disabled={busy || !ready}>
        {busy ? t("Bekle…") : mode === "login" ? t("Giriş yap") : t("Kaydol ve arkadaşınla bağlan")}
      </button>
      <small class="swap">
        {#if mode === "login"}{t("Hesabın yok mu?")} <button type="button" class="link" onclick={() => (mode = code ? "register" : "start")}>{t("Davet kodum var")}</button>
        {:else}{t("Zaten hesabın var mı?")} <button type="button" class="link" onclick={() => (mode = "login")}>{t("Giriş yap")}</button>{/if}
      </small>
    {/if}
    {#if editing}
      <div class="srv edit">
        <!-- svelte-ignore a11y_autofocus -->
        <input placeholder={t("Sunucunun adresi, ör. koni.ornek.com")} bind:value={serverIn} spellcheck="false" autofocus
          onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); saveServer(serverIn); } else if (e.key === "Escape") editing = false; }} />
        <button type="button" disabled={!serverIn.trim()} onclick={() => saveServer(serverIn)}>{t("Bağlan")}</button>
        {#if custom}<button type="button" class="link" onclick={() => saveServer("")}>{t("Davetli girişe dön")}</button>
        {:else}<button type="button" class="link" onclick={() => (editing = false)}>{t("Vazgeç")}</button>{/if}
      </div>
      <small class="srv">{t("Kurulum rehberi: GitHub'da serkanqltun-hub/koni → docs/SELF-HOSTING.md")}</small>
    {:else if custom}
      <small class="srv">{t("Sunucu:")} <span class="mono">{serverLabel(server)}</span> ·
        <button type="button" class="link" onclick={() => { serverIn = serverLabel(server); editing = true; }}>{t("Değiştir")}</button></small>
    {:else}
      <small class="srv">{t("Davetin yok mu?")}
        <button type="button" class="link" onclick={() => { serverIn = ""; editing = true; }}>{t("Kendi sunucunu kur")}</button></small>
    {/if}
  </form>
  <div class="lang"><LangSwitch /></div>
</div>

<style>
  .reg { position: relative; grid-area: main; min-height: 0; display: grid; grid-template-columns: 1fr 1fr; }
  .left {
    display: flex; flex-direction: column; justify-content: center; gap: 18px; padding: 0 clamp(32px, 6vw, 72px); overflow: hidden;
    background: radial-gradient(70% 60% at 30% 20%, rgba(10, 132, 255, 0.28), transparent 70%),
      radial-gradient(60% 50% at 80% 90%, rgba(191, 90, 242, 0.2), transparent 70%), #08090b;
  }
  .lang { position: absolute; right: 16px; top: 12px; }
  .brand { display: flex; align-items: center; gap: 8px; font-weight: 800; letter-spacing: 0.02em; font-size: 16px; }
  h1 { margin: 0; font-size: clamp(32px, 3.6vw, 44px); font-weight: 900; letter-spacing: -0.04em; line-height: 1.02; }
  .left p { margin: 0; color: var(--text-2); font-size: 15px; max-width: 420px; }
  .host { display: flex; flex-direction: column; padding: 14px 16px; border-radius: 18px; background: rgba(255, 255, 255, 0.06); border: 1px solid var(--line-2); max-width: 420px; }
  .host small { color: var(--text-2); }
  .perks { display: flex; flex-direction: column; gap: 8px; color: var(--text-2); font-size: 13px; }
  .perks span::before { content: "✓"; color: var(--green); font-weight: 900; margin-right: 10px; }
  .right { display: flex; flex-direction: column; justify-content: center; gap: 14px; padding: 0 clamp(32px, 6vw, 72px); border-left: 1px solid var(--line); background: var(--bg); overflow: auto; }
  .k { color: var(--text-2); font-size: 11.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em; }
  h2 { margin: 0; font-size: 26px; font-weight: 900; letter-spacing: -0.03em; }
  .muted { margin: 0; color: var(--text-2); font-size: 13.5px; }
  .fld { display: flex; flex-direction: column; gap: 6px; }
  .fld > span:first-child { font-size: 12px; color: var(--text-2); font-weight: 600; }
  .fld small { color: var(--text-3); font-size: 11.5px; }
  .fld input { width: 100%; }
  input.mono { font-weight: 600; letter-spacing: 0.04em; }
  .uname { position: relative; display: block; }
  .uname em { position: absolute; right: 14px; top: 50%; transform: translateY(-50%); font: 600 14px var(--font-mono); font-style: normal; color: var(--text-3); pointer-events: none; }
  .big { height: 48px; margin-top: 6px; }
  .hr { height: 1px; background: var(--line); margin: 6px 0; }
  .err { color: #ff6961; font-size: 12.5px; }
  .swap { color: var(--text-3); text-align: center; }
  .swap .link { color: var(--text); font-weight: 650; font-size: inherit; }
  .srv { margin-top: 18px; color: var(--text-3); text-align: center; font-size: 11.5px; }
  .srv .link { color: var(--text-2); font-weight: 650; font-size: inherit; }
  .srv.edit { display: flex; gap: 8px; align-items: center; }
  .srv.edit input { flex: 1; }
</style>
