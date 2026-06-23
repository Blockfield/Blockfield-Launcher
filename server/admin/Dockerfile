FROM dunglas/frankenphp:1-php8.3-bookworm

RUN apt-get update && apt-get install -y git unzip zip \
    && install-php-extensions intl pdo_sqlite zip opcache \
    && rm -rf /var/lib/apt/lists/*

RUN echo "upload_max_filesize=2G\npost_max_size=2G\nmax_execution_time=300\nmax_input_time=300" \
    > /usr/local/etc/php/conf.d/uploads.ini

COPY --from=composer:2 /usr/bin/composer /usr/bin/composer

WORKDIR /app

RUN rm -rf /app/* /app/.[!.]* /app/..?*

RUN composer create-project laravel/laravel:"^12.0" . --no-interaction --prefer-dist \
    && composer require filament/filament:"^5.0" --no-interaction \
    && php artisan filament:install --panels --no-interaction

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
