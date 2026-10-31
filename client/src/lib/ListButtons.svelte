<script lang="ts">
  // Künyede liste düğmeleri (profil-liste mockup pl2): İzleyeceklerime ekle / İzledim
  // ve bu başlığı isteyen ya da izlemiş arkadaşlar.
  import Avatar from "./Avatar.svelte";
  import { listsAdd, listsRate, listsRemove, listsWho, type ListEntry, type ListWho } from "./player";
  import { social, splitHandle } from "./social.svelte";
  import { t } from "./i18n.svelte";

  let { entry }: { entry: ListEntry } = $props();

  let who = $state<ListWho | null>(null);
  let busy = $state(false);
  $effect(() => {
    const ref = entry.ref;
    who = null;
    if (!ref || !social.server) return;
    let live = true;
    listsWho(social.server, ref).then((w) => { if (live) who = w; }, () => {});
    return () => { live = false; };
  });

  async function run(f: () => Promise<unknown>) {
    if (busy || !social.server) return;
    busy = true;
    try {
      await f();
      who = await listsWho(social.server, entry.ref);
    } catch { /* bir sonraki açılışta düzelir */ } finally { busy = false; }
  }
  const toggleWant = () => run(() => who?.mine.want ? listsRemove(social.server, "want", entry.ref) : listsAdd(social.server, "want", $state.snapshot(entry)));
  const toggleSeen = () => run(() => who?.mine.seen ? listsRemove(social.server, "seen", entry.ref) : listsAdd(social.server, "seen", $state.snapshot(entry)));
  const rate = (r: number) => run(() => listsRate(social.server, entry.ref, who?.mine.rating === r ? 0 : r));

  const bare = (n: string) => splitHandle(n)[0];
  const wants = $derived(who?.want ?? []);
  const seens = $derived(who?.seen ?? []);
  function join(n: string[]): string {
    return n.length <= 2 ? n.join(t(" ve ")) : `${n.slice(0, 2).join(", ")} +${n.length - 2}`;
  }
</script>

{#if social.server && entry.ref}
  <div class="lb">
    <div class="btns">
      <button class="sm" class:on={who?.mine.want} disabled={busy || !who} onclick={toggleWant}>
        {who?.mine.want ? t("✓ İzleyeceklerimde") : t("+ İzleyeceklerime ekle")}</button>
      <button class="sm" class:on={who?.mine.seen} disabled={busy || !who} onclick={toggleSeen}>
        {who?.mine.seen ? t("✓ İzledim") : t("İzledim olarak işaretle")}</button>
      {#if who?.mine.seen}
        <button class="sm r" class:on={who.mine.rating === 1} title={t("Beğendim")} disabled={busy} onclick={() => rate(1)}>❤️</button>
        <button class="sm r" class:on={who.mine.rating === -1} title={t("Beğenmedim")} disabled={busy} onclick={() => rate(-1)}>👎</button>
      {/if}
    </div>
    {#if wants.length || seens.length}
      <div class="who">
        <span class="ppl">{#each [...wants, ...seens].slice(0, 3) as h, i (i)}<Avatar id={h.avatar} name={h.name} size={20} />{/each}</span>
        <span>
          {#if wants.length}<b>{join(wants.map((h) => bare(h.name)))}</b> {t("da izlemek istiyor")}{/if}{#if wants.length && seens.length} · {/if}{#if seens.length}<b>{join(seens.map((h) => bare(h.name)))}</b> {t("izledi")}{#if seens.length === 1 && seens[0].rating} {seens[0].rating > 0 ? "❤️" : "👎"}{/if}{/if}
        </span>
      </div>
    {/if}
  </div>
{/if}

<style>
  .lb { display: flex; flex-direction: column; gap: 8px; }
  .btns { display: flex; flex-wrap: wrap; gap: 6px; }
  .btns button { height: 32px; padding: 0 12px; border-radius: 10px; font-size: 12px; font-weight: 650; }
  .btns button.on { background: rgba(48, 209, 88, 0.14); border-color: rgba(48, 209, 88, 0.5); color: #8af0a5; }
  .btns button.r { padding: 0 9px; opacity: 0.55; }
  .btns button.r.on { opacity: 1; background: rgba(255, 255, 255, 0.1); border-color: rgba(255, 255, 255, 0.3); color: inherit; }
  .who { display: flex; align-items: center; gap: 8px; padding: 7px 10px; border-radius: 10px; background: var(--glass); border: 1px solid var(--line); font-size: 12px; color: var(--text-2); }
  .who b { color: #fff; font-weight: 650; }
  .ppl { display: flex; flex: none; }
  .ppl :global(.avw) { margin-left: -5px; border-radius: 50%; box-shadow: 0 0 0 1.5px #17181b; }
  .ppl :global(.avw:first-child) { margin-left: 0; }
</style>
