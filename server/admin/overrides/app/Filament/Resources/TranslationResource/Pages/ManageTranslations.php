<?php

namespace App\Filament\Resources\TranslationResource\Pages;

use App\Filament\Resources\TranslationResource;
use App\Support\LauncherContentCache;
use Filament\Actions;
use Filament\Resources\Pages\ManageRecords;

class ManageTranslations extends ManageRecords
{
    protected static string $resource = TranslationResource::class;

    protected function getHeaderActions(): array
    {
        return [
            Actions\CreateAction::make()->after(fn () => LauncherContentCache::invalidate()),
        ];
    }
}
