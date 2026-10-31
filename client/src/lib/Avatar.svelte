<script lang="ts">
  // Profil fotoğrafı; yoksa (ya da yüklenene kadar) adın baş harfi. `dot`: çevrimiçi durumu.
  import { pic, splitHandle } from "./social.svelte";
  import { locale } from "./i18n.svelte";

  let { id, name, size = 34, dot }: { id?: string | null; name: string; size?: number; dot?: "on" | "room" | "off" } = $props();

  const src = $derived(pic(id));
  const bare = $derived(splitHandle(name)[0]);
  function hue(s: string) {
    let h = 0;
    for (const c of s) h = (h * 53 + c.codePointAt(0)! * 7) % 360;
    return h;
  }
</script>

<span class="avw" style="--s:{size}px">
  {#if src}
    <img class="pp" {src} alt="" draggable="false" />
  {:else}
    <span class="pp ini" style="--h:{hue(bare)}">{bare.slice(0, 1).toLocaleUpperCase(locale())}</span>
  {/if}
  {#if dot}<i class="dot {dot}"></i>{/if}
</span>

<style>
  .avw { position: relative; flex: none; display: inline-block; width: var(--s); height: var(--s); }
  .pp { width: 100%; height: 100%; border-radius: 50%; object-fit: cover; display: block; }
  .ini { display: grid; place-items: center; font-weight: 650; font-size: calc(var(--s) * 0.42); background: hsl(var(--h) 50% 42%); color: #fff; }
  .dot { position: absolute; right: -1px; bottom: -1px; width: max(10px, calc(var(--s) * 0.32)); height: max(10px, calc(var(--s) * 0.32)); border-radius: 50%; background: var(--green); border: 2.5px solid var(--dot-ring, #111214); }
  .dot.room { background: var(--accent, #0a84ff); }
  .dot.off { background: var(--text-3); }
</style>
