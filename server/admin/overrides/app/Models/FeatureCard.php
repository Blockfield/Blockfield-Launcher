<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

class FeatureCard extends Model
{
    protected $fillable = ['icon', 'title', 'desc', 'sort_order', 'translations'];

    protected function casts(): array
    {
        return ['translations' => 'array'];
    }
}
