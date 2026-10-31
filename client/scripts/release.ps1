# GitHub Actions olmadan sürüm çıkarır (Serkan'ın makinesinde). release.yml'in yaptığını yapar:
#   1. LGPL libmpv paketini indirir, imzalı kurulum + taşınabilir zip derler (build-installer.ps1)
#   2. Dosyaları bu reponun v<sürüm> release'ine koyar
#   3. Kurulum + latest.json'u public Savas07/koni-releases reposunun `incoming` dalına iter;
#      o reponun publish iş akışı (public repo, dakika harcamaz) Release'i açar, uygulamalar güncellenir.
# Kullanım (client klasöründe):  powershell -ExecutionPolicy Bypass -File scripts/release.ps1
# Önce tauri.conf.json + Cargo.toml sürümünü artır, docs/releases/v<sürüm>.md yaz, main'e koy.
# Gerekenler: gh (gh auth login), git, imza anahtarı (%USERPROFILE%\.koni-updater\ ya da
# TAURI_SIGNING_PRIVATE_KEY / _PASSWORD), Savas07/koni-releases'e yazma izni.
param(
  [string]$Libmpv = "libmpv-v0.40.0-r2",
  [switch]$SkipBuild
)
$ErrorActionPreference = "Stop"
$client = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $client
$v = (Get-Content (Join-Path $client "src-tauri/tauri.conf.json") -Raw | ConvertFrom-Json).version
$tag = "v$v"
Write-Host "KONİ $tag hazırlanıyor" -ForegroundColor Cyan

foreach ($t in "gh", "git", "npm", "cargo") {
  if (-not (Get-Command $t -ErrorAction SilentlyContinue)) { throw "$t bulunamadı" }
}
gh release view $tag -R Savas07/koni-releases *> $null
if ($LASTEXITCODE -eq 0) { throw "$tag zaten yayınlı (Savas07/koni-releases). Önce sürümü artır." }

# İmza anahtarı: ortam değişkeni yoksa anahtarı üreten bilgisayardaki yedek.
if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
  $dir = Join-Path $env:USERPROFILE ".koni-updater"
  $key = Join-Path $dir "koni-updater.key"
  if (-not (Test-Path $key)) {
    throw "İmza anahtarı yok: $key. Anahtarı üreten bilgisayardan kopyala ya da TAURI_SIGNING_PRIVATE_KEY ayarla (docs/UPDATES.md)."
  }
  $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw $key
  $pw = Join-Path $dir "password.txt"
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = if (Test-Path $pw) { (Get-Content -Raw $pw).Trim() } else { "" }
}

$nsis = Join-Path $client "target/release/bundle/nsis"
$setup = "KONI_${v}_x64-setup.exe"
$zip = "KONI_${v}_x64-portable.zip"
if (-not $SkipBuild) {
  $tmp = Join-Path ([IO.Path]::GetTempPath()) "koni-release-$PID"
  New-Item -ItemType Directory -Force $tmp | Out-Null
  gh release download $Libmpv -R serkanqltun-hub/koni -p libmpv-lgpl-win64.zip -D $tmp --clobber
  if ($LASTEXITCODE -ne 0) { throw "libmpv paketi indirilemedi ($Libmpv)" }
  & (Join-Path $PSScriptRoot "build-installer.ps1") -Libmpv (Join-Path $tmp "libmpv-lgpl-win64.zip") -Portable
  Remove-Item -Recurse -Force $tmp
}
foreach ($f in $setup, "$setup.sig", $zip) {
  if (-not (Test-Path (Join-Path $nsis $f))) { throw "eksik: $f (imza anahtarı doğru mu?)" }
}

# Notlar
$notesFile = Join-Path $repoRoot "docs/releases/$tag.md"
$notes = if (Test-Path $notesFile) { (Get-Content $notesFile -Raw -Encoding utf8).Trim() } else { "KONİ $v" }

# 1) Bu reponun release'i (kayıt için)
$files = @((Join-Path $nsis $setup), (Join-Path $nsis $zip))
gh release view $tag -R serkanqltun-hub/koni *> $null
if ($LASTEXITCODE -eq 0) {
  gh release upload $tag @files -R serkanqltun-hub/koni --clobber
} else {
  $sha = git -C $repoRoot rev-parse HEAD
  gh release create $tag @files -R serkanqltun-hub/koni --target $sha --prerelease --title "KONİ $v (Windows)" --notes $notes
}
if ($LASTEXITCODE -ne 0) { throw "gh release başarısız" }

# 2) Public dağıtım reposu: incoming dalına tek commit
$out = Join-Path ([IO.Path]::GetTempPath()) "koni-releases-$PID"
git clone -q --depth 1 --branch main https://github.com/Savas07/koni-releases.git $out
if ($LASTEXITCODE -ne 0) { throw "Savas07/koni-releases klonlanamadı" }
Copy-Item (Join-Path $nsis $setup), (Join-Path $nsis "$setup.sig"), (Join-Path $nsis $zip) $out
$utf8 = New-Object System.Text.UTF8Encoding $false
[IO.File]::WriteAllText((Join-Path $out "notes.md"), $notes, $utf8)
$latest = [ordered]@{
  version   = $v
  notes     = $notes
  pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
  platforms = @{ "windows-x86_64" = @{
    signature = (Get-Content (Join-Path $nsis "$setup.sig") -Raw).Trim()
    url       = "https://github.com/Savas07/koni-releases/releases/download/$tag/$setup"
  } }
} | ConvertTo-Json -Depth 4
[IO.File]::WriteAllText((Join-Path $out "latest.json"), $latest, $utf8)
Push-Location $out
try {
  git checkout -q -b incoming
  git add -A
  git -c user.name=koni-release -c user.email=koni-release@users.noreply.github.com commit -q -m "KONİ $v"
  git push -f origin incoming
  if ($LASTEXITCODE -ne 0) { throw "koni-releases'e push edilemedi (yazma izni?)" }
} finally { Pop-Location }
Remove-Item -Recurse -Force $out
Write-Host "Tamam: $tag yüklendi. koni-releases publish iş akışı birkaç dakikada Release'i açar." -ForegroundColor Green
