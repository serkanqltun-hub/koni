<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { t } from "./i18n.svelte";
  // Donma bekçisi (src-tauri/src/watchdog.rs) ana thread 2 sn'den uzun takılınca
  // `app://freeze` (ms) gönderir. Uyarı başlık çubuğunda kısa süre görünür:
  // video yüzeyinin üstüne çizilmez.
  let { onFreeze }: { onFreeze?: (sec: number) => void } = $props();
  let sec = $state<number | null>(null);

  onMount(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const un = listen<number>("app://freeze", (e) => {
      sec = Math.max(2, Math.round(e.payload / 1000));
      onFreeze?.(sec);
      clearTimeout(timer);
      timer = setTimeout(() => (sec = null), 8000);
    });
    return () => {
      clearTimeout(timer);
      un.then((f) => f());
    };
  });
</script>

{#if sec !== null}
  <button class="freeze" onclick={() => (sec = null)} title={t("Kapat")}>
    {t("Uygulama {n} sn yanıt vermedi. Kaydı tutuldu, Sorun bildir ile gönderebilirsin.", { n: sec })}
  </button>
{/if}

<style>
  .freeze {
    position: fixed; top: 6px; left: 50%; transform: translateX(-50%); z-index: 1000;
    padding: 5px 12px; border-radius: 8px; font-size: 12px; font-weight: 600; white-space: nowrap;
    color: var(--yellow); background: rgba(40, 32, 0, 0.92); border: 1px solid rgba(255, 214, 10, 0.35);
    cursor: pointer;
  }
</style>
