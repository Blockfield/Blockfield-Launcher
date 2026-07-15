<?php

namespace App\Filament\Resources\LauncherUserResource\Pages;

use App\Filament\Resources\LauncherUserResource;
use Filament\Actions;
use Filament\Resources\Pages\ManageRecords;

class ManageLauncherUsers extends ManageRecords
{
    protected static string $resource = LauncherUserResource::class;

    protected function getHeaderActions(): array
    {
        return [Actions\CreateAction::make()];
    }
}
