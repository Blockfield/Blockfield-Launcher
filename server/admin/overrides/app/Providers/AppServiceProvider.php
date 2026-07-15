<?php

namespace App\Providers;

use App\Support\CmsLocale;
use Filament\Support\Facades\FilamentView;
use Filament\View\PanelsRenderHook;
use Illuminate\Contracts\View\View;
use Illuminate\Support\ServiceProvider;

class AppServiceProvider extends ServiceProvider
{
    public function register(): void
    {
        //
    }

    public function boot(): void
    {
        FilamentView::registerRenderHook(
            PanelsRenderHook::USER_MENU_BEFORE,
            fn (): View => view('filament.locale-switcher', [
                'locale' => CmsLocale::current(),
                'locales' => CmsLocale::OPTIONS,
            ]),
        );
    }
}
