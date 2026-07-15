<?php

use App\Http\Controllers\CmsApiController;
use App\Http\Controllers\LauncherAuthController;
use App\Http\Middleware\CmsToken;
use Illuminate\Support\Facades\Route;

Route::prefix('launcher/v1/auth')->group(function (): void {
    Route::post('/register', [LauncherAuthController::class, 'register'])->middleware('throttle:3,1');
    Route::post('/login', [LauncherAuthController::class, 'login'])->middleware('throttle:5,1');
    Route::post('/refresh', [LauncherAuthController::class, 'refresh'])->middleware('throttle:20,1');
    Route::post('/logout', [LauncherAuthController::class, 'logout']);
    Route::get('/me', [LauncherAuthController::class, 'me']);
});

Route::middleware(CmsToken::class)->group(function (): void {
    Route::get('/items/{collection}', [CmsApiController::class, 'items']);
    Route::post('/items/{collection}', [CmsApiController::class, 'storeItem']);
    Route::post('/items/modpack_releases/{release}/activate', [CmsApiController::class, 'activateRelease']);
    Route::post('/files', [CmsApiController::class, 'storeFile']);
    Route::get('/assets/{path}', [CmsApiController::class, 'asset'])->where('path', '.*');
});
