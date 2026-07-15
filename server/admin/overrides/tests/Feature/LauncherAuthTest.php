<?php

use App\Models\LauncherSession;
use App\Models\LauncherUser;
use Illuminate\Foundation\Testing\RefreshDatabase;
use Illuminate\Support\Facades\Hash;
use Illuminate\Support\Str;

uses(RefreshDatabase::class);

beforeEach(fn () => config()->set('blockfield.auth_signing_key', str_repeat('k', 32)));

function launcherUser(array $attributes = []): LauncherUser
{
    return LauncherUser::create(array_merge([
        'username' => 'operator_1',
        'email' => 'operator@example.com',
        'password' => 'test-password-123',
        'minecraft_uuid' => (string) Str::uuid(),
        'status' => 'active',
        'role' => 'player',
    ], $attributes));
}

test('valid login stores only token hashes and returns the account identity', function (): void {
    $user = launcherUser();

    $response = $this->postJson('/api/launcher/v1/auth/login', [
        'username' => 'OPERATOR_1', 'password' => 'test-password-123', 'remember' => true,
    ])->assertOk()->assertJsonPath('user.minecraft_uuid', $user->minecraft_uuid);

    $session = LauncherSession::firstOrFail();
    expect($session->access_token_hash)->toBe(hash('sha256', $response['accessToken']))
        ->and($session->refresh_token_hash)->toBe(hash('sha256', $response['refreshToken']))
        ->and($session->access_token_hash)->not->toBe($response['accessToken']);
});

test('unknown invalid disabled and banned accounts receive the same error', function (): void {
    launcherUser();
    launcherUser(['username' => 'disabled', 'email' => 'disabled@example.com', 'minecraft_uuid' => (string) Str::uuid(), 'status' => 'disabled']);
    launcherUser(['username' => 'banned', 'email' => 'banned@example.com', 'minecraft_uuid' => (string) Str::uuid(), 'status' => 'banned']);

    foreach ([
        ['missing', 'test-password-123'], ['operator_1', 'wrong'],
        ['disabled', 'test-password-123'], ['banned', 'test-password-123'],
    ] as [$username, $password]) {
        $this->postJson('/api/launcher/v1/auth/login', compact('username', 'password'))
            ->assertUnauthorized()->assertExactJson(['message' => 'Invalid credentials.']);
    }
});

test('access expiry refresh rotation logout and status revocation are enforced', function (): void {
    $user = launcherUser();
    $login = $this->postJson('/api/launcher/v1/auth/login', [
        'username' => $user->username, 'password' => 'test-password-123',
    ])->json();

    $this->withToken($login['accessToken'])->getJson('/api/launcher/v1/auth/me')->assertOk();
    LauncherSession::first()->update(['access_expires_at' => now()->subSecond()]);
    $this->withToken($login['accessToken'])->getJson('/api/launcher/v1/auth/me')->assertUnauthorized();

    $refreshed = $this->postJson('/api/launcher/v1/auth/refresh', ['refreshToken' => $login['refreshToken']])
        ->assertOk()->json();
    $this->postJson('/api/launcher/v1/auth/refresh', ['refreshToken' => $login['refreshToken']])->assertUnauthorized();
    $this->withToken($refreshed['accessToken'])->postJson('/api/launcher/v1/auth/logout')->assertOk();
    $this->withToken($refreshed['accessToken'])->getJson('/api/launcher/v1/auth/me')->assertUnauthorized();

    $second = $this->postJson('/api/launcher/v1/auth/login', [
        'username' => $user->username, 'password' => 'test-password-123',
    ])->json();
    $user->update(['status' => 'banned']);
    $this->withToken($second['accessToken'])->getJson('/api/launcher/v1/auth/me')->assertUnauthorized();
});

test('passwords are hashed and usernames are unique', function (): void {
    $user = launcherUser();
    expect($user->password)->not->toBe('test-password-123')
        ->and(Hash::check('test-password-123', $user->password))->toBeTrue();

    $this->expectException(\Illuminate\Database\UniqueConstraintViolationException::class);
    launcherUser(['email' => 'other@example.com', 'minecraft_uuid' => (string) Str::uuid()]);
});

test('login is rate limited', function (): void {
    for ($attempt = 0; $attempt < 5; $attempt++) {
        $this->postJson('/api/launcher/v1/auth/login', [
            'username' => 'missing', 'password' => 'wrong-password',
        ])->assertUnauthorized();
    }
    $this->postJson('/api/launcher/v1/auth/login', [
        'username' => 'missing', 'password' => 'wrong-password',
    ])->assertTooManyRequests();
});
