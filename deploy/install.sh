#!/usr/bin/env bash
# Build and (re)install KONİ Santral on an Ubuntu host. Run as root from an
# unpacked copy of the repo (see deploy/README.md):
#   ./deploy/install.sh            -> build, install binary + systemd unit, restart
#   DOMAIN=koni.example.com NGINX=1 ./deploy/install.sh
#                                  -> also install the nginx site (+ Let's Encrypt
#                                     certificate on first run) and reload nginx.
#                                     EMAIL=you@example.com is optional (certbot notices).
set -euo pipefail
SRC=$(cd "$(dirname "$0")/.." && pwd)

command -v go >/dev/null || DEBIAN_FRONTEND=noninteractive apt-get install -y -q golang-go
id koni >/dev/null 2>&1 || useradd --system --no-create-home --shell /usr/sbin/nologin koni
mkdir -p /opt/koni /var/lib/koni/avatars /var/lib/koni/reports
chown -R koni:koni /var/lib/koni/avatars /var/lib/koni/reports 2>/dev/null || true

cd "$SRC/server"
go test ./...
CGO_ENABLED=0 go build -trimpath -ldflags='-s -w' -o /opt/koni/koni-server.new ./cmd/koni-server
# keep the running binary for rollback (see deploy/README.md "Geri alma")
[ -f /opt/koni/koni-server ] && cp -f /opt/koni/koni-server /opt/koni/koni-server.prev
mv /opt/koni/koni-server.new /opt/koni/koni-server

install -m 644 "$SRC/deploy/koni.service" /etc/systemd/system/koni.service
systemctl daemon-reload
systemctl enable koni >/dev/null
systemctl restart koni
sleep 1
curl -fsS http://127.0.0.1:8080/healthz && echo " <- koni healthz"
journalctl -u koni -n 20 --no-pager | grep -o 'koni-server p[0-9.]* listening.*' | tail -n 1

if [ "${NGINX:-0}" = 1 ]; then
  # Domain: DOMAIN=koni.example.com, or the one already in the installed site file.
  DOMAIN=${DOMAIN:-$(awk '/server_name/ {gsub(";", "", $2); print $2; exit}' /etc/nginx/sites-available/koni 2>/dev/null || true)}
  if [ -z "$DOMAIN" ] || [ "$DOMAIN" = koni.example.com ]; then
    echo "set DOMAIN, e.g. DOMAIN=koni.example.com NGINX=1 $0" >&2; exit 1
  fi
  command -v nginx >/dev/null || DEBIAN_FRONTEND=noninteractive apt-get install -y -q nginx
  command -v certbot >/dev/null || DEBIAN_FRONTEND=noninteractive apt-get install -y -q certbot
  mkdir -p /var/www/koni-acme
  if [ ! -f /etc/letsencrypt/live/$DOMAIN/fullchain.pem ]; then
    # HTTP-only block first, so the ACME challenge can be answered
    sed -n '1,/^}/p' "$SRC/deploy/nginx-koni.conf" | sed "s/koni\.example\.com/$DOMAIN/g" > /etc/nginx/sites-available/koni
    ln -sf ../sites-available/koni /etc/nginx/sites-enabled/koni
    nginx -t && systemctl reload nginx
    if [ -n "${EMAIL:-}" ]; then reg=(--email "$EMAIL"); else reg=(--register-unsafely-without-email); fi
    certbot certonly --webroot -w /var/www/koni-acme -d "$DOMAIN" --non-interactive --agree-tos \
      "${reg[@]}" --deploy-hook "systemctl reload nginx"
  fi
  sed "s/koni\.example\.com/$DOMAIN/g" "$SRC/deploy/nginx-koni.conf" > /etc/nginx/sites-available/koni
  chmod 644 /etc/nginx/sites-available/koni
  ln -sf ../sites-available/koni /etc/nginx/sites-enabled/koni
  nginx -t && systemctl reload nginx
fi
