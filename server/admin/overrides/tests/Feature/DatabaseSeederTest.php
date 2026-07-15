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

test('admin locale switcher is hidden outside localized content', function (): void {
    $user = User::factory()->create(['role' => 'admin']);

    $this->actingAs($user)
        ->get('/admin/modpack-releases')
        ->assertOk()
        ->assertDontSee('Content locale');
});

test('content api flattens localized fields for the launcher', function (): void {
    config()->set('blockfield.admin.email', 'admin@example.com');
    config()->set('blockfield.admin.password', '12345678');
    config()->set('blockfield.cms_token', str_repeat('a', 32));

    $this->seed(DatabaseSeeder::class);
    LauncherContent::firstOrFail()->update([
        'translations' => ['ru' => ['content' => ['operationName' => 'ЖЕЛЕЗНЫЙ ФРОНТ']]],
    ]);
    \App\Models\FeatureCard::orderBy('sort_order')->firstOrFail()->update([
        'translations' => ['ru' => ['title' => 'ТОЧКИ ЗАХВАТА']],
    ]);
    \App\Models\NewsFeedEntry::orderBy('sort_order')->firstOrFail()->update([
        'translations' => ['ru' => ['tag' => 'ПАТЧ', 'title' => 'Баланс техники']],
    ]);

    $this->withToken(str_repeat('a', 32))
        ->getJson('/api/items/launcher_content')
        ->assertOk()
        ->assertJsonFragment(['content.operationName' => 'ЖЕЛЕЗНЫЙ ФРОНТ'])
        ->assertJsonFragment(['feature.0.title' => 'ТОЧКИ ЗАХВАТА'])
        ->assertJsonFragment(['feed.0.title' => 'Баланс техники'])
        ->assertJsonMissing(['feed.0.tag' => 'ПАТЧ']);
});
