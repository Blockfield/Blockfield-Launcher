<?php

namespace App\Filament\Resources;

use App\Filament\Resources\NewsFeedEntryResource\Pages;
use App\Models\NewsFeedEntry;
use Filament\Actions;
use Filament\Forms;
use Filament\Resources\Resource;
use Filament\Schemas\Schema;
use Filament\Tables;
use Filament\Tables\Table;

class NewsFeedEntryResource extends Resource
{
    protected static ?string $model = NewsFeedEntry::class;

    protected static ?string $navigationIcon = 'heroicon-o-newspaper';

    protected static ?string $navigationLabel = 'News Feed';

    public static function form(Schema $schema): Schema
    {
        return $schema->components([
            Forms\Components\Select::make('tag')
                ->options(['PATCH' => 'PATCH', 'EVENT' => 'EVENT', 'OPS' => 'OPS'])
                ->default('PATCH')
                ->required(),
            Forms\Components\Select::make('tone')
                ->options(['amber' => 'Amber', 'green' => 'Green', 'sand' => 'Sand'])
                ->default('amber')
                ->required(),
            Forms\Components\TextInput::make('date')
                ->maxLength(255)
                ->hint('e.g. "06.07"'),
            Forms\Components\TextInput::make('title')
                ->maxLength(255),
            Forms\Components\Textarea::make('body')
                ->rows(3)
                ->columnSpanFull(),
            Forms\Components\TextInput::make('sort_order')
                ->label('Sort order')
                ->numeric()
                ->default(0),
        ])->columns(2);
    }

    public static function table(Table $table): Table
    {
        return $table
            ->columns([
                Tables\Columns\TextColumn::make('tag')->badge()->sortable(),
                Tables\Columns\TextColumn::make('tone')->badge(),
                Tables\Columns\TextColumn::make('date')->searchable(),
                Tables\Columns\TextColumn::make('title')->searchable(),
                Tables\Columns\TextColumn::make('body')->limit(60),
                Tables\Columns\TextColumn::make('sort_order')->label('Order')->sortable(),
                Tables\Columns\TextColumn::make('updated_at')->dateTime()->sortable(),
            ])
            ->defaultSort('sort_order')
            ->recordActions([
                Actions\EditAction::make(),
                Actions\DeleteAction::make(),
            ]);
    }

    public static function getPages(): array
    {
        return [
            'index' => Pages\ManageNewsFeedEntries::route('/'),
        ];
    }
}
