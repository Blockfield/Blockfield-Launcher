<?php

use App\Models\LauncherContent;
use App\Models\User;
use App\Support\CmsLocale;
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

    expect(LauncherContent::firstOrFail()->translations['ru']['nav']['deploy'])->toBe('БОЙ');
});

test('editing one locale preserves the others', function (): void {
    $data = CmsLocale::preserveOtherLocales(
        ['translations' => ['ru' => ['title' => 'Новый заголовок']]],
        ['ru' => ['desc' => 'Описание'], 'uk' => ['title' => 'Заголовок']],
    );

    expect($data['translations']['ru'])->toBe([
        'desc' => 'Описание',
        'title' => 'Новый заголовок',
    ])->and($data['translations']['uk']['title'])->toBe('Заголовок');
});

test('admin locale switcher is visible and persists the selected locale', function (): void {
    $user = User::factory()->create(['role' => 'admin']);

    $this->actingAs($user)
        ->get('/admin/feature-cards?locale=ru')
        ->assertOk()
        ->assertSee('Content locale')
        ->assertSee('RU · Русский')
        ->assertSessionHas('cms_locale', 'ru');
});

test('content api flattens localized fields for the launcher', function (): void {
    config()->set('blockfield.admin.email', 'admin@example.com');
    config()->set('blockfield.admin.password', '12345678');
    config()->set('blockfield.cms_token', str_repeat('a', 32));

    $this->seed(DatabaseSeeder::class);
    \App\Models\FeatureCard::orderBy('sort_order')->firstOrFail()->update([
        'translations' => ['ru' => ['title' => 'ТОЧКИ ЗАХВАТА']],
    ]);
    \App\Models\NewsFeedEntry::orderBy('sort_order')->firstOrFail()->update([
        'translations' => ['ru' => ['title' => 'Баланс техники']],
    ]);

    $this->withToken(str_repeat('a', 32))
        ->getJson('/api/items/launcher_content')
        ->assertOk()
        ->assertJsonFragment(['nav.deploy' => 'БОЙ'])
        ->assertJsonFragment(['feature.0.title' => 'ТОЧКИ ЗАХВАТА'])
        ->assertJsonFragment(['feed.0.title' => 'Баланс техники']);
});
