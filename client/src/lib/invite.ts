// Davet linki: koni://join/<kod>. Paylaşılan metinden ya da düz koddan kodu ayıklar.
const LINK = /koni:\/\/join\/([^\s/?#)]+)/i;
const KOD = /\((?:kod|code):\s*([^\s)]+)\)/i;

export function parseInvite(text: string): string {
  const t = text.trim();
  const m = t.match(LINK) ?? t.match(KOD);
  return m ? decodeURIComponent(m[1]) : t;
}

