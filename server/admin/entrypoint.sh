#!/bin/sh
set -eu

mkdir -p /data/storage/app /app/storage
rm -rf /app/storage/app
ln -s /data/storage/app /app/storage/app
touch /data/database.sqlite

# ── Persist APP_KEY on the volume so it survives restarts ────────────
#    Other env vars are picked up fresh from Docker on every start.
if [ -z "${APP_KEY:-}" ]; then
  if [ -f /data/app-key ]; then
    APP_KEY="$(cat /data/app-key)"
  else
    APP_KEY="$(php -r 'echo "base64:".base64_encode(random_bytes(32));')"
    echo "$APP_KEY" > /data/app-key
  fi
fi

cat > .env <<EOF
APP_NAME=Blockfield
APP_ENV=${APP_ENV:-production}
APP_KEY=${APP_KEY}
APP_DEBUG=${APP_DEBUG:-false}
APP_URL=${APP_URL:-http://localhost:8055}
LOG_CHANNEL=stack
LOG_LEVEL=info
DB_CONNECTION=sqlite
DB_DATABASE=${DB_DATABASE:-/data/database.sqlite}
SESSION_DRIVER=file
CACHE_STORE=file
QUEUE_CONNECTION=sync
FILESYSTEM_DISK=${FILESYSTEM_DISK:-local}
CMS_TOKEN=${CMS_TOKEN:-}
EOF

php artisan migrate --force
php artisan db:seed --force
php artisan optimize:clear >/dev/null

exec frankenphp run --config /etc/frankenphp/Caddyfile
