<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

class ModpackRelease extends Model
{
    protected $fillable = [
        'version',
        'minecraft_version',
        'modpack_zip',
        'zip_url',
        'prune',
        'java',
        'forge',
        'status',
        'active',
        'archive_size',
        'archive_sha256',
        'publish_error',
        'published_at',
    ];

    protected function casts(): array
    {
        return [
            'prune' => 'array',
            'java' => 'array',
            'forge' => 'array',
            'active' => 'boolean',
            'published_at' => 'datetime',
        ];
    }
}
