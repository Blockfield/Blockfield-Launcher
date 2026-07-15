<?php

use App\Http\Middleware\CmsToken;
use Illuminate\Support\Facades\Route;

beforeEach(function (): void {
    Route::middleware(CmsToken::class)->get('/_cms-token-test', fn () => 'ok');
});

test('cms endpoints fail closed without a strong token', function (): void {
    config()->set('blockfield.cms_token', '');

    $this->get('/_cms-token-test')->assertServiceUnavailable();
});

test('cms endpoints require the configured bearer token', function (): void {
    config()->set('blockfield.cms_token', str_repeat('a', 32));

    $this->get('/_cms-token-test')->assertUnauthorized();
    $this->withToken(str_repeat('b', 32))->get('/_cms-token-test')->assertUnauthorized();
    $this->withToken(str_repeat('a', 32))->get('/_cms-token-test')->assertOk();
});
