<?php

use App\Filament\Resources\LauncherUserResource;
use App\Models\LauncherSession;
use App\Models\LauncherUser;
use App\Models\User;
use Illuminate\Foundation\Testing\RefreshDatabase;
use Illuminate\Support\Str;

uses(RefreshDatabase::class);

function managedLauncherUser(): LauncherUser
{
    return LauncherUser::create([
        'username' => 'managed_user',
        'email' => 'managed@example.com',
        'password' => 'strong-password-123',
        'minecraft_uuid' => (string) Str::uuid(),
        'status' => 'active',
        'role' => 'player',
    ]);
}

test('only administrators can access launcher user management', function (): void {
    $admin = User::factory()->create(['role' => 'admin']);
    $editor = User::factory()->create(['role' => 'editor']);

    $this->actingAs($admin);
    expect(LauncherUserResource::canViewAny())->toBeTrue();

    $this->actingAs($editor);
    expect(LauncherUserResource::canViewAny())->toBeFalse();
});

test('status actions revoke sessions and record the exact transition', function (): void {
    $admin = User::factory()->create(['role' => 'admin']);
    $user = managedLauncherUser();
    $this->actingAs($admin);
    LauncherSession::create([
        'launcher_user_id' => $user->id,
        'access_token_hash' => hash('sha256', 'access'),
        'refresh_token_hash' => hash('sha256', 'refresh'),
        'access_expires_at' => now()->addMinutes(15),
        'expires_at' => now()->addDay(),
    ]);

    LauncherUserResource::changeStatus($user, 'banned');

    expect($user->fresh()->status)->toBe('banned')
        ->and(LauncherSession::firstOrFail()->revoked_at)->not->toBeNull();
    $this->assertDatabaseHas('admin_audit_logs', [
        'actor_id' => $admin->id,
        'launcher_user_id' => $user->id,
        'action' => 'user_banned',
    ]);
});

test('session revocation action invalidates every active session', function (): void {
    $admin = User::factory()->create(['role' => 'admin']);
    $user = managedLauncherUser();
    $this->actingAs($admin);
    foreach (['one', 'two'] as $token) {
        LauncherSession::create([
            'launcher_user_id' => $user->id,
            'access_token_hash' => hash('sha256', "access-$token"),
            'refresh_token_hash' => hash('sha256', "refresh-$token"),
            'access_expires_at' => now()->addMinutes(15),
            'expires_at' => now()->addDay(),
        ]);
    }

    LauncherUserResource::revokeSessions($user);

    expect(LauncherSession::whereNull('revoked_at')->count())->toBe(0);
    $this->assertDatabaseHas('admin_audit_logs', [
        'launcher_user_id' => $user->id,
        'action' => 'sessions_revoked',
    ]);
});

test('Minecraft UUID is generated when an administrator leaves it empty', function (): void {
    $user = LauncherUser::create([
        'username' => 'generated_uuid',
        'email' => 'generated@example.com',
        'password' => '12345678',
        'status' => 'active',
        'role' => 'player',
    ]);

    expect($user->minecraft_uuid)->toMatch('/^[0-9a-f-]{36}$/');
});
