<?php

namespace App\Filament\Resources\LauncherContentResource\Pages;

use App\Filament\Resources\LauncherContentResource;
use App\Models\LauncherContent;
use Filament\Resources\Pages\EditRecord;

class ManageLauncherContent extends EditRecord
{
    protected static string $resource = LauncherContentResource::class;

    // ponytail: singleton — always resolve the one content record, ignoring the URL param
    public function mount($record = ''): void
    {
        $this->record = LauncherContent::first() ?? LauncherContent::create([]);
        $this->fillForm();
    }

    protected function getHeaderActions(): array
    {
        return [];
    }
}
