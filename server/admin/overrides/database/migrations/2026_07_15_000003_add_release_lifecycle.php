<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::table('modpack_releases', function (Blueprint $table): void {
            $table->string('status')->default('draft')->index();
            $table->boolean('active')->default(false)->index();
            $table->unsignedBigInteger('archive_size')->nullable();
            $table->char('archive_sha256', 64)->nullable();
            $table->text('publish_error')->nullable();
            $table->timestamp('published_at')->nullable();
        });

        $latest = DB::table('modpack_releases')->orderByDesc('id')->first();
        if ($latest) {
            DB::table('modpack_releases')->where('id', $latest->id)->update([
                'status' => 'published', 'active' => true, 'published_at' => now(),
            ]);
        }
        DB::statement('CREATE UNIQUE INDEX modpack_releases_one_active ON modpack_releases(active) WHERE active = 1');
    }

    public function down(): void
    {
        DB::statement('DROP INDEX IF EXISTS modpack_releases_one_active');
        Schema::table('modpack_releases', fn (Blueprint $table) => $table->dropColumn([
            'status', 'active', 'archive_size', 'archive_sha256', 'publish_error', 'published_at',
        ]));
    }
};
