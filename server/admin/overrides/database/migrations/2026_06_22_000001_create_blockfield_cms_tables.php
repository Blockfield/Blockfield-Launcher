<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        // ── Modpack releases ──────────────────────────────────────

        Schema::create('modpack_releases', function (Blueprint $table): void {
            $table->id();
            $table->string('version');
            $table->string('minecraft_version')->default('1.20.1');
            $table->string('modpack_zip')->nullable();
            $table->string('zip_url')->nullable();
            $table->json('prune')->nullable();
            $table->json('java')->nullable();
            $table->json('forge')->nullable();
            $table->timestamps();
        });

        // ── Launcher content (singleton) ──────────────────────────

        Schema::create('launcher_content', function (Blueprint $table): void {
            $table->id();
            $table->string('brand')->nullable();
            $table->string('brand_subtitle')->nullable();
            $table->string('chrome_title')->nullable();
            $table->string('operation_name')->nullable();
            $table->string('season')->nullable();
            $table->text('description')->nullable();
            $table->string('server_name')->nullable();
            $table->string('server_ip')->nullable();
            $table->string('server_region')->nullable();
            $table->string('operators')->nullable();
            $table->string('ping')->nullable();
            $table->string('region')->nullable();
            $table->string('launcher_version')->nullable();
            $table->string('coordinates')->nullable();
            $table->string('copyright')->nullable();
            $table->string('login_sector')->nullable();
            $table->string('login_slogan')->nullable();
            $table->string('operator_handle')->nullable();
            $table->string('operator_initials')->nullable();
            $table->string('operator_rank')->nullable();
            $table->string('support_label')->nullable();
            $table->string('network_status')->nullable();
            $table->text('update_description')->nullable();
            $table->string('settings_preferences')->nullable();
            $table->timestamps();
        });

        // ── Feature cards ─────────────────────────────────────────

        Schema::create('feature_cards', function (Blueprint $table): void {
            $table->id();
            $table->string('icon')->nullable();
            $table->string('title')->nullable();
            $table->text('desc')->nullable();
            $table->unsignedInteger('sort_order')->default(0);
            $table->timestamps();
        });

        // ── News feed entries ─────────────────────────────────────

        Schema::create('news_feed_entries', function (Blueprint $table): void {
            $table->id();
            $table->string('tag')->nullable();
            $table->string('tone')->nullable();
            $table->string('date')->nullable();
            $table->string('title')->nullable();
            $table->text('body')->nullable();
            $table->unsignedInteger('sort_order')->default(0);
            $table->timestamps();
        });

        // ── Translations ──────────────────────────────────────────

        Schema::create('translations', function (Blueprint $table): void {
            $table->id();
            $table->string('locale')->index();
            $table->string('key');
            $table->text('value')->nullable();
            $table->timestamps();
            $table->unique(['locale', 'key']);
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('translations');
        Schema::dropIfExists('news_feed_entries');
        Schema::dropIfExists('feature_cards');
        Schema::dropIfExists('launcher_content');
        Schema::dropIfExists('modpack_releases');
    }
};
