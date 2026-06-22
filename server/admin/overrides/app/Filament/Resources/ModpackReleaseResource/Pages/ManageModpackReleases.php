<?php

namespace App\Filament\Resources\ModpackReleaseResource\Pages;

use App\Filament\Resources\ModpackReleaseResource;
use Filament\Actions;
use Filament\Resources\Pages\ManageRecords;

class ManageModpackReleases extends ManageRecords
{
    protected static string $resource = ModpackReleaseResource::class;

    protected function getHeaderActions(): array
    {
        return [
            Actions\CreateAction::make(),
        ];
    }
}
