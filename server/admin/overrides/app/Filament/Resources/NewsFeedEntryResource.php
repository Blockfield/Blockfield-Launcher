<?php

namespace App\Filament\Resources;

use App\Filament\Resources\NewsFeedEntryResource\Pages;
use App\Models\NewsFeedEntry;
use App\Support\CmsLocale;
use App\Support\LauncherContentCache;
use BackedEnum;
use Filament\Actions;
use Filament\Forms;
use Filament\Resources\Resource;
use Filament\Schemas\Schema;
use Filament\Tables;
use Filament\Tables\Table;

class NewsFeedEntryResource extends Resource
{
    protected static ?string $model = NewsFeedEntry::class;

    protected static string|BackedEnum|null $navigationIcon = 'heroicon-o-newspaper';

    protected static ?string $navigationLabel = 'News Feed';
    protected static string|\UnitEnum|null $navigationGroup = 'Content';

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
                ->maxLength(16)
                ->hint('e.g. "06.07"'),
            Forms\Components\TextInput::make(CmsLocale::field('title', 'title'))
                ->label('Title')
                ->maxLength(96),
            Forms\Components\Textarea::make(CmsLocale::field('body', 'body'))
                ->label('Body')
                ->maxLength(500)
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
        $locale = CmsLocale::current();

        return $table
            ->columns([
                Tables\Columns\TextColumn::make('tag')->badge()->sortable(),
                Tables\Columns\TextColumn::make('tone')->badge(),
                Tables\Columns\TextColumn::make('date')->searchable(),
                Tables\Columns\TextColumn::make('title')
                    ->state(fn (NewsFeedEntry $record): ?string => $locale === 'en'
                        ? $record->title
                        : (data_get($record->translations, "{$locale}.title") ?: $record->title))
                    ->searchable(),
                Tables\Columns\TextColumn::make('body')
                    ->state(fn (NewsFeedEntry $record): ?string => $locale === 'en'
                        ? $record->body
                        : (data_get($record->translations, "{$locale}.body") ?: $record->body))
                    ->limit(60),
                Tables\Columns\TextColumn::make('sort_order')->label('Order')->sortable(),
                Tables\Columns\TextColumn::make('updated_at')->dateTime()->sortable(),
            ])
            ->defaultSort('sort_order')
            ->recordActions([
                Actions\EditAction::make()
                    ->mutateDataUsing(
                        fn (array $data, NewsFeedEntry $record): array => CmsLocale::preserveOtherLocales(
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
            'index' => Pages\ManageNewsFeedEntries::route('/'),
        ];
    }
}
