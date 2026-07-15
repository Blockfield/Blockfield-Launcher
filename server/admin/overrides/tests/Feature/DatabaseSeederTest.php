<?php

use App\Models\User;
use App\Models\LauncherContent;
use Database\Seeders\DatabaseSeeder;
use Illuminate\Foundation\Testing\RefreshDatabase;
use Illuminate\Support\Facades\Hash;

uses(RefreshDatabase::class);

test('initial admin requires explicit strong credentials', function (): void {
    config()->set('blockfield.admin.email', 'admin@example.com');
    config()->set('blockfield.admin.password', 'admin');

    $this->expectException(RuntimeException::class);
    $this->seed(DatabaseSeeder::class);
});

test('seeding does not replace an existing admin password', function (): void {
    $user = User::factory()->create(['password' => Hash::make('existing-password')]);
    config()->set('blockfield.admin.email', 'new@example.com');
    config()->set('blockfield.admin.password', 'new-password-123');

    $this->seed(DatabaseSeeder::class);

    expect(Hash::check('existing-password', $user->fresh()->password))->toBeTrue();
    expect(User::count())->toBe(1);
});

test('launcher translations live on launcher content instead of a separate resource', function (): void {
    config()->set('blockfield.admin.email', 'admin@example.com');
    config()->set('blockfield.admin.password', '12345678');

    $this->seed(DatabaseSeeder::class);

    expect(LauncherContent::firstOrFail()->translations['ru']['nav.deploy'])->toBe('БОЙ');
});
