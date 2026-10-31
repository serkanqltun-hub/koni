# Run KONİ on your own server

KONİ has two parts: the desktop app and a small sync server (the *Santral*, `server/`).
The app ships with a default server, but anyone can run their own and point the app at it.
Your accounts, invites, friends and stats then live only on your server.

What goes through the server: room state, chat, presence and small JSON calls.
Video never does: every member plays the file, stream or link on their own machine.
A 1 vCPU / 1 GB VPS is plenty for a group of friends.

## What you need

- A Linux VPS with a public IP. The script below targets **Ubuntu 24.04+ / Debian 13+**
  (it uses `apt`, `systemd` and nginx).
- A domain or subdomain pointing at the server, e.g. `koni.example.com` (an `A` record;
  if you use Cloudflare, set it to *DNS only*).
- Ports 80 and 443 open. The server itself listens only on `127.0.0.1:8080`.

## Install

On the server, as root:

```sh
git clone https://github.com/serkanqltun-hub/koni /opt/koni/src
cd /opt/koni/src
DOMAIN=koni.example.com NGINX=1 ./deploy/install.sh
```

`install.sh` does the following:

1. installs Go (and lets it fetch the version in `server/go.mod`), creates a `koni` system user,
2. runs the server tests, builds `/opt/koni/koni-server`,
3. installs and starts the `koni` systemd service (`deploy/koni.service`),
4. with `NGINX=1`: installs nginx + certbot if missing, gets a Let's Encrypt certificate for
   `DOMAIN` and installs the site from `deploy/nginx-koni.conf`. Other nginx sites are not touched.
   Add `EMAIL=you@example.com` to get certificate expiry notices.

Check it: `curl https://koni.example.com/healthz` prints `ok`.

## Create the first account

Registration needs an invite code, also for the very first account. Make one on the server:

```sh
runuser -u koni -- /opt/koni/koni-server invite -db /var/lib/koni/koni.db            # valid 7 days
runuser -u koni -- /opt/koni/koni-server invite -db /var/lib/koni/koni.db -ttl 720h  # 30 days
```

Always run it as the `koni` user (files created by root break the service).
The first account becomes admin. After that, members invite friends from inside the app
(*Davet et* / Invite); admins see all rooms and members and can ban under *Yönetim* (Admin).

To give or take admin later:

```sh
runuser -u koni -- /opt/koni/koni-server admin -db /var/lib/koni/koni.db -list
runuser -u koni -- /opt/koni/koni-server admin -db /var/lib/koni/koni.db 'Name#1234'
runuser -u koni -- /opt/koni/koni-server admin -db /var/lib/koni/koni.db -off 'Name#1234'
```

## Connect the app

On the sign-in screen, click **Host your own server** (Turkish: *Kendi sunucunu kur*) at the bottom. The language switch is in the top-right corner of the same screen and type your domain
(`koni.example.com`; `wss://` and `/ws` are added for you). Then sign in or register with the code.

Invite links made on your server carry its address (`koni://invite/CODE?s=koni.example.com`),
so friends who open the link are switched to your server automatically.
To go back to the default server, sign out and use **Change → Back to invite sign-in** (Turkish: *Değiştir → Davetli girişe dön*).

## Optional features (API keys)

These are off until you add keys. Keys stay on the server; the app never sees them.

| Variable | Feature | Where to get it |
| --- | --- | --- |
| `KONI_TMDB_KEY` | "What should we watch?", movie/series search, posters, trailers | themoviedb.org → Settings → API (v3 key or v4 read token) |
| `KONI_OMDB_KEY` | IMDb / Rotten Tomatoes ratings | omdbapi.com/apikey.aspx |
| `KONI_OS_KEY` | Subtitle search | opensubtitles.com → API consumers |
| `KONI_OS_USER`, `KONI_OS_PASS` | Higher daily subtitle download quota | your OpenSubtitles account |

```sh
install -d -m 750 -o root -g koni /etc/koni
install -m 640 -o root -g koni /dev/null /etc/koni/env
nano /etc/koni/env          # KONI_TMDB_KEY=... one per line
systemctl restart koni && journalctl -u koni -n 3 --no-pager
```

The startup line lists what is on, e.g. `… accounts, db /var/lib/koni/koni.db, discover+omdb, subtitles`.

**Profile pictures:** members pick from the images in `/var/lib/koni/avatars`
(square `.jpg`/`.png`/`.webp`, owned by `koni`: `chown koni:koni /var/lib/koni/avatars/*`).
An admin's own uploaded photo is added there too. With none, members get letter avatars.

## Update

```sh
cd /opt/koni/src && git pull && ./deploy/install.sh
```

Tests run first; if they fail, the running version stays. The previous binary is kept as
`/opt/koni/koni-server.prev`. Restarting closes open rooms (people just open a new one);
accounts and history stay. When `deploy/nginx-koni.conf` changes, run with `NGINX=1`
(the domain is read from the installed site, so `DOMAIN` can be left out).

Roll back: `cp -f /opt/koni/koni-server.prev /opt/koni/koni-server && systemctl restart koni`.

Keep the server in step with the app: when a new app release comes out, update the server
from the same commit (the protocol version is `ProtocolVersion` in `server/hub/messages.go`).

## Backup and operations

- Data: `/var/lib/koni/` (`koni.db` + `-wal`/`-shm`, `avatars/`, `reports/`). Stop the service
  before copying the database, or use `sqlite3 koni.db ".backup out.db"`.
- Logs: `journalctl -u koni -f` · status: `systemctl status koni`.
- Bug reports sent from the app land in `/var/lib/koni/reports/`.

## Without nginx (LAN / testing)

```sh
cd server
go run ./cmd/koni-server -addr 0.0.0.0:8080 -db koni.db
go run ./cmd/koni-server invite -db koni.db
```

In the app, enter `192.168.1.10:8080` (or `localhost:8080`); private addresses use plain `ws://`.
Don't expose this to the internet without TLS: passwords and session tokens would travel unencrypted.

## Your own app build (optional)

To ship a build that points at your server by default, build the client with
`VITE_KONI_SERVER=wss://koni.example.com/ws` set (see `client/README.md`).
Auto-update follows the official releases (`plugins.updater` in
`client/src-tauri/tauri.conf.json`); change it and the signing key if you publish your own.
