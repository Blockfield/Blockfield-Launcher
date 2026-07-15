<?php

namespace App\Support;

use Illuminate\Support\Facades\Http;

class LauncherContentCache
{
    public static function invalidate(): void
    {
        Http::withToken((string) config('blockfield.reload_token'))
            ->timeout(10)
            ->post(config('blockfield.backend_url').'/content/reload')
            ->throw();
    }
}
