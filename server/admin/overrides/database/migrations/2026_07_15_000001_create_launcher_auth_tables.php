<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('launcher_users', function (Blueprint $table): void {
            $table->id();
            $table->string('username')->unique();
            $table->string('email')->nullable()->unique();
            $table->string('password');
            $table->uuid('minecraft_uuid')->unique();
            $table->string('status')->default('active')->index();
            $table->string('role')->default('player')->index();
            $table->text('ban_reason')->nullable();
            $table->timestamp('banned_until')->nullable();
            $table->timestamp('last_login_at')->nullable();
            $table->string('last_login_ip', 45)->nullable();
            $table->timestamps();
        });

        Schema::create('launcher_sessions', function (Blueprint $table): void {
            $table->id();
            $table->foreignId('launcher_user_id')->constrained()->cascadeOnDelete();
            $table->char('access_token_hash', 64)->unique();
            $table->char('refresh_token_hash', 64)->unique();
            $table->uuid('family_id')->index();
            $table->foreignId('parent_session_id')->nullable()->constrained('launcher_sessions')->nullOnDelete();
            $table->string('device_name')->nullable();
            $table->boolean('remember')->default(false);
            $table->timestamp('access_expires_at')->index();
            $table->timestamp('expires_at')->index();
            $table->timestamp('revoked_at')->nullable()->index();
            $table->timestamp('rotated_at')->nullable();
            $table->timestamp('last_used_at')->nullable();
            $table->timestamps();
        });

        Schema::create('admin_audit_logs', function (Blueprint $table): void {
            $table->id();
            $table->foreignId('actor_id')->nullable()->constrained('users')->nullOnDelete();
            $table->foreignId('launcher_user_id')->nullable()->constrained()->nullOnDelete();
            $table->string('action')->index();
            $table->json('metadata')->nullable();
            $table->timestamps();
        });
    }

    public function down(): void
    {
        Schema::dropIfExists('admin_audit_logs');
        Schema::dropIfExists('launcher_sessions');
        Schema::dropIfExists('launcher_users');
    }
};
