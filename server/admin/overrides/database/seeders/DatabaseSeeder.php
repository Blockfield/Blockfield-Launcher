<?php

namespace Database\Seeders;

use App\Models\FeatureCard;
use App\Models\LauncherContent;
use App\Models\NewsFeedEntry;
use App\Models\Translation;
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
            [],
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
            ],
        );

        if (FeatureCard::count() === 0) {
            FeatureCard::insert([
                ['icon' => 'flag', 'title' => 'CAPTURE POINTS', 'desc' => 'Dynamic objective control across multiple sectors.', 'sort_order' => 0, 'created_at' => now(), 'updated_at' => now()],
                ['icon' => 'swords', 'title' => '6 CLASSES', 'desc' => 'Assault, Recon, Engineer, Medic, Support, Pilot.', 'sort_order' => 1, 'created_at' => now(), 'updated_at' => now()],
                ['icon' => 'truck', 'title' => 'ARMORED VEHICLES', 'desc' => 'Tanks, APCs, light recon and air transport.', 'sort_order' => 2, 'created_at' => now(), 'updated_at' => now()],
                ['icon' => 'crosshair', 'title' => 'TACTICAL BATTLES', 'desc' => 'Squad-based 64v64 persistent warfare.', 'sort_order' => 3, 'created_at' => now(), 'updated_at' => now()],
            ]);
        }

        if (NewsFeedEntry::count() === 0) {
            NewsFeedEntry::insert([
                ['tag' => 'PATCH', 'tone' => 'amber', 'date' => '06.07', 'title' => '0.1.43 - Vehicle Balance', 'body' => 'New modpack release is available.', 'sort_order' => 0, 'created_at' => now(), 'updated_at' => now()],
            ]);
        }

        if (Translation::count() === 0) {
            Translation::insert([
                // ru
                ['locale' => 'ru', 'key' => 'nav.deploy', 'value' => 'БОЙ', 'created_at' => now(), 'updated_at' => now()],
                // uk
                ['locale' => 'uk', 'key' => 'nav.deploy', 'value' => 'БІЙ', 'created_at' => now(), 'updated_at' => now()],
            ]);
        }
    }
}
