<script lang="ts">
  import { t, tx } from "./i18n.svelte";
  // "Sorun bildir": isteğe bağlı kısa açıklama + Gönder. Logları App toplar (`send`).
  let { send }: { send: (note: string) => Promise<unknown> } = $props();

  let open = $state(false);
  let note = $state("");
  let busy = $state(false);
  let result = $state<{ ok: boolean; text: string } | null>(null);

  async function submit() {
    busy = true;
    result = null;
    try {
      await send(note);
      result = { ok: true, text: t("Gönderildi, teşekkürler") };
      note = "";
      open = false;
    } catch (e) {
      result = { ok: false, text: tx(String(e)) };
    } finally {
      busy = false;
    }
  }
</script>

<div class="bug">
  {#if open}
    <textarea rows="3" maxlength="2000" placeholder={t("Ne oldu? (isteğe bağlı)")} bind:value={note}></textarea>
    <div class="row">
      <small>{t("Sürüm, işletim sistemi, etkinlik ve senkron logları eklenir.")}</small>
      <button disabled={busy} onclick={() => (open = false)}>{t("Vazgeç")}</button>
      <button class="white" disabled={busy} onclick={submit}>{busy ? t("Gönderiliyor…") : t("Gönder")}</button>
    </div>
  {:else}
    <div class="row">
      <div class="grow"><b>{t("Sorun bildir")}</b><small>{t("Bir şey ters gittiyse loglarla birlikte geliştiriciye gönder.")}</small></div>
      <button onclick={() => { open = true; result = null; }}>{t("Sorun bildir")}</button>
    </div>
  {/if}
  {#if result}<small class:ok={result.ok} class:err={!result.ok}>{result.text}</small>{/if}
</div>

<style>
  .bug { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  .row { display: flex; align-items: center; gap: 8px; }
  .grow { flex: 1; min-width: 0; }
  .grow b { display: block; font-weight: 550; }
  .row small { flex: 1; }
  small { color: var(--text-3); font-size: 12px; display: block; }
  textarea {
    width: 100%; resize: vertical; font: inherit; color: var(--fg); user-select: text;
    background: var(--fill); border: 1px solid transparent; border-radius: 10px; padding: 7px 11px; outline: none;
  }
  textarea:focus-visible { border-color: rgba(10, 132, 255, 0.8); box-shadow: 0 0 0 3px rgba(10, 132, 255, 0.35); }
  .ok { color: var(--green); }
  .err { color: var(--red); }
</style>
