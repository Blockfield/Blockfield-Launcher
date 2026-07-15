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
        'copyright',
        'login_sector',
        'login_slogan',
        'support_label',
        'update_description',
        'settings_preferences',
    ];
}
