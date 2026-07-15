FROM dunglas/frankenphp:1-php8.3-bookworm@sha256:968e68e09a2a5ecd5adc3080902f0cffcc3cc84823876ec687900bce3f27894e

RUN apt-get update && apt-get install -y git unzip zip \
    && install-php-extensions intl pdo_sqlite zip opcache \
    && rm -rf /var/lib/apt/lists/*

RUN echo "upload_max_filesize=2G\npost_max_size=2G\nmax_execution_time=300\nmax_input_time=300" \
    > /usr/local/etc/php/conf.d/uploads.ini

COPY --from=composer:2@sha256:5946476338742b200bb9ff88f8be56275ddae4b3949c72305cb0dbf10cfcb760 /usr/bin/composer /usr/bin/composer

WORKDIR /app

# ── Locked PHP dependency layer ────────────────────────────────────────────────
COPY composer.json composer.lock /tmp/blockfield-composer/
RUN rm -rf /app/* /app/.[!.]* /app/..?* \
    && composer create-project laravel/laravel:v12.12.2 . --no-interaction --prefer-dist --no-install --no-scripts \
    && cp /tmp/blockfield-composer/composer.json /tmp/blockfield-composer/composer.lock /app/ \
    && composer install --no-interaction --prefer-dist \
    && php artisan filament:install --panels --no-interaction

# Overrides and finalization — this layer invalidates on source changes, but deps stay cached
COPY overrides/ ./
COPY entrypoint.sh /usr/local/bin/blockfield-filament

RUN chmod +x /usr/local/bin/blockfield-filament \
    && composer dump-autoload --optimize

ENV APP_ENV=production \
    APP_DEBUG=false \
    DB_CONNECTION=sqlite \
    DB_DATABASE=/data/database.sqlite \
    FILESYSTEM_DISK=local \
    SERVER_NAME=:8055 \
    SERVER_ROOT=public \
    PORT=8055

EXPOSE 8055

ENTRYPOINT ["blockfield-filament"]
