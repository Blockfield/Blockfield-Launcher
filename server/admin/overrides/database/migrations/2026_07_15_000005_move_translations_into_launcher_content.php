<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::table('launcher_content', function (Blueprint $table): void {
            $table->json('translations')->nullable();
        });

        if (! Schema::hasTable('translations')) {
            return;
        }

        $translations = DB::table('translations')->get()
            ->groupBy('locale')
            ->map(fn ($group) => $group->pluck('value', 'key'))
            ->toArray();
        if ($translations !== []) {
            DB::table('launcher_content')->update([
                'translations' => json_encode($translations, JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR),
            ]);
        }
    }

    public function down(): void
    {
        Schema::table('launcher_content', function (Blueprint $table): void {
            $table->dropColumn('translations');
        });
    }
};
