<?php

use App\Models\ModpackRelease;
use Illuminate\Foundation\Testing\RefreshDatabase;
use Illuminate\Support\Facades\Storage;

uses(RefreshDatabase::class);

beforeEach(function (): void {
    config()->set('blockfield.cms_token', str_repeat('c', 32));
    Storage::fake('local');
    Storage::disk('local')->put('modpacks/new.zip', 'test archive');
});

function releaseRecord(array $attributes = []): ModpackRelease
{
    return ModpackRelease::create(array_merge([
        'version' => '1.0.0',
        'minecraft_version' => '1.20.1',
        'modpack_zip' => 'modpacks/release.zip',
        'java' => [
            'version' => '17.0.16+8',
            'platform' => 'windows-x64',
            'url' => 'https://example.com/java-17.0.16.zip',
            'sha256' => str_repeat('a', 64),
            'size' => 123,
        ],
        'status' => 'ready',
        'active' => false,
    ], $attributes));
}

function cmsHeaders(): array
{
    return ['Authorization' => 'Bearer '.str_repeat('c', 32)];
}

test('draft creation never changes the active release', function (): void {
    $active = releaseRecord(['active' => true, 'status' => 'published']);

    $this->withHeaders(cmsHeaders())->postJson('/api/items/modpack_releases', [
        'version' => '1.1.0',
        'minecraft_version' => '1.20.1',
        'modpack_zip' => 'modpacks/new.zip',
    ])->assertCreated();

    expect($active->fresh()->active)->toBeTrue()
        ->and(ModpackRelease::latest('id')->first()->status)->toBe('draft');
});

test('release input requires exactly one archive source', function (): void {
    $base = ['version' => '1.1.0', 'minecraft_version' => '1.20.1'];
    $this->withHeaders(cmsHeaders())->postJson('/api/items/modpack_releases', $base)
        ->assertUnprocessable();
    $this->withHeaders(cmsHeaders())->postJson('/api/items/modpack_releases', $base + [
        'modpack_zip' => 'modpacks/new.zip',
        'zip_url' => 'https://example.com/new.zip',
    ])->assertUnprocessable();
});

test('activation and rollback keep exactly one active release and write audit history', function (): void {
    $old = releaseRecord(['active' => true, 'status' => 'published']);
    $new = releaseRecord(['version' => '1.1.0']);

    $this->withHeaders(cmsHeaders())
        ->postJson("/api/items/modpack_releases/{$new->id}/activate")
        ->assertOk()->assertJsonPath('data.id', $new->id);
    expect($old->fresh()->active)->toBeFalse()
        ->and($new->fresh()->active)->toBeTrue()
        ->and(ModpackRelease::where('active', true)->count())->toBe(1);
    $this->assertDatabaseHas('admin_audit_logs', ['action' => 'release_published_cli']);

    $this->withHeaders(cmsHeaders())
        ->postJson("/api/items/modpack_releases/{$old->id}/activate")
        ->assertOk();
    expect($old->fresh()->active)->toBeTrue()
        ->and($new->fresh()->active)->toBeFalse()
        ->and(ModpackRelease::where('active', true)->count())->toBe(1);
});
