<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

class LauncherContent extends Model
{
    protected $table = 'launcher_content';

    protected $fillable = [
        'status',
        'brand',
        'brand_subtitle',
        'chrome_title',
        'operation_name',
        'season',
        'description',
        'server_name',
        'server_ip',
        'server_region',
        'operators',
        'ping',
        'region',
        'launcher_version',
        'coordinates',
        'copyright',
        'login_sector',
        'login_slogan',
        'operator_handle',
        'operator_initials',
        'operator_rank',
        'support_label',
        'network_status',
        'update_description',
        'settings_preferences',
        'translations',
        'features',
        'feed',
    ];

    protected function casts(): array
    {
        return [
            'translations' => 'array',
            'features' => 'array',
            'feed' => 'array',
        ];
    }
}
