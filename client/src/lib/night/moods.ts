// Ne izlesek / Ne izlesem hava kutuları (TMDB tür kimlikleri). Başlıklar t() ile çevrilir.
import type { NightFilter } from "../player";

export interface MoodTile { id: string; title: string; sub: string; grad: string; f: NightFilter }
export const MOODS: MoodTile[] = [
  { id: "gerim", title: "Gerim gerim", sub: "Gerilim, gizem, son sahnede ağzın açık kalsın", grad: "linear-gradient(135deg,#3a1414,#7a1f1f)", f: { genres: [53, 9648, 80], voteMin: 6.8 } },
  { id: "hafif", title: "Kafa dağıtmalık", sub: "Hafif komedi, kafa yormaz", grad: "linear-gradient(135deg,#3a2e0e,#8a6a12)", f: { genres: [35], without: [27, 53], voteMin: 6.3 } },
  { id: "bilim", title: "Akıl yakan bilimkurgu", sub: "Büyük fikirler, büyük ekran", grad: "linear-gradient(135deg,#0e2a3a,#14526f)", f: { genres: [878], voteMin: 7 } },
  { id: "kalp", title: "Kalbe dokunsun", sub: "Romantik, biraz da hüzün", grad: "linear-gradient(135deg,#2a0e2a,#6a1a5a)", f: { genres: [10749, 18], voteMin: 7 } },
  { id: "korku", title: "Işıkları kapat", sub: "Korku; yalnız izlenmez", grad: "linear-gradient(135deg,#1a1a1a,#4a4a4a)", f: { genres: [27], voteMin: 6.2 } },
  { id: "herkes", title: "Hepimize uyar", sub: "Animasyon, her yaşa", grad: "linear-gradient(135deg,#0e3a20,#1f7a3f)", f: { genres: [16, 10751], voteMin: 7 } },
];
