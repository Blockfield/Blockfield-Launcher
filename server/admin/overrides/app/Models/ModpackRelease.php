<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

class ModpackRelease extends Model
{
    protected $fillable = [
        'version',
        'minecraft_version',
        'modpack_zip',
        'prune',
        'java',
        'status',
        'active',
        'archive_size',
        'archive_sha256',
        'publish_error',
        'published_at',
    ];

    protected $hidden = ['zip_url'];

    protected function casts(): array
    {
        return [
            'prune' => 'array',
            'java' => 'array',
            'active' => 'boolean',
            'published_at' => 'datetime',
        ];
    }
}
