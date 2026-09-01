<?php

namespace App\Filament\Resources;

use App\Filament\Resources\ModpackReleaseResource\Pages;
use App\Models\ModpackRelease;
use Filament\Actions;
use Filament\Forms;
use Filament\Notifications\Notification;
use Filament\Resources\Resource;
use Filament\Schemas\Schema;
use Filament\Tables;
use Filament\Tables\Table;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Http;
use Illuminate\Support\Facades\Storage;
use ZipArchive;

class ModpackReleaseResource extends Resource
{
    protected static ?string $model = ModpackRelease::class;
    protected static string|\UnitEnum|null $navigationGroup = 'Deployment';

    public static function canViewAny(): bool
    {
        return in_array(auth()->user()?->role, ['admin', 'editor'], true);
    }

    public static function form(Schema $schema): Schema
    {
        return $schema->components([
            Forms\Components\TextInput::make('version')->required()->regex('/^\d+(\.\d+)+$/')->maxLength(64)
                ->helperText('Numeric release version, for example 1.4.2.'),
            Forms\Components\TextInput::make('minecraft_version')->default('1.20.1')->required()->maxLength(64),
            Forms\Components\FileUpload::make('modpack_zip')->label('Modpack ZIP')
                ->directory('modpacks')->acceptedFileTypes(['application/zip', 'application/x-zip-compressed', 'application/x-zip'])
                ->maxSize(2_000_000)->required(),
            Forms\Components\TagsInput::make('prune')->placeholder('config/old/*')->helperText('Relative paths only; only a trailing /* wildcard is supported.')->columnSpanFull(),
            Forms\Components\TextInput::make('java.version')->label('Java version')->required(),
            Forms\Components\TextInput::make('java.platform')->label('Java platform')->required(),
            Forms\Components\TextInput::make('java.url')->label('Immutable Java archive URL')->url()->required(),
            Forms\Components\TextInput::make('java.sha256')->label('Java SHA-256')->length(64)->required(),
            Forms\Components\TextInput::make('java.size')->label('Java archive size')->numeric()->minValue(1)->required(),
        ])->columns(2);
    }

    public static function table(Table $table): Table
    {
        return $table->columns([
            Tables\Columns\IconColumn::make('active')->boolean(),
            Tables\Columns\TextColumn::make('version')->searchable()->sortable(),
            Tables\Columns\TextColumn::make('minecraft_version')->sortable(),
            Tables\Columns\TextColumn::make('status')->badge()->sortable(),
            Tables\Columns\TextColumn::make('archive_size')->formatStateUsing(fn (?int $state): string => $state ? number_format($state / 1048576, 1).' MB' : '—'),
            Tables\Columns\TextColumn::make('archive_sha256')->label('SHA-256')->limit(12)->copyable(),
            Tables\Columns\TextColumn::make('published_at')->dateTime()->sortable(),
        ])->defaultSort('id', 'desc')->recordActions([
            Actions\EditAction::make()->visible(fn (ModpackRelease $record): bool => ! $record->active),
            Actions\Action::make('validate')->visible(fn (ModpackRelease $record): bool => in_array($record->status, ['draft', 'failed'], true))
                ->action(fn (ModpackRelease $record) => self::validateRecord($record)),
            Actions\Action::make('publish')->requiresConfirmation()
                ->visible(fn (ModpackRelease $record): bool => auth()->user()?->role === 'admin' && in_array($record->status, ['ready', 'failed'], true))
                ->action(fn (ModpackRelease $record) => self::activate($record, 'release_published')),
            Actions\Action::make('rollback')->requiresConfirmation()
                ->visible(fn (ModpackRelease $record): bool => auth()->user()?->role === 'admin' && $record->status === 'published' && ! $record->active)
                ->action(fn (ModpackRelease $record) => self::activate($record, 'release_rolled_back')),
            Actions\Action::make('archive')->requiresConfirmation()
                ->visible(fn (ModpackRelease $record): bool => ! $record->active && $record->status !== 'archived')
                ->action(fn (ModpackRelease $record) => $record->update(['status' => 'archived'])),
            Actions\DeleteAction::make()->visible(fn (ModpackRelease $record): bool => ! $record->active && in_array($record->status, ['draft', 'failed', 'archived'], true)),
        ]);
    }

    public static function getPages(): array
    {
        return ['index' => Pages\ManageModpackReleases::route('/')];
    }

