<?php

namespace App\Filament\Resources\FeatureCardResource\Pages;

use App\Filament\Resources\FeatureCardResource;
use App\Support\CmsLocale;
use App\Support\LauncherContentCache;
use Filament\Actions;
use Filament\Resources\Pages\ManageRecords;

class ManageFeatureCards extends ManageRecords
{
    protected static string $resource = FeatureCardResource::class;

    protected function getHeaderActions(): array
    {
        return [
            Actions\CreateAction::make()
                ->visible(CmsLocale::current() === 'en')
                ->after(fn () => LauncherContentCache::invalidate()),
        ];
    }
}
