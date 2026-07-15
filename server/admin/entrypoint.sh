#!/bin/sh
set -eu

mkdir -p /data/storage/app /app/storage
rm -rf /app/storage/app
ln -s /data/storage/app /app/storage/app
touch /data/database.sqlite

export APP_KEY="${APP_KEY:-${FILAMENT_APP_KEY:-}}"
export APP_URL="${APP_URL:-${FILAMENT_APP_URL:-http://localhost:8055}}"
export CMS_TOKEN="${CMS_TOKEN:-}"
export AUTH_SIGNING_KEY="${AUTH_SIGNING_KEY:-}"
export RELOAD_TOKEN="${RELOAD_TOKEN:-}"

if [ -z "$APP_KEY" ]; then
  echo "APP_KEY or FILAMENT_APP_KEY is required and must remain stable." >&2
  exit 1
fi
if [ "${#CMS_TOKEN}" -lt 32 ]; then
  echo "CMS_TOKEN is required and must be at least 32 characters." >&2
  exit 1
fi
if [ "${#AUTH_SIGNING_KEY}" -lt 32 ]; then
  echo "AUTH_SIGNING_KEY is required and must be at least 32 characters." >&2
  exit 1
fi
if [ "${#RELOAD_TOKEN}" -lt 32 ]; then
  echo "RELOAD_TOKEN is required and must be at least 32 characters." >&2
  exit 1
fi

cat > .env <<EOF
APP_NAME=Blockfield
APP_ENV=${APP_ENV:-production}
APP_KEY=${APP_KEY}
APP_DEBUG=${APP_DEBUG:-false}
APP_URL=${APP_URL}
LOG_CHANNEL=stack
LOG_LEVEL=info
DB_CONNECTION=sqlite
DB_DATABASE=${DB_DATABASE:-/data/database.sqlite}
SESSION_DRIVER=file
CACHE_STORE=file
QUEUE_CONNECTION=sync
FILESYSTEM_DISK=${FILESYSTEM_DISK:-local}
CMS_TOKEN=${CMS_TOKEN:-}
AUTH_SIGNING_KEY=${AUTH_SIGNING_KEY:-}
RELOAD_TOKEN=${RELOAD_TOKEN:-}
BLOCKFIELD_API_URL=${BLOCKFIELD_API_URL:-http://blockfield-api:3000/api/launcher/v1}
EOF

php artisan migrate --force

php artisan db:seed --force
php artisan optimize:clear >/dev/null

exec frankenphp run --config /etc/frankenphp/Caddyfile
