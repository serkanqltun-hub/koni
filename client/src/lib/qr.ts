import qrcode from "qrcode-generator";

// Metni QR'a çevirir; beyaz zeminli, 4 modül boşluklu SVG data URL döner (telefon kamerası için).
export function qrDataUrl(text: string): string {
  const q = qrcode(0, "M");
  q.addData(text);
  q.make();
  const n = q.getModuleCount();
  const m = 4;
  let d = "";
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) if (q.isDark(y, x)) d += `M${x + m} ${y + m}h1v1h-1z`;
  const s = n + m * 2;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${s} ${s}" shape-rendering="crispEdges"><rect width="${s}" height="${s}" fill="#fff"/><path d="${d}" fill="#000"/></svg>`;
  return "data:image/svg+xml;utf8," + encodeURIComponent(svg);
}
