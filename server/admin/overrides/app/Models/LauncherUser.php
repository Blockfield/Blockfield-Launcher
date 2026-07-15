<?php

namespace App\Models;

use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Relations\HasMany;
use Illuminate\Database\Eloquent\Casts\Attribute;
use Illuminate\Support\Str;

class LauncherUser extends Model
{
    protected $fillable = ['username', 'email', 'password', 'minecraft_uuid', 'status', 'role', 'ban_reason', 'banned_until'];

    protected $hidden = ['password'];

    protected static function booted(): void
    {
        static::creating(function (LauncherUser $user): void {
            $user->minecraft_uuid ??= (string) Str::uuid();
        });
    }

    protected function casts(): array
    {
        return [
            'password' => 'hashed',
            'banned_until' => 'datetime',
            'last_login_at' => 'datetime',
        ];
    }

    public function sessions(): HasMany
    {
        return $this->hasMany(LauncherSession::class);
    }

    public function canAuthenticate(): bool
    {
        return $this->status === 'active' && (! $this->banned_until || $this->banned_until->isPast());
    }

    protected function username(): Attribute
    {
        return Attribute::make(set: fn (string $value): string => Str::lower(trim($value)));
    }

    protected function email(): Attribute
    {
        return Attribute::make(set: fn (?string $value): ?string => filled($value) ? Str::lower(trim($value)) : null);
    }
}
