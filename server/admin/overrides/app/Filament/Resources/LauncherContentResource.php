<?php

namespace App\Filament\Resources;

use App\Filament\Resources\LauncherContentResource\Pages;
use App\Models\LauncherContent;
use Filament\Forms;
use Filament\Resources\Resource;
use Filament\Schemas\Schema;

class LauncherContentResource extends Resource
{
    protected static ?string $model = LauncherContent::class;

    public static function form(Schema $schema): Schema
    {
        return $schema->components([
            Forms\Components\Select::make('status')
                ->options(['draft' => 'Draft', 'published' => 'Published'])
                ->default('draft')
                ->required(),
            Forms\Components\TextInput::make('brand')->maxLength(255),
            Forms\Components\TextInput::make('brand_subtitle')->maxLength(255),
            Forms\Components\TextInput::make('chrome_title')->maxLength(255),
            Forms\Components\TextInput::make('operation_name')->maxLength(255),
            Forms\Components\TextInput::make('season')->maxLength(255),
            Forms\Components\Textarea::make('description')->rows(3)->columnSpanFull(),
            Forms\Components\TextInput::make('server_name')->maxLength(255),
            Forms\Components\TextInput::make('server_ip')->maxLength(255),
            Forms\Components\TextInput::make('server_region')->maxLength(255),
            Forms\Components\TextInput::make('operators')->maxLength(255),
            Forms\Components\TextInput::make('ping')->maxLength(255),
            Forms\Components\TextInput::make('region')->maxLength(255),
            Forms\Components\TextInput::make('launcher_version')->maxLength(255),
            Forms\Components\TextInput::make('coordinates')->maxLength(255),
            Forms\Components\TextInput::make('copyright')->maxLength(255),
            Forms\Components\TextInput::make('login_sector')->maxLength(255),
            Forms\Components\TextInput::make('login_slogan')->maxLength(255),
            Forms\Components\TextInput::make('operator_handle')->maxLength(255),
            Forms\Components\TextInput::make('operator_initials')->maxLength(255),
            Forms\Components\TextInput::make('operator_rank')->maxLength(255),
            Forms\Components\TextInput::make('support_label')->maxLength(255),
            Forms\Components\TextInput::make('network_status')->maxLength(255),
            Forms\Components\TextInput::make('settings_preferences')->maxLength(255),
            Forms\Components\Textarea::make('update_description')->rows(4)->columnSpanFull(),
            self::jsonTextarea('translations', 'Translations JSON'),
            self::jsonTextarea('features', 'Feature cards JSON'),
            self::jsonTextarea('feed', 'News feed JSON'),
        ])->columns(3);
    }

    public static function getPages(): array
    {
        return [
            'index' => Pages\ManageLauncherContent::route('/'),
        ];
    }

    private static function jsonTextarea(string $name, string $label): Forms\Components\Textarea
    {
        return Forms\Components\Textarea::make($name)
            ->label($label)
            ->rows(8)
            ->formatStateUsing(fn ($state): string => $state ? json_encode($state, JSON_PRETTY_PRINT | JSON_UNESCAPED_UNICODE) : '')
            ->dehydrateStateUsing(fn (?string $state): mixed => filled($state) ? json_decode($state, true) : null)
            ->rules(['nullable', 'json'])
            ->columnSpanFull();
    }
}
