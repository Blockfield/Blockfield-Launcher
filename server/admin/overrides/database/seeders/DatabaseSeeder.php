<?php

namespace Database\Seeders;

use App\Models\FeatureCard;
use App\Models\LauncherContent;
use App\Models\NewsFeedEntry;
use App\Models\User;
use App\Models\LauncherSession;
use Illuminate\Database\Seeder;
use Illuminate\Support\Facades\Hash;
use Illuminate\Support\Facades\Schema;
use Illuminate\Support\Facades\DB;

class DatabaseSeeder extends Seeder
{
    public function run(): void
    {
        LauncherSession::where('expires_at', '<', now()->subDays(7))->delete();
        DB::table('admin_audit_logs')->where('created_at', '<', now()->subDays(90))->delete();
        if (! User::exists()) {
            $email = (string) config('blockfield.admin.email', '');
            $password = (string) config('blockfield.admin.password', '');

            if (! filter_var($email, FILTER_VALIDATE_EMAIL)) {
                throw new \RuntimeException('FILAMENT_ADMIN_EMAIL is required for initial bootstrap.');
            }
            if (strlen($password) < 8 || strtolower($password) === 'admin') {
                throw new \RuntimeException('FILAMENT_ADMIN_PASSWORD must be at least 8 characters.');
            }

            User::create([
                'email' => $email,
                'name' => config('blockfield.admin.name', 'Blockfield Admin'),
                'password' => Hash::make($password),
            ]);
        }

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
                'copyright' => '2026 BLOCKFIELD COMMAND',
                'login_sector' => 'SECTOR 07 - NORTH RIDGE',
                'login_slogan' => 'DEPLOY. CAPTURE. DOMINATE.',
                'support_label' => 'SUPPORT',
                'update_description' => 'Synchronizing modpack assets with the primary deployment server. Do not close the launcher until the operation completes.',
                'settings_preferences' => '/ LAUNCHER PREFERENCES',
                'translations' => [
                    'ru' => ['nav' => ['deploy' => 'БОЙ']],
                    'uk' => ['nav' => ['deploy' => 'БІЙ']],
                ],
            ],
        );

        if (Schema::hasTable('feature_cards') && FeatureCard::count() === 0) {
            FeatureCard::insert([
                ['icon' => 'flag', 'title' => 'CAPTURE POINTS', 'desc' => 'Dynamic objective control across multiple sectors.', 'sort_order' => 0, 'created_at' => now(), 'updated_at' => now()],
                ['icon' => 'swords', 'title' => '6 CLASSES', 'desc' => 'Assault, Recon, Engineer, Medic, Support, Pilot.', 'sort_order' => 1, 'created_at' => now(), 'updated_at' => now()],
                ['icon' => 'truck', 'title' => 'ARMORED VEHICLES', 'desc' => 'Tanks, APCs, light recon and air transport.', 'sort_order' => 2, 'created_at' => now(), 'updated_at' => now()],
                ['icon' => 'crosshair', 'title' => 'TACTICAL BATTLES', 'desc' => 'Squad-based 64v64 persistent warfare.', 'sort_order' => 3, 'created_at' => now(), 'updated_at' => now()],
            ]);
        }

        if (Schema::hasTable('news_feed_entries') && NewsFeedEntry::count() === 0) {
            NewsFeedEntry::insert([
                ['tag' => 'PATCH', 'tone' => 'amber', 'date' => '06.07', 'title' => '0.1.43 - Vehicle Balance', 'body' => 'New modpack release is available.', 'sort_order' => 0, 'created_at' => now(), 'updated_at' => now()],
            ]);
        }

    }
}
