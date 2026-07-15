<?php

namespace App\Filament\Widgets;

use App\Models\ModpackRelease;
use Filament\Widgets\StatsOverviewWidget;
use Filament\Widgets\StatsOverviewWidget\Stat;
use Illuminate\Support\Facades\Http;

class DeploymentOverview extends StatsOverviewWidget
{
    protected function getStats(): array
    {
        $release = ModpackRelease::where('active', true)->first();
        $status = null;
        try {
            $response = Http::timeout(4)->get(config('blockfield.backend_url').'/server-status');
            if ($response->successful()) {
                $status = $response->json();
            }
        } catch (\Throwable) {
            // The dashboard must remain usable while the game/API server is unavailable.
        }

        $online = $status['online'] ?? null;
        $players = $online === true
            ? ($status['playersOnline'] ?? '—').'/'.($status['playersMax'] ?? '—')
            : '—';

        return [
            Stat::make('Active release', $release?->version ?? 'None')
                ->description($release?->published_at?->format('Y-m-d H:i') ?? 'No published release'),
            Stat::make('Minecraft server', $online === true ? 'ONLINE' : ($online === false ? 'OFFLINE' : 'UNAVAILABLE'))
                ->description('Players '.$players)
                ->color($online === true ? 'success' : ($online === false ? 'danger' : 'gray')),
        ];
    }
}
