<?php

namespace Database\Seeders;

use App\Models\LauncherContent;
use App\Models\LauncherUpdate;
use App\Models\User;
use Illuminate\Database\Seeder;
use Illuminate\Support\Facades\Hash;

class DatabaseSeeder extends Seeder
{
    public function run(): void
    {
        User::updateOrCreate(
            ['email' => env('FILAMENT_ADMIN_EMAIL', 'admin@example.com')],
            [
                'name' => env('FILAMENT_ADMIN_NAME', 'Blockfield Admin'),
                'password' => Hash::make(env('FILAMENT_ADMIN_PASSWORD', 'admin')),
            ],
        );

        LauncherContent::firstOrCreate(
            ['status' => 'published'],
            [
                'brand' => 'BLOCKFIELD',
                'brand_subtitle' => 'TACTICAL OPS',
                'chrome_title' => 'BLOCKFIELD LAUNCHER',
                'operation_name' => 'IRON FRONT',
                'season' => '/ SEASON 01',
                'description' => 'Large-scale tactical PvP across contested terrain.',
                'server_name' => 'BLOCKFIELD - PRIMARY',
                'server_ip' => 'play.blockfield.gg:25565',
                'server_region' => 'EU-WEST - 28ms',
                'operators' => '142',
                'ping' => '28',
                'region' => 'EU-W',
                'launcher_version' => '0.4.2',
                'coordinates' => 'LAT 47.3829 / LON 19.0402',
                'copyright' => '2026 BLOCKFIELD COMMAND',
                'login_sector' => 'SECTOR 07 - NORTH RIDGE',
                'login_slogan' => 'DEPLOY. CAPTURE. DOMINATE.',
                'operator_handle' => 'KILO_7',
                'operator_initials' => 'K7',
                'operator_rank' => 'RANK - SERGEANT',
                'support_label' => 'SUPPORT',
                'network_status' => 'NETWORK NOMINAL',
                'update_description' => 'Synchronizing modpack assets with the primary deployment server. Do not close the launcher until the operation completes.',
                'settings_preferences' => '/ LAUNCHER PREFERENCES',
                'translations' => ['en' => [], 'ru' => [], 'uk' => []],
                'features' => [
                    ['icon' => 'flag', 'title' => 'CAPTURE POINTS', 'desc' => 'Dynamic objective control across multiple sectors.'],
                    ['icon' => 'swords', 'title' => '6 CLASSES', 'desc' => 'Assault, Recon, Engineer, Medic, Support, Pilot.'],
                    ['icon' => 'truck', 'title' => 'ARMORED VEHICLES', 'desc' => 'Tanks, APCs, light recon and air transport.'],
                    ['icon' => 'crosshair', 'title' => 'TACTICAL BATTLES', 'desc' => 'Squad-based 64v64 persistent warfare.'],
                ],
                'feed' => [
                    ['tag' => 'PATCH', 'tone' => 'amber', 'date' => '06.07', 'title' => '0.1.43 - Vehicle Balance', 'body' => 'New modpack release is available.'],
                ],
            ],
        );

        LauncherUpdate::firstOrCreate(
            ['status' => 'published'],
            [
                'version' => '0.1.0',
                'notes' => 'No launcher update available.',
                'pub_date' => '2026-06-18T00:00:00Z',
                'windows_url' => 'https://play.blockfield.gg/downloads/blockfield-launcher_0.1.0_x64-setup.exe',
                'windows_signature' => '',
            ],
        );
    }
}
