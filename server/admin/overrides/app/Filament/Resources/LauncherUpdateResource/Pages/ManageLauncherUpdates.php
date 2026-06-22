<?php

namespace App\Filament\Resources\LauncherUpdateResource\Pages;

use App\Filament\Resources\LauncherUpdateResource;
use Filament\Actions;
use Filament\Resources\Pages\ManageRecords;

class ManageLauncherUpdates extends ManageRecords
{
    protected static string $resource = LauncherUpdateResource::class;

    protected function getHeaderActions(): array
    {
        return [
            Actions\CreateAction::make(),
        ];
    }
}
