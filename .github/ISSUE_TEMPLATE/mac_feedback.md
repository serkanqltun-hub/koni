---
name: macOS feedback
about: You tried KONİ on a Mac. Tell us how it went, good or bad.
title: "[macOS] "
labels: macos
---

<!--
Thank you! The maintainer doesn't own a Mac: KONİ for macOS is built and tested only in
GitHub Actions. Your report is often the first time a feature ran on real hardware.
"Everything worked" is a useful report too.
-->

**Your Mac**
- Model and chip: <!-- e.g. MacBook Pro 14" M3 Pro -->
- macOS version: <!-- Apple menu → About This Mac -->
- KONİ version: <!-- Settings → About -->
- Display: <!-- built-in only / external monitor / scaled resolution -->

**Install and first launch**
- [ ] Opened after *Open Anyway* in System Settings → Privacy & Security
- [ ] Needed `xattr -cr /Applications/KONI.app` to open
- [ ] Needed `codesign --force --deep --sign - /Applications/KONI.app` to play video (should not be needed since 1.0.4)
- [ ] Didn't open at all

**What did you try?**
| Feature | Works? | Notes |
|:--|:--|:--|
| Cinema: local file | | |
| Cinema: link | | |
| Music mode | | |
| Discover: *Watch in KONİ* | | |
| What should we watch? | | |
| Full screen / window resize | | |
| Sync with a friend (which OS?) | | |
| Auto-update to a newer version | | |

**What went wrong?**
<!-- Video black/frozen/stuttering, wrong position or size, no sound, high CPU or heat, sync problems… -->

**Crash report** (if KONİ quit unexpectedly)
<!--
macOS shows "KONİ quit unexpectedly" → click *Report…* and copy the text, or find it in
Console.app → Crash Reports (files named KONI-*.ips), or in ~/Library/Logs/DiagnosticReports/.
Paste the top part (until the first "Thread" list ends) inside the block below.
-->
```
paste crash log here
```

**Terminal output** (optional, very helpful)
<!-- Start the app from Terminal and paste what it prints around the problem: -->
<!-- /Applications/KONI.app/Contents/MacOS/koni -->
```
paste output here
```

**Sync log** (for sync issues): attach the newest CSV from `~/Library/Logs/app.koni.desktop/`.
