// README demo: runs the Svelte UI in a plain browser with a fake Tauri layer
// (tools/demo/fake-tauri.js), then writes docs/img/readme/demo.webp.
// Usage: see tools/demo/README.md
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createRequire } from "node:module";

const root = resolve(import.meta.dirname, "../..");
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || "playwright");
const out = join(root, "docs/img/readme/demo.webp");
const tmp = process.env.DEMO_TMP || mkdtempSync(join(tmpdir(), "koni-demo-"));
const procs = [];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const run = (cmd, args, opts) => { const p = spawn(cmd, args, { stdio: "ignore", ...opts }); procs.push(p); return p; };

try {
  run("npx", ["vite", "--port", "1420"], { cwd: join(root, "client") });
  await sleep(5000);

  const browser = await chromium.launch();
  const ctx = await browser.newContext({
    viewport: { width: 1180, height: 720 }, deviceScaleFactor: 1,
    recordVideo: { dir: tmp, size: { width: 1180, height: 720 } },
  });
  const page = await ctx.newPage();
  await page.addInitScript({ path: join(import.meta.dirname, "fake-tauri.js") });
  const shot = async (name) => page.screenshot({ path: join(tmp, name + ".png") });
  await page.goto("http://localhost:1420/");
  await sleep(3000);                                   // home: open rooms, friends, stats
  await page.getByText("Activity", { exact: true }).click();
  await sleep(2000);
  await page.getByRole("button", { name: "Open player" }).click();
  await sleep(3500);                                   // Discover
  await shot("discover");
  await page.getByText("Cinema", { exact: true }).first().click();
  await sleep(3500);
  await shot("cinema");
  await page.getByText("Music", { exact: true }).first().click();
  await sleep(3500);
  await shot("music");
  await ctx.close();
  await browser.close();
  const webm = join(tmp, readdirSync(tmp).find((f) => f.endsWith(".webm")));
  execFileSync("ffmpeg", ["-y", "-v", "error", "-ss", "2", "-i", webm, "-vf", "fps=10,scale=860:-1:flags=lanczos",
    "-loop", "0", "-quality", "70", "-compression_level", "6", out]);
  console.log("wrote", out);
} finally {
  procs.forEach((p) => p.kill());
  if (!process.env.DEMO_TMP) rmSync(tmp, { recursive: true, force: true });
}
