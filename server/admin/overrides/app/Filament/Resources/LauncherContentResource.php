<?php

namespace App\Filament\Resources;

use App\Filament\Resources\LauncherContentResource\Pages;
use App\Models\LauncherContent;
use Filament\Forms;
use Filament\Resources\Resource;
use Filament\Schemas\Schema;
use Filament\Schemas\Components\Utilities\Get;

class LauncherContentResource extends Resource
{
    protected static ?string $model = LauncherContent::class;
    protected static string|\UnitEnum|null $navigationGroup = 'Content';

    public static function form(Schema $schema): Schema
    {
        return $schema->components([
            Forms\Components\Select::make('editing_locale')
                ->label('Locale')
                ->options(['en' => 'English', 'ru' => 'Русский', 'uk' => 'Українська'])
                ->default('en')
                ->live()
                ->dehydrated(false),
            Forms\Components\TextInput::make('brand')->maxLength(64)->helperText('Primary launcher brand.'),
            Forms\Components\TextInput::make('brand_subtitle')->maxLength(64),
            Forms\Components\TextInput::make('chrome_title')->maxLength(64),
            Forms\Components\TextInput::make('operation_name')->maxLength(64),
            Forms\Components\TextInput::make('season')->maxLength(64)->helperText('Example: / SEASON 02'),
            Forms\Components\Textarea::make('description')->maxLength(500)->rows(3)->columnSpanFull(),
            Forms\Components\TextInput::make('server_name')->maxLength(64),
            Forms\Components\TextInput::make('copyright')->maxLength(128),
            Forms\Components\TextInput::make('login_sector')->maxLength(64),
            Forms\Components\TextInput::make('login_slogan')->maxLength(128),
            Forms\Components\TextInput::make('support_label')->maxLength(32),
            Forms\Components\TextInput::make('settings_preferences')->maxLength(64),
            Forms\Components\Textarea::make('update_description')->maxLength(500)->rows(4)->columnSpanFull(),
            Forms\Components\KeyValue::make('translations.en')
                ->label('English overrides')
                ->keyLabel('Translation key')->valueLabel('English text')
                ->helperText('Optional. Bundled English is used for missing keys.')
                ->visible(fn (Get $get): bool => $get('editing_locale') === 'en')
                ->columnSpanFull(),
            Forms\Components\KeyValue::make('translations.ru')
                ->label('Русские переводы')
                ->keyLabel('Ключ')->valueLabel('Текст')
                ->visible(fn (Get $get): bool => $get('editing_locale') === 'ru')
                ->columnSpanFull(),
            Forms\Components\KeyValue::make('translations.uk')
                ->label('Українські переклади')
                ->keyLabel('Ключ')->valueLabel('Текст')
                ->visible(fn (Get $get): bool => $get('editing_locale') === 'uk')
                ->columnSpanFull(),
        ])->columns(3);
    }

    public static function getPages(): array
    {
        return [
            'index' => Pages\ManageLauncherContent::route('/'),
        ];
    }
}
