<?php

namespace App\Filament\Resources;

use App\Filament\Resources\LauncherContentResource\Pages;
use App\Models\LauncherContent;
use App\Support\CmsLocale;
use Filament\Forms;
use Filament\Resources\Resource;
use Filament\Schemas\Schema;

class LauncherContentResource extends Resource
{
    protected static ?string $model = LauncherContent::class;
    protected static string|\UnitEnum|null $navigationGroup = 'Content';

    public static function form(Schema $schema): Schema
    {
        return $schema->components([
            Forms\Components\TextInput::make('brand')->maxLength(64)->helperText('Primary launcher brand.'),
            Forms\Components\TextInput::make(CmsLocale::field('brand_subtitle', 'content.brandSubtitle'))
                ->label('Brand subtitle')->maxLength(64),
            Forms\Components\TextInput::make(CmsLocale::field('chrome_title', 'content.chromeTitle'))
                ->label('Chrome title')->maxLength(64),
            Forms\Components\TextInput::make(CmsLocale::field('operation_name', 'content.operationName'))
                ->label('Operation name')->maxLength(64),
            Forms\Components\TextInput::make(CmsLocale::field('season', 'content.season'))
                ->label('Season')->maxLength(64)->helperText('Example: / SEASON 02'),
            Forms\Components\Textarea::make(CmsLocale::field('description', 'content.description'))
                ->label('Description')->maxLength(500)->rows(3)->columnSpanFull(),
            Forms\Components\TextInput::make(CmsLocale::field('server_name', 'content.serverName'))
                ->label('Server name')->maxLength(64),
            Forms\Components\TextInput::make('copyright')->maxLength(128),
            Forms\Components\TextInput::make(CmsLocale::field('login_sector', 'content.loginSector'))
                ->label('Login sector')->maxLength(64),
            Forms\Components\TextInput::make(CmsLocale::field('login_slogan', 'content.loginSlogan'))
                ->label('Login slogan')->maxLength(128),
            Forms\Components\TextInput::make(CmsLocale::field('support_label', 'content.supportLabel'))
                ->label('Support label')->maxLength(32),
            Forms\Components\TextInput::make(CmsLocale::field('settings_preferences', 'content.settingsPreferences'))
                ->label('Settings preferences')->maxLength(64),
            Forms\Components\Textarea::make(CmsLocale::field('update_description', 'content.updateDescription'))
                ->label('Update description')->maxLength(500)->rows(4)->columnSpanFull(),
        ])->columns(3);
    }

    public static function getPages(): array
    {
        return [
            'index' => Pages\ManageLauncherContent::route('/'),
        ];
    }
}