    private static function validateRecord(ModpackRelease $record): void
    {
        $error = self::recordError($record);
        if ($error) {
            $record->update(['status' => 'failed', 'publish_error' => $error]);
            Notification::make()->danger()->title('Validation failed')->body($error)->send();
            return;
        }
        if ($record->modpack_zip) {
            $path = Storage::disk('local')->path($record->modpack_zip);
            $record->archive_size = filesize($path);
            $record->archive_sha256 = hash_file('sha256', $path);
        }
        $record->status = 'ready';
        $record->publish_error = null;
        $record->save();
        Notification::make()->success()->title('Release is ready to publish')->send();
    }

    private static function activate(ModpackRelease $record, string $action): void
    {
        if ($error = self::recordError($record)) {
            $record->update(['status' => 'failed', 'publish_error' => $error]);
            Notification::make()->danger()->title('Publish failed')->body($error)->send();
            return;
        }
        $originalStatus = $record->status;
        $previous = ModpackRelease::where('active', true)->value('id');
        $backendActivated = false;
        $record->update(['status' => 'processing', 'publish_error' => null]);
        try {
            Http::withToken((string) config('blockfield.reload_token'))
                ->timeout(300)->post(config('blockfield.backend_url').'/reload', ['releaseId' => $record->id])->throw();
            $backendActivated = true;
            DB::transaction(function () use ($record, $action, $previous): void {
                ModpackRelease::where('active', true)->update(['active' => false]);
                $record->update(['active' => true, 'status' => 'published', 'published_at' => now()]);
                DB::table('admin_audit_logs')->insert([
                    'actor_id' => auth()->id(), 'action' => $action,
                    'metadata' => json_encode(['release_id' => $record->id, 'previous_release_id' => $previous]),
                    'created_at' => now(), 'updated_at' => now(),
                ]);
            });
            Notification::make()->success()->title('Release activated')->send();
        } catch (\Throwable $error) {
            $rollbackError = null;
            if ($backendActivated && $previous && $previous !== $record->id) {
                try {
                    Http::withToken((string) config('blockfield.reload_token'))
                        ->timeout(300)->post(config('blockfield.backend_url').'/reload', ['releaseId' => $previous])->throw();
                } catch (\Throwable $rollback) {
                    $rollbackError = $rollback->getMessage();
                }
            }
            $message = $error->getMessage().($rollbackError ? ' Backend rollback failed: '.$rollbackError : '');
            $record->update([
                'status' => $originalStatus === 'published' ? 'published' : 'failed',
                'publish_error' => $message,
            ]);
            DB::table('admin_audit_logs')->insert([
                'actor_id' => auth()->id(), 'action' => 'release_publish_failed',
                'metadata' => json_encode([
                    'release_id' => $record->id, 'previous_release_id' => $previous,
                    'failure_message' => $message,
                ]),
                'created_at' => now(), 'updated_at' => now(),
            ]);
            Notification::make()->danger()->title('Publish failed')->body($message)->send();
        }
    }

    private static function recordError(ModpackRelease $record): ?string
    {
        if (! $record->modpack_zip) return 'Upload a modpack ZIP.';
        if (! preg_match('/^\d+(\.\d+)+$/', $record->version)) return 'Version must be numeric and dot-separated.';
        $java = $record->java ?? [];
        if (! isset($java['version'], $java['platform'], $java['url'], $java['sha256'], $java['size'])) return 'Complete the Java runtime fields.';
        if (! str_starts_with($java['url'], 'https://') || strlen($java['sha256']) !== 64 || ! ctype_xdigit($java['sha256'])
            || str_contains(strtolower($java['url']), '/latest/') || (int) $java['size'] < 1) return 'Java must use an immutable HTTPS URL, size, and valid SHA-256.';
        foreach ($record->prune ?? [] as $pattern) {
            $path = str_ends_with($pattern, '/*') ? substr($pattern, 0, -2) : $pattern;
            if ($path === '' || str_starts_with($path, '/') || str_starts_with($path, '\\')
                || str_contains($path, '..') || str_contains($path, ':') || str_contains($path, '\\')
                || str_contains($path, '*') || str_contains($path, '?')) return 'A prune rule is unsafe.';
        }
        if ($record->modpack_zip) {
            $path = Storage::disk('local')->path($record->modpack_zip);
            if (! is_file($path)) return 'The uploaded modpack ZIP is missing.';
            $zip = new ZipArchive;
            if ($zip->open($path) !== true) return 'The uploaded modpack ZIP is corrupt.';
            for ($index = 0; $index < $zip->numFiles; $index++) {
                $name = $zip->getNameIndex($index);
                if (! is_string($name) || str_starts_with($name, '/') || str_starts_with($name, '\\')
                    || str_contains($name, '..') || str_contains($name, ':') || str_contains($name, '\\')) {
                    $zip->close();
                    return 'The modpack ZIP contains an unsafe path.';
                }
            }
            $zip->close();
        }
        return null;
    }
}
