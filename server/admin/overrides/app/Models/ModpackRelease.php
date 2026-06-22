<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

class ModpackRelease extends Model
{
    protected $fillable = [
        'status',
        'version',
        'minecraft_version',
        'modpack_zip',
        'zip_url',
        'prune',
        'java',
        'forge',
    ];

    protected function casts(): array
    {
        return [
            'prune' => 'array',
            'java' => 'array',
            'forge' => 'array',
        ];
    }
}
