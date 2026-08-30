<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\BelongsTo;

class LauncherSession extends Model
{
    protected $guarded = [];

    protected function casts(): array
    {
        return [
            'access_expires_at' => 'datetime',
            'expires_at' => 'datetime',
            'revoked_at' => 'datetime',
            'rotated_at' => 'datetime',
            'last_used_at' => 'datetime',
            'remember' => 'boolean',
        ];
    }

    public function user(): BelongsTo
    {
        return $this->belongsTo(LauncherUser::class, 'launcher_user_id');
    }
}
