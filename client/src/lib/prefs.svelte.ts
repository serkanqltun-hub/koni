// Küçük yerel tercihler (localStorage). Sunucuya gitmez.
function flag(key: string, def: boolean) {
  let v = $state(localStorage.getItem(key) == null ? def : localStorage.getItem(key) === "1");
  return {
    get on() { return v; },
    set on(x: boolean) { v = x; localStorage.setItem(key, x ? "1" : "0"); },
  };
}
const fs = flag("koni.fullscreenStart", false);
const spoilers = flag("koni.hideSpoilers", false);

export const prefs = {
  /** Video açılınca tam ekrana geç. */
  get fullscreenStart() { return fs.on; },
  set fullscreenStart(x: boolean) { fs.on = x; },
  /** Dizide izlemediğin bölümlerin küçük resmi/adı bulanık. */
  get hideSpoilers() { return spoilers.on; },
  set hideSpoilers(x: boolean) { spoilers.on = x; },
};
