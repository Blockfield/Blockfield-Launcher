<?php

namespace App\Filament\Resources;

use App\Filament\Resources\ModpackReleaseResource\Pages;
use App\Models\ModpackRelease;
use Filament\Actions;
use Filament\Forms;
use Filament\Resources\Resource;
use Filament\Schemas\Schema;
use Filament\Tables;
use Filament\Tables\Table;

class ModpackReleaseResource extends Resource
{
    protected static ?string $model = ModpackRelease::class;

    public static function form(Schema $schema): Schema
    {
        return $schema->components([
            Forms\Components\TextInput::make('version')->required()->maxLength(255),
            Forms\Components\TextInput::make('minecraft_version')
                ->default('1.20.1')
                ->required()
                ->maxLength(255),
            Forms\Components\FileUpload::make('modpack_zip')
                ->label('Modpack ZIP')
                ->directory('modpacks')
                ->acceptedFileTypes(['application/zip', 'application/x-zip-compressed', 'application/x-zip'])
                ->maxSize(2_000_000),
            Forms\Components\TextInput::make('zip_url')
                ->label('External ZIP URL')
                ->url()
                ->maxLength(255),
            self::jsonTextarea('prune', 'Prune paths'),
            self::jsonTextarea('java', 'Java runtime'),
            self::jsonTextarea('forge', 'Forge installer'),
        ])->columns(2);
    }

    public static function table(Table $table): Table
    {
        return $table
            ->columns([
                Tables\Columns\TextColumn::make('version')->searchable()->sortable(),
                Tables\Columns\TextColumn::make('minecraft_version')->sortable(),
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
            'index' => Pages\ManageModpackReleases::route('/'),
        ];
    }

    private static function jsonTextarea(string $name, string $label): Forms\Components\Textarea
    {
        return Forms\Components\Textarea::make($name)
            ->label($label)
            ->rows(6)
            ->formatStateUsing(fn ($state): string => $state ? json_encode($state, JSON_PRETTY_PRINT | JSON_UNESCAPED_UNICODE) : '')
            ->dehydrateStateUsing(fn (?string $state): mixed => filled($state) ? json_decode($state, true) : null)
            ->rules(['nullable', 'json'])
            ->columnSpanFull();
    }
}
