<?php

namespace App\Filament\Resources\NewsFeedEntryResource\Pages;

use App\Filament\Resources\NewsFeedEntryResource;
use App\Support\LauncherContentCache;
use Filament\Actions;
use Filament\Resources\Pages\ManageRecords;

class ManageNewsFeedEntries extends ManageRecords
{
    protected static string $resource = NewsFeedEntryResource::class;

    protected function getHeaderActions(): array
    {
        return [
            Actions\CreateAction::make()->after(fn () => LauncherContentCache::invalidate()),
        ];
    }
}
