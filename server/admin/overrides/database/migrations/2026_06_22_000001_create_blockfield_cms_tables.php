<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('modpack_releases', function (Blueprint $table): void {
            $table->id();
            $table->string('status')->default('draft')->index();
            $table->string('version');
            $table->string('minecraft_version')->default('1.20.1');
            $table->string('modpack_zip')->nullable();
            $table->string('zip_url')->nullable();
            $table->json('prune')->nullable();
            $table->json('java')->nullable();
            $table->json('forge')->nullable();
            $table->timestamps();
        });

        Schema::create('launcher_updates', function (Blueprint $table): void {
            $table->id();
            $table->string('status')->default('draft')->index();
            $table->string('version');
            $table->text('notes')->nullable();
            $table->string('pub_date')->nullable();
            $table->string('windows_url')->nullable();
            $table->text('windows_signature')->nullable();
            $table->json('platforms')->nullable();
            $table->timestamps();
        });

        Schema::create('launcher_content', function (Blueprint $table): void {
            $table->id();
            $table->string('status')->default('draft')->index();
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
            $table->json('translations')->nullable();
            $table->json('features')->nullable();
            $table->json('feed')->nullable();
            $table->timestamps();
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('launcher_content');
        Schema::dropIfExists('launcher_updates');
        Schema::dropIfExists('modpack_releases');
    }
};
