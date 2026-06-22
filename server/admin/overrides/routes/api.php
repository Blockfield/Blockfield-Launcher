<?php

use App\Http\Controllers\CmsApiController;
use App\Http\Middleware\CmsToken;
use Illuminate\Support\Facades\Route;

Route::middleware(CmsToken::class)->group(function (): void {
    Route::get('/items/{collection}', [CmsApiController::class, 'items']);
    Route::post('/items/{collection}', [CmsApiController::class, 'storeItem']);
    Route::post('/files', [CmsApiController::class, 'storeFile']);
    Route::get('/assets/{path}', [CmsApiController::class, 'asset'])->where('path', '.*');
});
