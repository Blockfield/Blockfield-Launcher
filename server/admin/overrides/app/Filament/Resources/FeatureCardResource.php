<?php

namespace App\Filament\Resources;

use App\Filament\Resources\FeatureCardResource\Pages;
use App\Models\FeatureCard;
use App\Support\CmsLocale;
use App\Support\LauncherContentCache;
use BackedEnum;
use Filament\Actions;
use Filament\Forms;
use Filament\Resources\Resource;
use Filament\Schemas\Schema;
use Filament\Tables;
use Filament\Tables\Table;

class FeatureCardResource extends Resource
{
    protected static ?string $model = FeatureCard::class;

    protected static string|BackedEnum|null $navigationIcon = 'heroicon-o-flag';

    protected static ?string $navigationLabel = 'Feature Cards';
    protected static string|\UnitEnum|null $navigationGroup = 'Content';

    public static function form(Schema $schema): Schema
    {
        return $schema->components([
            Forms\Components\TextInput::make('icon')
                ->maxLength(32)
                ->hint('Lucide icon name (flag, swords, truck, crosshair, etc.)'),
            Forms\Components\TextInput::make(CmsLocale::field('title', 'title'))
                ->label('Title')
                ->maxLength(64),
            Forms\Components\Textarea::make(CmsLocale::field('desc', 'desc'))
                ->label('Description')
                ->maxLength(160)
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
        $locale = CmsLocale::current();

        return $table
            ->columns([
                Tables\Columns\TextColumn::make('icon')->searchable(),
                Tables\Columns\TextColumn::make('title')
                    ->state(fn (FeatureCard $record): ?string => $locale === 'en'
                        ? $record->title
                        : (data_get($record->translations, "{$locale}.title") ?: $record->title))
                    ->searchable(),
                Tables\Columns\TextColumn::make('desc')
                    ->state(fn (FeatureCard $record): ?string => $locale === 'en'
                        ? $record->desc
                        : (data_get($record->translations, "{$locale}.desc") ?: $record->desc))
                    ->limit(60),
                Tables\Columns\TextColumn::make('sort_order')->label('Order')->sortable(),
                Tables\Columns\TextColumn::make('updated_at')->dateTime()->sortable(),
            ])
            ->defaultSort('sort_order')
            ->recordActions([
                Actions\EditAction::make()
                    ->mutateDataUsing(
                        fn (array $data, FeatureCard $record): array => CmsLocale::preserveOtherLocales(
                            $data,
                            $record->translations,
                        ),
                    )
                    ->after(fn () => LauncherContentCache::invalidate()),
                Actions\DeleteAction::make()->after(fn () => LauncherContentCache::invalidate()),
            ]);
    }

    public static function getPages(): array
    {
        return [
            'index' => Pages\ManageFeatureCards::route('/'),
        ];
    }
}
