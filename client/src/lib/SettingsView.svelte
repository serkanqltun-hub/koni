<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { relayOn, setRelay, type UpdateInfo } from "./player";
  import BugReport from "./BugReport.svelte";
  import DebridSettings from "./DebridSettings.svelte";
  import LangSwitch from "./LangSwitch.svelte";
  import { t, locale } from "./i18n.svelte";
  import { DEFAULT_SERVER, serverLabel } from "./server";

  // Uygulama ayarları (UI-PLAN §5.9, E3). Yalnızca gerçekten çalışan ayarlar var.
  let {
    account,
    username,
    server,
    update,
    updating,
    onBack,
    onLogout,
    onCheckUpdate,
    onInstall,
    onBugReport,
  }: {
    account: string | null;
    username: string;
    server: string;
    update: UpdateInfo | null;
    updating: number | null;
    onBack: () => void;
    onLogout: () => void;
    onCheckUpdate: () => Promise<void>;
    onInstall: () => void;
    onBugReport: (note: string) => Promise<unknown>;
  } = $props();

  type Section = "account" | "debrid" | "keys" | "about";
  const SECTIONS: { id: Section; label: string }[] = [
    { id: "account", label: "Hesap" },
    { id: "debrid", label: "Sağlayıcı" },
    { id: "keys", label: "Kısayollar" },
    { id: "about", label: "Hakkında" },
  ];
  let section = $state<Section>("account");
  let version = $state("");
  let checking = $state(false);
  let checked = $state(false);

  $effect(() => { getVersion().then((v) => (version = v)).catch(() => {}); });

  async function check() {
    checking = true;
    try { await onCheckUpdate(); } finally { checking = false; checked = true; }
  }

  const KEYS: [string, string][] = [
    ["Boşluk", "Oynat / duraklat"],
    ["F", "Tam ekran"],
    ["Esc", "Çekmeceyi kapat · tam ekrandan çık"],
    ["Ctrl V", "Linki Video ekle'ye yapıştır"],
    ["G / H", "Altyazı 50 ms erken / geç"],
  ];
  let relay = $state(relayOn());
  // Telefon kumandası (p0.24): santralin adresi tarayıcıda açılınca kumanda sayfası gelir.
  const remoteUrl = $derived(server.replace(/^ws/i, "http"));
  let copied = $state(false);
  function copyRemote() {
    navigator.clipboard.writeText(remoteUrl).then(() => { copied = true; setTimeout(() => (copied = false), 1500); }, () => {});
  }
</script>

