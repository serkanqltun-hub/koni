# Builds the Windows installer (NSIS, per-user) with the LGPL libmpv package and yt-dlp.
#   pwsh client/scripts/build-installer.ps1 -Libmpv <libmpv-lgpl-win64.zip | extracted folder> [-Portable]
# The libmpv package is the "libmpv-lgpl-win64" asset of the libmpv-v* release / workflow
# artifact (docs/LIBMPV.md). Output: client/target/release/bundle/nsis/KONI_<version>_x64-setup.exe
# and, with -Portable, KONI_<version>_x64-portable.zip next to it (same files, no installer).
# With TAURI_SIGNING_PRIVATE_KEY (+ _PASSWORD) set, the updater signature
# KONI_<version>_x64-setup.exe.sig is produced too (docs/UPDATES.md).
param(
  [Parameter(Mandatory = $true)][string]$Libmpv,
  [string]$YtDlpUrl = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
  [switch]$Portable
)
$ErrorActionPreference = "Stop"
$client = Split-Path -Parent $PSScriptRoot
$vendor = Join-Path $client "src-tauri/vendor/win64"
if (Test-Path $vendor) { Remove-Item -Recurse -Force $vendor }
New-Item -ItemType Directory -Force $vendor | Out-Null

# libmpv: the artifact zip contains another zip; accept either, or an extracted folder.
$src = $Libmpv
if ($Libmpv -like "*.zip") {
  $tmp = Join-Path ([IO.Path]::GetTempPath()) "koni-libmpv-$PID"
  Expand-Archive -Force $Libmpv $tmp
  $inner = Get-ChildItem $tmp -Filter "libmpv-lgpl-win64.zip" -Recurse | Select-Object -First 1
  if ($inner) { Expand-Archive -Force $inner.FullName $tmp }
  $src = $tmp
}
$dll = Get-ChildItem $src -Filter libmpv-2.dll -Recurse | Select-Object -First 1
if (-not $dll) { throw "libmpv-2.dll not found in $Libmpv" }
if (-not (Test-Path (Join-Path $dll.DirectoryName "NOTICE.md"))) { throw "NOTICE.md missing: not the LGPL package?" }
# DLLs next to koni.exe; NOTICE.md, LICENSES/, source/, BUILDINFO.txt, REBUILD.md ship too (LGPL).
Copy-Item -Recurse -Force (Join-Path $dll.DirectoryName "*") $vendor
Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $vendor "SHA256SUMS.txt")

Invoke-WebRequest -UseBasicParsing $YtDlpUrl -OutFile (Join-Path $vendor "yt-dlp.exe")

# uBlock Origin Lite (GPL-3.0, github.com/uBlockOrigin/uBOL-home): Keşfet tarayıcısına
# eklenti olarak yüklenir (sniffer.rs, exe'nin yanındaki extensions/). İnmezse kurulum
# yine çıkar, Keşfet yalnızca KONİ'nin kendi reklam listesiyle çalışır.
try {
  $hdr = @{ "User-Agent" = "koni-build" }
  if ($env:GH_TOKEN) { $hdr["Authorization"] = "Bearer $env:GH_TOKEN" }
  $rel = Invoke-RestMethod -Headers $hdr "https://api.github.com/repos/uBlockOrigin/uBOL-home/releases/latest"
  $asset = $rel.assets | Where-Object { $_.name -like "*chromium*.zip" } | Select-Object -First 1
  if (-not $asset) { throw "chromium paketi yok ($($rel.tag_name))" }
  $tmpExt = Join-Path ([IO.Path]::GetTempPath()) "koni-ubol-$PID"
  $zip = "$tmpExt.zip"
  Invoke-WebRequest -UseBasicParsing $asset.browser_download_url -OutFile $zip
  Expand-Archive -Force $zip $tmpExt
  $man = Get-ChildItem $tmpExt -Filter manifest.json -Recurse | Sort-Object { $_.FullName.Length } | Select-Object -First 1
  if (-not $man) { throw "manifest.json yok" }
  # Bütün sitelerde izin kurulumda verilsin: uBOL açılır pencere sormadan tam modda
  # (öğe gizleme + YouTube betikleri) çalışır.
  $m = Get-Content $man.FullName -Raw | ConvertFrom-Json
  $m | Add-Member -Force -NotePropertyName host_permissions -NotePropertyValue @("<all_urls>")
  [IO.File]::WriteAllText($man.FullName, ($m | ConvertTo-Json -Depth 50), (New-Object System.Text.UTF8Encoding $false))
  $ext = Join-Path $vendor "extensions/ubol"
  New-Item -ItemType Directory -Force $ext | Out-Null
  Copy-Item -Recurse -Force (Join-Path $man.DirectoryName "*") $ext
  Write-Host "uBOL $($rel.tag_name) eklendi"
} catch {
  Write-Warning "uBlock Origin Lite eklenemedi: $_"
}

$configs = @("--config", "src-tauri/installer.conf.json")
if ($env:TAURI_SIGNING_PRIVATE_KEY) { $configs += @("--config", "src-tauri/updater.conf.json") }
Push-Location $client
try {
  npm run tauri -- build @configs
  if ($LASTEXITCODE -ne 0) { throw "tauri build failed" }
} finally { Pop-Location }
$nsis = Join-Path $client "target/release/bundle/nsis"
if ($Portable) {
  $version = (Get-Content (Join-Path $client "src-tauri/tauri.conf.json") -Raw | ConvertFrom-Json).version
  $root = Join-Path ([IO.Path]::GetTempPath()) "koni-portable-$PID"
  $stage = Join-Path $root "KONI"
  New-Item -ItemType Directory -Force $stage | Out-Null
  Copy-Item -Recurse -Force (Join-Path $vendor "*") $stage
  Copy-Item -Force (Join-Path $client "target/release/koni.exe") $stage
  $zip = Join-Path $nsis "KONI_${version}_x64-portable.zip"
  Remove-Item -Force -ErrorAction SilentlyContinue $zip
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  [IO.Compression.ZipFile]::CreateFromDirectory($root, $zip, [IO.Compression.CompressionLevel]::Optimal, $false)
  Remove-Item -Recurse -Force $root
}
Get-ChildItem $nsis -File | Where-Object { $_.Extension -in ".exe", ".zip", ".sig" }
