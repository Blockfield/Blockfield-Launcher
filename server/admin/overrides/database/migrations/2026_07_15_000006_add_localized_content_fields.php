<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Arr;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::table('feature_cards', function (Blueprint $table): void {
            $table->json('translations')->nullable();
        });
        Schema::table('news_feed_entries', function (Blueprint $table): void {
            $table->json('translations')->nullable();
        });

        $this->rewriteLauncherTranslations(fn (array $messages): array => Arr::undot($messages));
    }

    public function down(): void
    {
        $this->rewriteLauncherTranslations(fn (array $messages): array => Arr::dot($messages));

        Schema::table('feature_cards', function (Blueprint $table): void {
            $table->dropColumn('translations');
        });
        Schema::table('news_feed_entries', function (Blueprint $table): void {
            $table->dropColumn('translations');
        });
    }

    private function rewriteLauncherTranslations(Closure $rewrite): void
    {
        foreach (DB::table('launcher_content')->select(['id', 'translations'])->get() as $record) {
            $translations = json_decode($record->translations ?? '[]', true, flags: JSON_THROW_ON_ERROR);
            if (! is_array($translations)) {
                continue;
            }
            foreach ($translations as $locale => $messages) {
                if (is_array($messages)) {
                    $translations[$locale] = $rewrite($messages);
                }
            }
            DB::table('launcher_content')->where('id', $record->id)->update([
                'translations' => json_encode($translations, JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR),
            ]);
        }
    }
};
