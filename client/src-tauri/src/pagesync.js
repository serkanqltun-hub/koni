// KONİ tarayıcıda senkron: Keşfet tarayıcısındaki her çerçeveye enjekte edilir
// (sniffer.rs). Sayfanın ana videosunu bulur, durumunu 0,5 sn'de bir Rust'a bildirir
// (`page_video`) ve KONİ'nin komutlarını (`window.__koniPage`) uygular. Kullanıcının
// sitenin oynatıcısında bastığı oynat/durdur/sar ayrıca bildirilir; KONİ'nin kendi
// verdiği komutların yankısı bildirilmez. Sayfanın davranışını başka türlü değiştirmez.
(() => {
  if (window.__koniPageSync) return;
  window.__koniPageSync = true;
  const isTop = window === window.top;

  // Ana video: görünür en büyük alan; oynayan öne geçer. Çok kısa (reklam/önizleme) geri plana düşer.
  const pick = () => {
    let best = null, bestScore = 0;
    for (const v of document.querySelectorAll("video")) {
      const r = v.getBoundingClientRect();
      const area = Math.max(0, r.width) * Math.max(0, r.height);
      if (area < 200 * 100) continue;
      const shortPenalty = isFinite(v.duration) && v.duration > 0 && v.duration < 10 ? 0.1 : 1;
      const score = area * (v.paused ? 1 : 4) * shortPenalty;
      if (score > bestScore) { best = v; bestScore = score; }
    }
    return best;
  };

  let video = null;
  let area = 0;
  // KONİ'nin verdiği komutun yankısı: bu türdeki olay bu zamana kadar kullanıcıdan sayılmaz.
  const quiet = { play: 0, pause: 0, seek: 0 };
  const hush = (k) => { quiet[k] = performance.now() + 900; };

  const snapshot = (user) => {
    const v = video;
    if (!v) return null;
    return {
      pos: v.currentTime || 0,
      paused: v.paused,
      rate: v.playbackRate || 1,
      dur: isFinite(v.duration) ? v.duration : null,
      buffering: !v.paused && v.readyState < 3,
      ended: v.ended,
      user: user || null,
    };
  };

  // Çerçeveler üst pencereye yollar; üst pencere en büyük videoyu Rust'a iletir.
  const frames = new Map(); // kaynak çerçeve → { area, report, at }
  const invoke = () => window.__TAURI_INTERNALS__?.invoke;
  const send = (report) => {
    const f = invoke();
    if (f) f("page_video", { report: { ...report, title: document.title || null } }).catch(() => {});
  };
  const deliver = (report, a, src) => {
    if (!isTop) return void window.top.postMessage({ koniPage: report, area: a }, "*");
    const now = performance.now();
    frames.set(src, { area: a, report, at: now });
    let top = null;
    for (const [k, f] of frames) {
      if (now - f.at > 2000) { frames.delete(k); continue; }
      if (!top || f.area > top.area) top = f;
    }
    // Kullanıcı hareketi hangi çerçeveden gelirse gelsin iletilir.
    if (top === frames.get(src) || report.user) send(report);
  };
  if (isTop) {
    addEventListener("message", (e) => {
      const d = e.data;
      if (d && d.koniPage && typeof d.area === "number") deliver(d.koniPage, d.area, e.source);
    });
  }

  const report = (user) => {
    const r = snapshot(user);
    if (r) deliver(r, area, window);
  };

  const onEvent = (e) => {
    const kind = e.type === "seeked" ? "seek" : e.type;
    // Kullanıcı hareketi: yankı değil ve az önce sayfada tıklama/tuş var (sitenin kendi
    // otomatik oynatması ya da kaldığı yere sarması odayı oynatmasın).
    const gesture = navigator.userActivation ? navigator.userActivation.isActive : true;
    const user = performance.now() > quiet[kind] && gesture ? kind : null;
    report(user);
  };
  const attach = (v) => {
    if (v === video) return;
    for (const t of ["play", "pause", "seeked"]) {
      video?.removeEventListener(t, onEvent);
      v?.addEventListener(t, onEvent);
    }
    video = v;
  };

  setInterval(() => {
    const v = pick();
    if (v) {
      const r = v.getBoundingClientRect();
      area = r.width * r.height;
    }
    attach(v);
    report(null);
  }, 500);

  // KONİ'nin komutları: üst pencereye gelir, çerçevelere dağıtılır.
  const apply = (c) => {
    const v = video;
    if (!v) return;
    try {
      if (c.op === "play" && v.paused) { hush("play"); v.play()?.catch?.(() => {}); }
      else if (c.op === "pause" && !v.paused) { hush("pause"); v.pause(); }
      else if (c.op === "seek" && isFinite(c.v)) { hush("seek"); v.currentTime = c.v; }
      else if (c.op === "rate" && c.v > 0) v.playbackRate = c.v;
    } catch {}
  };
  window.__koniPage = (c) => {
    apply(c);
    for (const f of document.querySelectorAll("iframe")) {
      try { f.contentWindow?.postMessage({ koniCmd: c }, "*"); } catch {}
    }
  };
  addEventListener("message", (e) => {
    const c = e.data?.koniCmd;
    if (!c || isTop) return;
    window.__koniPage(c);
  });
})();
