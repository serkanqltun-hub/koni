# README demo

`record.mjs` runs the Svelte interface in a headless browser with a fake Tauri layer
(`fake-tauri.js`, sample data only), clicks through a few screens and writes
`docs/img/readme/demo.webp`.

Needs Node 22, `ffmpeg` and Playwright (`npm i playwright` anywhere, then
`PLAYWRIGHT_PATH=/path/to/node_modules/playwright`; not needed if `require('playwright')` already works).

```sh
cd client && npm ci && cd ..
node tools/demo/record.mjs
```

To change the tour, edit the clicks in `record.mjs`; to change what the screens show, edit `fake-tauri.js`.
