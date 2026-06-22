<?php

namespace App\Filament\Resources;

use App\Filament\Resources\LauncherUpdateResource\Pages;
use App\Models\LauncherUpdate;
use Filament\Actions;
use Filament\Forms;
use Filament\Resources\Resource;
use Filament\Schemas\Schema;
use Filament\Tables;
use Filament\Tables\Table;

class LauncherUpdateResource extends Resource
{
    protected static ?string $model = LauncherUpdate::class;

    public static function form(Schema $schema): Schema
    {
        return $schema->components([
            Forms\Components\Select::make('status')
                ->options(['draft' => 'Draft', 'published' => 'Published'])
                ->default('draft')
                ->required(),
            Forms\Components\TextInput::make('version')->required()->maxLength(255),
            Forms\Components\TextInput::make('pub_date')
                ->label('Publication date')
                ->placeholder('2026-06-18T00:00:00Z')
                ->maxLength(255),
            Forms\Components\TextInput::make('windows_url')
                ->label('Windows installer URL')
                ->url()
                ->maxLength(255),
            Forms\Components\Textarea::make('windows_signature')
                ->label('Windows signature')
                ->rows(3)
                ->columnSpanFull(),
            Forms\Components\Textarea::make('notes')
                ->rows(5)
                ->columnSpanFull(),
            self::jsonTextarea('platforms', 'Raw Tauri platforms JSON'),
        ])->columns(2);
    }

    public static function table(Table $table): Table
    {
        return $table
            ->columns([
                Tables\Columns\TextColumn::make('version')->searchable()->sortable(),
                Tables\Columns\TextColumn::make('status')->badge()->sortable(),
                Tables\Columns\TextColumn::make('pub_date')->sortable(),
                Tables\Columns\TextColumn::make('created_at')->dateTime()->sortable(),
            ])
            ->defaultSort('id', 'desc')
            ->recordActions([
                Actions\EditAction::make(),
                Actions\DeleteAction::make(),
            ]);
    }

    public static function getPages(): array
    {
        return [
            'index' => Pages\ManageLauncherUpdates::route('/'),
        ];
    }

    private static function jsonTextarea(string $name, string $label): Forms\Components\Textarea
    {
        return Forms\Components\Textarea::make($name)
            ->label($label)
            ->rows(7)
            ->formatStateUsing(fn ($state): string => $state ? json_encode($state, JSON_PRETTY_PRINT | JSON_UNESCAPED_UNICODE) : '')
            ->dehydrateStateUsing(fn (?string $state): mixed => filled($state) ? json_decode($state, true) : null)
            ->rules(['nullable', 'json'])
            ->columnSpanFull();
    }
}
