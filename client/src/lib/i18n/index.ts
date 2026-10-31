// İngilizce sözlük: her dosya bir bölümün çevirileri (anahtar = Türkçe metin).
import app from "./app";
import night from "./night";
import home from "./home";
import views from "./views";
import panels from "./panels";
import misc from "./misc";
import rust from "./rust";
import { PATTERNS } from "./patterns";

export const EN: Record<string, string> = { ...misc, ...panels, ...views, ...home, ...night, ...app, ...rust };
export { PATTERNS };