<div class="screen ambient settings">
  <nav class="side">
    <button class="back" onclick={onBack}>‹ {t("Ana ekran")}</button>
    {#each SECTIONS as s (s.id)}
      <button class="nav" class:on={section === s.id} onclick={() => (section = s.id)}>{t(s.label)}</button>
    {/each}
  </nav>
  <main>
    {#if section === "account"}
      <h1>{t("Hesap")}</h1>
      <div class="group glass-card">
        <div class="item">
          <span class="avatar">{(account ?? username).slice(0, 1).toLocaleUpperCase(locale())}</span>
          <div class="grow"><b>{account ?? username}</b><small>{account ? t("Hesapla giriş yapıldı") : t("Hesapsız")}{server === DEFAULT_SERVER ? "" : ` · ${serverLabel(server)}`}</small></div>
          <button class="danger" onclick={onLogout}>{t("Çıkış yap")}</button>
        </div>
        <div class="item">
          <div class="grow"><b>Dil · Language</b></div>
          <LangSwitch />
        </div>
        <div class="item">
          <div class="grow"><b>{t("Bağlantıyı sunucu üzerinden yönlendir")}</b><small>{t("Bazı servislere doğrudan ulaşılamıyorsa bağlantı KONİ sunucusu üzerinden kurulur. Video sunucudan geçmez.")}</small></div>
          <input type="checkbox" role="switch" bind:checked={relay} onchange={() => setRelay(relay).catch(() => {})} />
        </div>
        {#if account && remoteUrl}
          <div class="item">
            <div class="grow"><b>{t("Telefondan kumanda")}</b><small>{t("Telefonunun tarayıcısında bu adresi aç ve KONİ hesabınla giriş yap: oynat/duraklat, ileri/geri ve ses. Odadaysan herkeste aynı anda olur.")}</small><small class="mono sel">{remoteUrl}</small></div>
            <button onclick={copyRemote}>{copied ? t("Kopyalandı") : t("Kopyala")}</button>
          </div>
        {/if}
        <div class="item"><small class="grow">{t('Başka bir sunucuya geçmek için çıkış yap; giriş ekranının altındaki "Kendi sunucunu kur" ile adresi yaz.')}</small></div>
      </div>
    {:else if section === "debrid"}
      <h1>{t("Sağlayıcı")}</h1>
      <DebridSettings />
    {:else if section === "keys"}
      <h1>{t("Kısayollar")}</h1>
      <div class="group glass-card">
        {#each KEYS as [k, d] (k)}
          <div class="item"><span class="grow">{t(d)}</span><kbd>{t(k)}</kbd></div>
        {/each}
      </div>
    {:else}
      <h1>{t("Hakkında")}</h1>
      <div class="group glass-card">
        <div class="item"><span class="grow">{t("Sürüm")}</span><span class="mono">{version || "–"}</span></div>
        <div class="item"><div class="grow"><b>{t("Veri kaynakları")}</b><small>{t("Film bilgileri ve afişler TMDB'den gelir.")} This product uses the TMDB API but is not endorsed or certified by TMDB. {t("Şarkı sözleri LRCLIB'den.")} {t("Altyazılar OpenSubtitles.com'dan.")}</small></div></div>
        <div class="item">
          <div class="grow">
            <b>{t("Güncelleme")}</b>
            <small>
              {#if update}{t("KONİ {v} hazır", { v: update.version })}{update.portable ? " · " + t("taşınabilir sürümü elle güncelle") : ""}
              {:else if checked}{t("Güncelsin")}
              {:else}{t("Açılışta ve 6 saatte bir kendiliğinden bakılır")}{/if}
            </small>
          </div>
          {#if update && !update.portable}
            <button class="white" disabled={updating !== null} onclick={onInstall}>{updating === null ? t("Güncelle") : `%${updating}`}</button>
          {:else}
            <button disabled={checking} onclick={check}>{checking ? t("Bakılıyor…") : t("Şimdi denetle")}</button>
          {/if}
        </div>
        <div class="item col"><BugReport send={onBugReport} /></div>
        <div class="item"><small class="grow">{t("Oynatıcı LGPL libmpv/FFmpeg kullanır; lisanslar ve kaynak kurulum klasöründe (NOTICE.md, LICENSES/, source/).")}</small></div>
      </div>
    {/if}
  </main>
</div>

<style>
  .settings { display: grid; grid-template-columns: 240px 1fr; place-items: stretch; padding: 0; }
  .side { padding: 20px 12px; border-right: 1px solid var(--line); background: rgba(17, 18, 20, 0.7); display: flex; flex-direction: column; gap: 2px; }
  .back { background: transparent; border: 0; text-align: left; color: var(--text-2); margin-bottom: 14px; }
  .nav { background: transparent; border: 0; text-align: left; padding: 8px 10px; border-radius: 9px; color: var(--text-2); }
  .nav.on { background: var(--glass-3); color: var(--text); }
  main { padding: 32px 48px; max-width: 760px; overflow-y: auto; }
  h1 { margin: 0 0 20px; font-size: 24px; font-weight: 650; letter-spacing: -0.02em; }
  .group { border-radius: 16px; overflow: hidden; }
  .item { display: flex; align-items: center; gap: 14px; padding: 14px 16px; }
  .item + .item { border-top: 1px solid var(--line); }
  .item.col { flex-direction: column; align-items: stretch; gap: 10px; }
  .item b { display: block; font-weight: 550; }
  small { color: var(--text-3); font-size: 12px; }
  .grow { flex: 1; min-width: 0; }
  .avatar {
    width: 44px; height: 44px; border-radius: 50%; display: grid; place-items: center; font-weight: 650; font-size: 18px;
    background: linear-gradient(135deg, #0a84ff, #5e5ce6);
  }
  .sel { display: block; margin-top: 4px; color: var(--text-2); user-select: text; overflow-wrap: anywhere; }
  kbd { font: 500 11px var(--font-mono); padding: 2px 7px; border-radius: 6px; border: 1px solid var(--line-2); color: var(--text-2); }
</style>
