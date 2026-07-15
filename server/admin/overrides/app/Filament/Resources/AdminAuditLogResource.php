<?php

namespace App\Filament\Resources;

use App\Filament\Resources\AdminAuditLogResource\Pages;
use App\Models\AdminAuditLog;
use Filament\Resources\Resource;
use Filament\Schemas\Schema;
use Filament\Tables;
use Filament\Tables\Table;
use Illuminate\Support\Str;

class AdminAuditLogResource extends Resource
{
    protected static ?string $model = AdminAuditLog::class;
    protected static string|\UnitEnum|null $navigationGroup = 'System';

    public static function canViewAny(): bool
    {
        return auth()->user()?->role === 'admin';
    }

    public static function canCreate(): bool
    {
        return false;
    }

    public static function canEdit($record): bool
    {
        return false;
    }

    public static function canDelete($record): bool
    {
        return false;
    }

    public static function form(Schema $schema): Schema
    {
        return $schema->components([]);
    }

    public static function table(Table $table): Table
    {
        return $table->columns([
            Tables\Columns\TextColumn::make('created_at')->dateTime()->sortable(),
            Tables\Columns\TextColumn::make('action')->searchable()->badge(),
            Tables\Columns\TextColumn::make('actor_id')->label('Admin'),
            Tables\Columns\TextColumn::make('launcher_user_id')->label('Launcher user'),
            Tables\Columns\TextColumn::make('metadata')->label('Details')
                ->formatStateUsing(fn ($state): string => collect($state ?? [])->map(
                    fn ($value, string $key): string => Str::headline($key).': '.(is_scalar($value) ? (string) $value : '—')
                )->join(' · '))->wrap(),
        ])->defaultSort('id', 'desc');
    }

    public static function getPages(): array
    {
        return ['index' => Pages\ListAdminAuditLogs::route('/')];
    }
}
