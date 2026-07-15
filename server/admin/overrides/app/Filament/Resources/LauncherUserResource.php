<?php

namespace App\Filament\Resources;

use App\Filament\Resources\LauncherUserResource\Pages;
use App\Models\LauncherUser;
use Filament\Actions;
use Filament\Forms;
use Filament\Resources\Resource;
use Filament\Schemas\Schema;
use Filament\Tables;
use Filament\Tables\Table;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Str;

class LauncherUserResource extends Resource
{
    protected static ?string $model = LauncherUser::class;
    protected static ?string $navigationLabel = 'Launcher Users';
    protected static string|\UnitEnum|null $navigationGroup = 'Access';

    public static function canViewAny(): bool
    {
        return auth()->user()?->role === 'admin';
    }

    public static function form(Schema $schema): Schema
    {
        return $schema->components([
            Forms\Components\TextInput::make('username')
                ->required()->minLength(3)->maxLength(16)->regex('/^[A-Za-z0-9_]+$/')
                ->dehydrateStateUsing(fn (string $state): string => Str::lower($state))
                ->unique(ignoreRecord: true),
            Forms\Components\TextInput::make('email')
                ->email()->maxLength(255)->unique(ignoreRecord: true)
                ->dehydrateStateUsing(fn (?string $state): ?string => filled($state) ? Str::lower($state) : null),
            Forms\Components\TextInput::make('minecraft_uuid')
                ->label('Minecraft UUID')->default(fn (): string => (string) Str::uuid())->required()->uuid()->unique(ignoreRecord: true),
            Forms\Components\TextInput::make('password')
                ->password()->revealable()->minLength(12)
                ->required(fn (string $operation): bool => $operation === 'create')
                ->dehydrated(fn (?string $state): bool => filled($state)),
            Forms\Components\Select::make('status')->options([
                'active' => 'Active', 'disabled' => 'Disabled', 'banned' => 'Banned',
            ])->required()->default('active'),
            Forms\Components\Select::make('role')->options([
                'player' => 'Player', 'moderator' => 'Moderator', 'admin' => 'Admin',
            ])->required()->default('player'),
            Forms\Components\Textarea::make('ban_reason')->columnSpanFull(),
            Forms\Components\DateTimePicker::make('banned_until'),
        ])->columns(2);
    }

    public static function table(Table $table): Table
    {
        return $table->columns([
            Tables\Columns\TextColumn::make('username')->searchable()->sortable(),
            Tables\Columns\TextColumn::make('email')->searchable(),
            Tables\Columns\TextColumn::make('minecraft_uuid')->label('UUID')->searchable()->copyable(),
            Tables\Columns\TextColumn::make('status')->badge()->sortable(),
            Tables\Columns\TextColumn::make('role')->badge()->sortable(),
            Tables\Columns\TextColumn::make('last_login_at')->dateTime()->sortable(),
            Tables\Columns\TextColumn::make('created_at')->dateTime()->sortable(),
        ])->filters([
            Tables\Filters\SelectFilter::make('status')->options(['active' => 'Active', 'disabled' => 'Disabled', 'banned' => 'Banned']),
            Tables\Filters\SelectFilter::make('role')->options(['player' => 'Player', 'moderator' => 'Moderator', 'admin' => 'Admin']),
        ])->recordActions([
            Actions\EditAction::make(),
            Actions\Action::make('disable')->requiresConfirmation()
                ->visible(fn (LauncherUser $record): bool => $record->status === 'active')
                ->action(fn (LauncherUser $record) => self::changeStatus($record, 'disabled')),
            Actions\Action::make('ban')->requiresConfirmation()
                ->visible(fn (LauncherUser $record): bool => $record->status !== 'banned')
                ->action(fn (LauncherUser $record) => self::changeStatus($record, 'banned')),
            Actions\Action::make('enable')->requiresConfirmation()
                ->visible(fn (LauncherUser $record): bool => $record->status !== 'active')
                ->action(fn (LauncherUser $record) => self::changeStatus($record, 'active')),
            Actions\Action::make('revokeSessions')->label('Revoke sessions')->requiresConfirmation()
                ->action(fn (LauncherUser $record) => self::revokeSessions($record)),
        ])->defaultSort('id', 'desc');
    }

    public static function getPages(): array
    {
        return ['index' => Pages\ManageLauncherUsers::route('/')];
    }

    public static function changeStatus(LauncherUser $record, string $status): void
    {
        $record->update(['status' => $status, 'ban_reason' => $status === 'active' ? null : $record->ban_reason]);
        $record->sessions()->whereNull('revoked_at')->update(['revoked_at' => now()]);
        self::audit($record, match ($status) {
            'active' => 'user_enabled',
            'banned' => 'user_banned',
            default => 'user_disabled',
        });
    }

    public static function revokeSessions(LauncherUser $record): void
    {
        $record->sessions()->whereNull('revoked_at')->update(['revoked_at' => now()]);
        self::audit($record, 'sessions_revoked');
    }

    private static function audit(LauncherUser $record, string $action): void
    {
        DB::table('admin_audit_logs')->insert([
            'actor_id' => auth()->id(), 'launcher_user_id' => $record->id, 'action' => $action,
            'created_at' => now(), 'updated_at' => now(),
        ]);
    }
}
