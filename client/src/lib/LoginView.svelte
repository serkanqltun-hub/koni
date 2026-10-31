<script lang="ts">
  // Giriş / kayıt (ui-plan ilk-acilis.html). KONİ'ye yalnızca bir üyenin davetiyle girilir;
  // uygulamada hazır sunucu yok: adres davet linkinden (`?s=`) gelir ya da burada yazılır.
  import Logo from "./Logo.svelte";
  import LangSwitch from "./LangSwitch.svelte";
  import { t } from "./i18n.svelte";
  import { DEFAULT_SERVER, normServer, probe, serverLabel } from "./server";

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
    /** Santral adresi (wss://…/ws); boşsa henüz sunucu yok. */
    server: string;
    onServer: (input: string) => void;
    busy: boolean;
    error: string | null;
    onSubmit: (mode: "login" | "register", password: string, code: string) => void;
  } = $props();

  const GUIDE = "github.com/serkanqltun-hub/koni/blob/main/docs/SELF-HOSTING.md";
  const known = (() => { try { return !!localStorage.getItem("koni.username"); } catch { return false; } })();
  let mode = $state<"start" | "register" | "login" | "host">(code ? "register" : known ? "login" : "start");
  let password = $state("");
  /** İlk ekranda yazılan link ya da kod; "Devam et" ile `code`'a geçer. */
  let typed = $state("");
  // Linkten kod gelirse kayıt formu açılır.
  $effect(() => { if (code && (mode === "start" || mode === "host")) mode = "register"; });

  /** "abcd efgh", "ABCDEFGH", link… → "ABCD-EFGH". */
  function normCode(t: string): string {
    const m = t.match(/koni:\/\/invite\/([^\s/?#)]+)/i);
    const raw = (m ? decodeURIComponent(m[1]) : t).toUpperCase().replace(/[^A-Z0-9]/g, "");
    return raw.length === 8 ? `${raw.slice(0, 4)}-${raw.slice(4)}` : t.trim();
  }
  const valid = (t: string) => /^[A-Z0-9]{4}-[A-Z0-9]{4}$/.test(normCode(t)) || t.trim().length > 12;
  /** Linkteki sunucu: koni://invite/KOD?s=sunucu */
  const linkServer = (t: string) => {
    const s = t.match(/koni:\/\/invite\/[^\s?#]+\?s=([^&#\s)]+)/i)?.[1];
    return s ? decodeURIComponent(s) : "";
  };

  /** Sunucu ayarlı mı (varsayılan adresli özel derlemede de sayılır). */
  const hasServer = $derived(!!server);
  /** Adres ekranda gösterilir mi: özel derlemenin varsayılan adresi gizli kalır. */
  const shown = $derived(hasServer && server !== DEFAULT_SERVER);

  // ---- sunucu adresi kutusu (yazılırken kontrol edilir)
  let serverIn = $state("");
  let check = $state<"" | "wait" | "ok" | "bad">("");
  $effect(() => {
    const url = normServer(serverIn);
    check = "";
    if (!url) return;
    check = "wait";
    let live = true;
    const id = setTimeout(() => probe(url).then((ok) => { if (live) check = ok ? "ok" : "bad"; }), 500);
    return () => { live = false; clearTimeout(id); };
  });
  /** Kayıtlı sunucuyu değiştirme kutusu açık mı. */
  let editing = $state(false);
  function saveServer() {
    if (!serverIn.trim()) return;
    onServer(serverIn);
    editing = false;
  }

  /** İlk ekranda kod linksiz yazıldıysa ve sunucu yoksa adres de sorulur. */
  const askServer = $derived(mode === "start" && !hasServer && valid(typed) && !linkServer(typed));
  /** Kayıt/girişte sunucu yoksa adres formun başında sorulur. */
  const needServer = $derived((mode === "register" || mode === "login") && !hasServer);
  const serverReady = $derived(!!serverIn.trim() && check !== "bad");

  const codeOk = $derived(valid(code));
  const bare = $derived(username.trim().replace(/#.*$/, ""));
  const ready = $derived(!!username.trim() && password.length > 0 && (mode === "login" || codeOk) && (!needServer || serverReady));
  const startReady = $derived(valid(typed) && (!askServer || serverReady));

  function submit(e: Event) {
    e.preventDefault();
    if (busy) return;
    if (mode === "host") { if (serverReady) { saveServer(); mode = "start"; } return; }
    if (mode === "start") {
      if (!startReady) return;
      const s = linkServer(typed);
      if (s) onServer(s);
      else if (askServer) saveServer();
      code = normCode(typed);
      return;
    }
    if (!ready) return;
    if (needServer) saveServer();
    onSubmit(mode, password, normCode(code));
  }
  function go(m: typeof mode) { mode = m; editing = false; serverIn = ""; }
  let copied = $state(false);
  function copyGuide() {
    navigator.clipboard.writeText(`https://${GUIDE}`).then(() => { copied = true; setTimeout(() => (copied = false), 1800); });
  }
</script>

{#snippet serverBox(hint: string)}
  <label class="fld"><span>{t("Sunucu adresi")}</span>
    <span class="st-wrap">
      <input class="lg mono" placeholder="koni.example.com" bind:value={serverIn} spellcheck="false" />
      {#if check === "ok"}<em class="st ok">{t("✓ bağlandı")}</em>{:else if check === "wait"}<em class="st">…</em>{/if}
    </span>
    {#if check === "bad"}<small class="bad">{t("Bu sunucuya ulaşılamadı. Adresi kontrol et.")}</small>
    {:else}<small>{hint}</small>{/if}</label>
{/snippet}

{#snippet serverChip()}
  {#if editing}
    <div class="row">
      <span class="grow">{@render serverBox(t("Hesabının ya da davetinin olduğu sunucu."))}</span>
    </div>
    <div class="row end">
      <button type="button" class="link" onclick={() => (editing = false)}>{t("Vazgeç")}</button>
      <button type="button" disabled={!serverReady} onclick={saveServer}>{t("Bağlan")}</button>
    </div>
  {:else}
    <div class="chip"><span class="d"></span>{t("Sunucu")} <span class="mono">{serverLabel(server)}</span>
      <button type="button" class="link" onclick={() => { serverIn = serverLabel(server); editing = true; }}>{t("Değiştir")}</button></div>
  {/if}
{/snippet}

<div class="reg">
  <div class="left" class:own={mode === "host"}>
    <span class="brand"><Logo size={18} />KONİ</span>
    {#if mode === "login"}
      <h1>{t("Tekrar hoş geldin.")}</h1>
      <p>{t("Arkadaşların seni bekliyor.")}</p>
    {:else if mode === "register"}
      <h1>{t("Seni bekleyen bir koltuk var.")}</h1>
      <div class="host"><b>{t("Bir arkadaşın seni davet etti")}</b><small>{t("Kaydolunca doğrudan arkadaş olursunuz.")}</small></div>
      <div class="perks"><span>{t("Birlikte film, dizi, YouTube; herkeste aynı saniye")}</span><span>{t("Ne izlesek? ile hep birlikte karar")}</span><span>{t("Arkadaşlarınla istatistik ve rozetler")}</span></div>
    {:else if mode === "host"}
      <h1>{t("Kendi KONİ'n, kendi grubun.")}</h1>
      <p>{t("KONİ küçük bir eşitleme sunucusu kullanır. Kendininkini kurarsan hesaplar, davetler ve istatistikler yalnızca orada durur. Video sunucudan geçmez.")}</p>
      <div class="perks"><span>{t("1 vCPU / 1 GB bir VPS yeter")}</span><span>{t("Tek komutla kurulum, HTTPS dahil")}</span><span>{t("İlk açılan hesap yönetici olur")}</span></div>
    {:else}
      <h1>{t("Arkadaşlarınla aynı saniyede izle.")}</h1>
      <p>{t("Film, dizi, YouTube ya da müzik: biri oynatınca herkeste aynı anda oynar. Duraklatan, saran, bekleten herkes görünür.")}</p>
    {/if}
  </div>

  <form class="right" onsubmit={submit}>
    {#if mode === "start"}
      <span class="k">{t("KONİ davetle çalışır")}</span>
      <h2>{t("Bir arkadaşın seni çağırsın")}</h2>
      <p class="muted">{t("KONİ kullanan bir arkadaşından davet linki iste. Linke tıklayınca KONİ açılır ve sunucu dahil her şey kendiliğinden dolar.")}</p>
      <label class="fld"><span>{t("Davet linki ya da kodu")}</span>
        <input class="lg mono" placeholder="koni://invite/ABCD-EFGH?s=…" bind:value={typed} spellcheck="false" />
        {#if !askServer}<small>{t("Linki buraya yapıştırabilirsin. Yalnızca kod varsa sunucu adresini de soracağız.")}</small>{/if}</label>
      {#if askServer}{@render serverBox(t("Davet linkinde ?s= sonrasında yazar. Bilmiyorsan seni davet eden arkadaşına sor."))}{/if}
      <button class="white lg big" type="submit" disabled={!startReady}>{t("Devam et")}</button>
      <div class="hr"></div>
      <button type="button" class="lg" onclick={() => go("login")}>{t("Zaten hesabım var · Giriş yap")}</button>
      {#if shown}<div class="foot">{@render serverChip()}</div>{/if}
      <small class="srv">{t("Seni davet edecek kimse yok mu?")}
        <button type="button" class="link" onclick={() => go("host")}>{t("Kendi sunucunu kur →")}</button></small>
    {:else if mode === "host"}
      <button type="button" class="link back" onclick={() => go("start")}>{t("← Geri")}</button>
      <span class="k">{t("Kendi sunucunu kur")}</span>
      <h2>{t("Üç adımda hazır")}</h2>
      <div class="steps">
        <div class="step"><i>1</i><div><b>{t("Bir Linux sunucu ve alan adı")}</b><small>{t("Ubuntu ya da Debian. Alan adını sunucuya yönlendir, ör. koni.example.com.")}</small></div></div>
        <div class="step"><i>2</i><div><b>{t("Kurulum betiğini çalıştır")}</b>
          <pre>git clone https://github.com/serkanqltun-hub/koni /opt/koni/src
cd /opt/koni/src
DOMAIN=koni.example.com NGINX=1 ./deploy/install.sh</pre></div></div>
        <div class="step"><i>3</i><div><b>{t("İlk davet kodunu oluştur")}</b><small>{t("Komut kurulum rehberinde. Sonra aşağıya adresini yaz ve o kodla kaydol.")}</small></div></div>
      </div>
      <div class="guide"><span class="mono">{GUIDE}</span>
        <button type="button" onclick={copyGuide}>{copied ? t("Kopyalandı ✓") : t("Rehberin linkini kopyala")}</button></div>
      <div class="hr"></div>
      {@render serverBox(t("Kurduğun sunucunun alan adı."))}
      <button class="white lg big" type="submit" disabled={!serverReady}>{t("Bağlan")}</button>
    {:else}
      <h2>{mode === "login" ? t("Giriş yap") : t("Hesabını aç")}</h2>
      {#if needServer}{@render serverBox(mode === "login" ? t("Hesabının açıldığı sunucu. Bir kez yazman yeter, KONİ hatırlar.") : t("Davet linkinde ?s= sonrasında yazar. Bilmiyorsan seni davet eden arkadaşına sor."))}
      {:else if shown}{@render serverChip()}{/if}
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
        {#if mode === "login"}{t("Hesabın yok mu?")} <button type="button" class="link" onclick={() => go(code ? "register" : "start")}>{t("Davet kodum var")}</button>
        {:else}{t("Zaten hesabın var mı?")} <button type="button" class="link" onclick={() => go("login")}>{t("Giriş yap")}</button>{/if}
      </small>
    {/if}
  </form>
  <div class="lang"><LangSwitch /></div>
</div>

<style>
  .reg { position: relative; grid-area: main; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
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
  .srv { margin-top: 14px; color: var(--text-3); text-align: center; font-size: 11.5px; }
  .srv .link { color: var(--text-2); font-weight: 650; font-size: inherit; }
  .left.own {
    background: radial-gradient(70% 60% at 30% 20%, rgba(48, 209, 88, 0.2), transparent 70%),
      radial-gradient(60% 50% at 80% 90%, rgba(10, 132, 255, 0.2), transparent 70%), #08090b;
  }
  .st-wrap { position: relative; display: block; }
  .st { position: absolute; right: 14px; top: 50%; transform: translateY(-50%); font: 650 12px var(--font); font-style: normal; color: var(--text-3); pointer-events: none; }
  .st.ok { color: var(--green); }
  .fld small.bad { color: #ff6961; }
  .chip { display: flex; align-items: center; gap: 10px; padding: 9px 12px; border-radius: 12px; background: rgba(48, 209, 88, 0.08); border: 1px solid rgba(48, 209, 88, 0.3); font-size: 12.5px; }
  .chip .d { width: 8px; height: 8px; border-radius: 50%; background: var(--green); box-shadow: 0 0 8px var(--green); }
  .chip .mono { font: 600 12.5px var(--font-mono); }
  .chip .link { margin-left: auto; font-size: inherit; font-weight: 650; }
  .row { display: flex; gap: 8px; align-items: center; }
  .row.end { justify-content: flex-end; }
  .grow { flex: 1; }
  .foot { margin-top: 6px; }
  .back { align-self: flex-start; font-size: 12px; color: var(--text-3); }
  .steps { display: flex; flex-direction: column; gap: 12px; }
  .step { display: grid; grid-template-columns: 26px 1fr; gap: 12px; }
  .step > i { width: 26px; height: 26px; border-radius: 50%; background: rgba(255, 255, 255, 0.08); border: 1px solid var(--line-2); display: grid; place-items: center; font-style: normal; font-weight: 800; font-size: 12px; }
  .step b { display: block; font-size: 13.5px; }
  .step small { color: var(--text-2); font-size: 12px; }
  pre { margin: 6px 0 0; padding: 9px 12px; border-radius: 10px; background: rgba(0, 0, 0, 0.45); border: 1px solid var(--line); font: 500 11.5px/1.55 var(--font-mono); color: #d0d0d5; white-space: pre; overflow-x: auto; user-select: text; }
  .guide { display: flex; align-items: center; gap: 10px; font-size: 11.5px; color: var(--text-3); }
  .guide .mono { font-family: var(--font-mono); flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; user-select: text; }
</style>
