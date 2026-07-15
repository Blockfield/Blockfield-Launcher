<?php

namespace App\Filament\Resources\FeatureCardResource\Pages;

use App\Filament\Resources\FeatureCardResource;
use App\Support\LauncherContentCache;
use Filament\Actions;
use Filament\Resources\Pages\ManageRecords;

class ManageFeatureCards extends ManageRecords
{
    protected static string $resource = FeatureCardResource::class;

    protected function getHeaderActions(): array
    {
        return [
            Actions\CreateAction::make()->after(fn () => LauncherContentCache::invalidate()),
        ];
    }
}
