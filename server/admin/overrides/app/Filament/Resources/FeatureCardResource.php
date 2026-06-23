<?php

namespace App\Filament\Resources;

use App\Filament\Resources\FeatureCardResource\Pages;
use App\Models\FeatureCard;
use Filament\Actions;
use Filament\Forms;
use Filament\Resources\Resource;
use Filament\Schemas\Schema;
use Filament\Tables;
use Filament\Tables\Table;

class FeatureCardResource extends Resource
{
    protected static ?string $model = FeatureCard::class;

    protected static $navigationIcon = 'heroicon-o-flag';

    protected static ?string $navigationLabel = 'Feature Cards';

    public static function form(Schema $schema): Schema
    {
        return $schema->components([
            Forms\Components\TextInput::make('icon')
                ->maxLength(255)
                ->hint('Lucide icon name (flag, swords, truck, crosshair, etc.)'),
            Forms\Components\TextInput::make('title')
                ->maxLength(255),
            Forms\Components\Textarea::make('desc')
                ->label('Description')
                ->rows(2)
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
                Tables\Columns\TextColumn::make('icon')->searchable(),
                Tables\Columns\TextColumn::make('title')->searchable(),
                Tables\Columns\TextColumn::make('desc')->limit(60),
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
            'index' => Pages\ManageFeatureCards::route('/'),
        ];
    }
}
