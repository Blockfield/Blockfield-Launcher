#!/bin/sh
set -eu

mkdir -p /data/storage/app /app/storage
rm -rf /app/storage/app
ln -s /data/storage/app /app/storage/app
touch /data/database.sqlite

if [ -z "${APP_KEY:-}" ]; then
  export APP_KEY="$(php -r 'echo "base64:".base64_encode(random_bytes(32));')"
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

# ponytail: if DB has stale migration history (tables missing but recorded as run),
# fall back to fresh migration. SQLite3 CLI may not be available; php -r is.
if ! php -r "echo (new PDO('sqlite:' . (getenv('DB_DATABASE') ?: '/data/database.sqlite')))->query('SELECT count(*) FROM sqlite_master WHERE type=\"table\" AND name=\"feature_cards\"')->fetchColumn();" 2>/dev/null | grep -q 1; then
  echo "=> Tables missing — running fresh migration"
  php artisan migrate:fresh --force
fi

php artisan db:seed --force
php artisan optimize:clear >/dev/null

exec frankenphp run --config /etc/frankenphp/Caddyfile
