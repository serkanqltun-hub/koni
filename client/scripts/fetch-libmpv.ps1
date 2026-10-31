# Downloads a prebuilt libmpv (dev build) and prints the path to libmpv-2.dll.
# NOTE: shinchiro builds may be GPL. Dev/CI use only; distribution will use the
# LGPL artifact from workflow 'libmpv (LGPL, Windows x64)' (separate PR).
param([string]$Out = "libmpv-dev")
$ErrorActionPreference = "Stop"
$repo = "shinchiro/mpv-winbuild-cmake"
$assets = gh release view --repo $repo --json assets | ConvertFrom-Json
# x86_64 only: skip the -v3 variant.
$name = ($assets.assets.name | Where-Object { $_ -like "mpv-dev-x86_64-*.7z" -and $_ -notlike "*-v3-*" } | Select-Object -First 1)
if (-not $name) { throw "no mpv-dev-x86_64 asset found in $repo" }
gh release download --repo $repo --pattern $name --dir $Out --clobber
7z x -y "-o$Out" (Join-Path $Out $name) | Out-Null
$dll = Get-ChildItem $Out -Recurse -Filter libmpv-2.dll | Select-Object -First 1
if (-not $dll) { throw "libmpv-2.dll not found in $name" }
$dll.FullName
