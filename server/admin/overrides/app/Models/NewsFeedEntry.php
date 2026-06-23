<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;

class NewsFeedEntry extends Model
{
    protected $fillable = ['tag', 'tone', 'date', 'title', 'body', 'sort_order'];
}
