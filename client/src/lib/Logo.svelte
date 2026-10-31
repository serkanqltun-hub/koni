<script lang="ts">
  // KONİ glifi (ikon I3): oynat üçgenine dönmüş koni, içinde iki şerit.
  let { size = 16 }: { size?: number } = $props();
  const id = `koni-g-${Math.random().toString(36).slice(2, 8)}`;
</script>

<svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
  <defs><linearGradient {id} x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#5ab0ff" /><stop offset="1" stop-color="#0a6cff" /></linearGradient></defs>
  <path d="M6 3.5v17a1 1 0 0 0 1.5.8l13-8.5a1 1 0 0 0 0-1.6l-13-8.5A1 1 0 0 0 6 3.5z" fill="url(#{id})" />
  <path d="M9.6 7.8v8.4l1.9-1.25V9.05zM13.3 10.2v3.6l1.9-1.2v-1.2z" fill="#fff" />
</svg>
