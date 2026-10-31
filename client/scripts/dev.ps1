# Geliştirme: uygulamayı CI beklemeden yerelde açar (npx tauri dev).
# Arayüz değişiklikleri anında yenilenir; Rust değişince yeniden derler.
# libmpv-2.dll ve yt-dlp.exe kurulu KONİ'den (%LOCALAPPDATA%\KONI) kopyalanır.
#   powershell -ExecutionPolicy Bypass -File scripts/dev.ps1 [-Pull]
param([switch]$Pull)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

foreach ($c in "node", "npm", "cargo") {
  if (-not (Get-Command $c -ErrorAction SilentlyContinue)) {
    throw "$c bulunamadı. Node 22 (https://nodejs.org) ve Rust (https://rustup.rs, MSVC) kur."
  }
}
if ($Pull) {
  # cargo, sürüm artınca Cargo.lock'u yerelde günceller; pull'u engellemesin.
  git checkout -- Cargo.lock 2>$null
  git pull --ff-only
}

$lock = "package-lock.json"
$stamp = "node_modules/.koni-lock"
if (-not (Test-Path $stamp) -or (Get-Content $stamp -Raw) -ne (Get-FileHash $lock).Hash) {
  npm ci
  (Get-FileHash $lock).Hash | Set-Content $stamp -NoNewline
}

$bin = "target/debug"
New-Item -ItemType Directory -Force $bin | Out-Null
if (-not (Test-Path "$bin/libmpv-2.dll")) {
  $inst = Join-Path $env:LOCALAPPDATA "KONI"
  if (-not (Test-Path "$inst/libmpv-2.dll")) { throw "KONİ kurulu değil: $inst içinde libmpv-2.dll yok. Önce kurulumu çalıştır." }
  Copy-Item "$inst/*.dll" $bin
  if (Test-Path "$inst/yt-dlp.exe") { Copy-Item "$inst/yt-dlp.exe" $bin }
}

npx tauri dev
