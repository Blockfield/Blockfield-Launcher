<?php

namespace App\Filament\Resources;

use App\Filament\Resources\TranslationResource\Pages;
use App\Models\Translation;
use Filament\Actions;
use Filament\Forms;
use Filament\Resources\Resource;
use Filament\Schemas\Schema;
use Filament\Tables;
use Filament\Tables\Table;

class TranslationResource extends Resource
{
    protected static ?string $model = Translation::class;

    protected static $navigationIcon = 'heroicon-o-language';

    protected static ?string $navigationLabel = 'Translations';

    public static function form(Schema $schema): Schema
    {
        return $schema->components([
            Forms\Components\Select::make('locale')
                ->options(['en' => 'English', 'ru' => 'Русский', 'uk' => 'Українська'])
                ->default('en')
                ->required(),
            Forms\Components\TextInput::make('key')
                ->required()
                ->maxLength(255)
                ->hint('e.g. nav.deploy, shell.rank'),
            Forms\Components\Textarea::make('value')
                ->rows(2)
                ->columnSpanFull(),
        ])->columns(2);
    }

    public static function table(Table $table): Table
    {
        return $table
            ->columns([
                Tables\Columns\TextColumn::make('locale')->badge()->sortable(),
                Tables\Columns\TextColumn::make('key')->searchable()->sortable(),
                Tables\Columns\TextColumn::make('value')->limit(60)->searchable(),
                Tables\Columns\TextColumn::make('updated_at')->dateTime()->sortable(),
            ])
            ->defaultSort('locale')
            ->groups([
                Tables\Grouping\Group::make('locale')->label('Language'),
            ])
            ->recordActions([
                Actions\EditAction::make(),
                Actions\DeleteAction::make(),
            ]);
    }

    public static function getPages(): array
    {
        return [
            'index' => Pages\ManageTranslations::route('/'),
        ];
    }
}
