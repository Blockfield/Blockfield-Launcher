<?php

use App\Models\User;
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
