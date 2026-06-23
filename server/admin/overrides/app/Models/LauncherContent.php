<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

class LauncherContent extends Model
{
    protected $table = 'launcher_content';

    protected $fillable = [
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
    ];
}
