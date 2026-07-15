<?php

namespace App\Support;

class CmsLocale
{
    public const OPTIONS = [
        'en' => 'EN · English',
        'ru' => 'RU · Русский',
        'uk' => 'UK · Українська',
    ];

    public static function current(): string
    {
        $requested = request()->query('locale');
        if (is_string($requested) && isset(self::OPTIONS[$requested])) {
            session()->put('cms_locale', $requested);
        }

        $locale = session()->get('cms_locale', 'en');

        return is_string($locale) && isset(self::OPTIONS[$locale]) ? $locale : 'en';
    }

    public static function field(string $english, string $translated): string
    {
        $locale = self::current();

        return $locale === 'en' ? $english : "translations.{$locale}.{$translated}";
    }

    public static function preserveOtherLocales(array $data, ?array $existing): array
    {
        if (isset($data['translations'])) {
            $data['translations'] = array_replace_recursive($existing ?? [], $data['translations']);
        }

        return $data;
    }
}
