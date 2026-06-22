<?php

namespace App\Filament\Resources\LauncherContentResource\Pages;

use App\Filament\Resources\LauncherContentResource;
use Filament\Actions;
use Filament\Resources\Pages\ManageRecords;

class ManageLauncherContent extends ManageRecords
{
    protected static string $resource = LauncherContentResource::class;

    protected function getHeaderActions(): array
    {
        return [
            Actions\CreateAction::make(),
        ];
    }
}
