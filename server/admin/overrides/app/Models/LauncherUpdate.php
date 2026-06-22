<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

class LauncherUpdate extends Model
{
    protected $fillable = [
        'status',
        'version',
        'notes',
        'pub_date',
        'windows_url',
        'windows_signature',
        'platforms',
    ];

    protected function casts(): array
    {
        return [
            'platforms' => 'array',
        ];
    }
}
