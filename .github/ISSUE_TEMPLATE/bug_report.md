---
name: Bug report
about: Something doesn't work as expected
title: "[Bug] "
labels: bug
---

**What happened?**
<!-- A clear description. Screenshots or a short screen recording help a lot. -->

**What did you expect?**

**Steps to reproduce**
1.
2.
3.

**Where**
- KONİ version: <!-- Settings → About -->
- OS and version: <!-- e.g. Windows 11 23H2, macOS 15.1 -->
- Machine / CPU / GPU: <!-- e.g. MacBook Air M2, Ryzen 5 + RTX 3060 -->
- Room mode: <!-- Cinema / Music / Discover / What should we watch? -->
- What was playing: <!-- local file (container + codec if known, e.g. MKV H.265 10-bit) or the kind of link; please don't paste private links -->
- Your own server or a friend's? Server and app on the same version?

### Video and playback (fill in if the problem is video, sound or freezing)
- [ ] Black or frozen video, but sound plays
- [ ] No sound
- [ ] Stutter / dropped frames
- [ ] App crashes when a video opens
- [ ] "couldn't load libmpv" message
- Does it also happen with a different file? With a smaller (1080p, H.264) file?
- macOS only: did you run `xattr -cr /Applications/KONI.app` after installing? Does it change anything?
- Hardware acceleration: KONİ plays with `hwdec=auto-safe` (VideoToolbox on Mac, D3D11VA on Windows). If you have [mpv](https://mpv.io) installed, does the same file play fine with `mpv --hwdec=auto-safe file` and with `mpv --hwdec=no file`? This tells a decoder problem apart from a KONİ problem.

### Sync (fill in if members drift apart or drop out)
- How many people in the room, and on which OS each?
- [ ] Someone falls behind and never catches up
- [ ] The room pauses again and again ("waiting for …")
- [ ] Someone gets disconnected / "reconnecting" loops
- [ ] Play/pause/seek doesn't reach everyone
- Roughly when did it start (minutes into the video)? Did *Sync with room* fix it?
- Connection: Wi-Fi or cable? VPN on?

**Logs**
<!--
Sync logs (one CSV per session, attach the newest one):
  Windows: %LOCALAPPDATA%\app.koni.desktop\logs\
  macOS:   ~/Library/Logs/app.koni.desktop/
Remove anything you don't want to share before attaching.
-->
