<?php

namespace App\Providers;

use App\Filament\Resources\FeatureCardResource\Pages\ManageFeatureCards;
use App\Filament\Resources\LauncherContentResource\Pages\ManageLauncherContent;
use App\Filament\Resources\NewsFeedEntryResource\Pages\ManageNewsFeedEntries;
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
            PanelsRenderHook::PAGE_HEADER_ACTIONS_BEFORE,
            fn (): View => view('filament.locale-switcher', [
                'locale' => CmsLocale::current(),
                'locales' => CmsLocale::OPTIONS,
            ]),
            scopes: [
                ManageLauncherContent::class,
                ManageFeatureCards::class,
                ManageNewsFeedEntries::class,
            ],
        );
    }
}
