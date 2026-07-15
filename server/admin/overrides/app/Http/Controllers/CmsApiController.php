<?php

namespace App\Http\Controllers;

use App\Models\FeatureCard;
use App\Models\LauncherContent;
use App\Models\ModpackRelease;
use App\Models\NewsFeedEntry;
use App\Models\Translation;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Storage;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Str;
use Illuminate\Validation\ValidationException;
use Symfony\Component\HttpFoundation\StreamedResponse;

class CmsApiController extends Controller
{
    public function items(Request $request, string $collection): JsonResponse
    {
        $model = $this->modelFor($collection);
        $query = $model::query();
        if ($collection === 'modpack_releases') {
            if ($request->filled('id')) {
                $query->whereKey((int) $request->query('id'));
            } elseif ($request->boolean('active')) {
                $query->where('active', true)->where('status', 'published');
            }
        }
        $sort = (string) $request->query('sort', '-id');
        $direction = str_starts_with($sort, '-') ? 'desc' : 'asc';
        $column = ltrim($sort, '-') ?: 'id';
        if (! in_array($column, ['id', 'created_at', 'updated_at', 'version'], true)) {
            $column = 'id';
        }
        $limit = (int) $request->query('limit', 50);

        $query->orderBy($column, $direction);
        if ($limit > 0) {
            $query->limit($limit);
        }

        $records = $query->get();

        // ponytail: stitch related data for launcher_content so the Rust server sees the same JSON shape
        if ($collection === 'launcher_content') {
            $features = FeatureCard::orderBy('sort_order')->get()
                ->map(fn ($card) => ['icon' => $card->icon, 'title' => $card->title, 'desc' => $card->desc])
                ->toArray();

            $feed = NewsFeedEntry::orderBy('sort_order')->get()
                ->map(fn ($entry) => [
                    'tag' => $entry->tag,
                    'tone' => $entry->tone,
                    'date' => $entry->date,
                    'title' => $entry->title,
                    'body' => $entry->body,
                ])
                ->toArray();

            $translations = Translation::all()
                ->groupBy('locale')
                ->map(fn ($group) => $group->pluck('value', 'key'))
                ->toArray();

            foreach ($records as $record) {
                $record->features = $features;
                $record->feed = $feed;
                $record->translations = $translations;
            }
        }

        return response()->json(['data' => $records]);
    }

    public function storeItem(Request $request, string $collection): JsonResponse
    {
        $model = $this->modelFor($collection);
        $data = match ($collection) {
            'modpack_releases' => $request->validate([
                'version' => ['required', 'regex:/^\d+(\.\d+)+$/', 'max:64'],
                'minecraft_version' => ['required', 'string', 'max:64'],
                'modpack_zip' => ['nullable', 'string', 'required_without:zip_url', 'prohibits:zip_url'],
                'zip_url' => ['nullable', 'url:http,https', 'required_without:modpack_zip', 'prohibits:modpack_zip'],
                'prune' => ['nullable', 'array'],
                'prune.*' => ['string', 'max:512', function (string $attribute, mixed $value, \Closure $fail): void {
                    $path = str_ends_with((string) $value, '/*') ? substr((string) $value, 0, -2) : (string) $value;
                    if ($path === '' || str_starts_with($path, '/') || str_starts_with($path, '\\')
                        || str_contains($path, '..') || str_contains($path, ':') || str_contains($path, '\\')
                        || str_contains($path, '*') || str_contains($path, '?')) {
                        $fail('The prune rule is unsafe.');
                    }
                }],
                'java' => ['required_if:status,ready', 'nullable', 'array'],
                'java.version' => ['required_with:java', 'string', 'max:64'],
                'java.platform' => ['required_with:java', 'string', 'max:64'],
                'java.url' => ['required_with:java', 'url:https', 'max:2048', 'not_regex:/\/latest\//i'],
                'java.sha256' => ['required_with:java', 'string', 'size:64', 'regex:/^[a-f0-9]{64}$/i'],
                'java.size' => ['required_with:java', 'integer', 'min:1'],
                'forge' => ['nullable', 'array'],
                'forge.version' => ['required_with:forge', 'string', 'max:64'],
                'forge.url' => ['required_with:forge', 'url:https', 'max:2048'],
                'forge.sha256' => ['required_with:forge', 'string', 'size:64', 'regex:/^[a-f0-9]{64}$/i'],
                'forge.size' => ['required_with:forge', 'integer', 'min:1'],
                'archive_size' => ['required_with:zip_url', 'nullable', 'integer', 'min:1'],
                'archive_sha256' => ['required_with:zip_url', 'nullable', 'string', 'size:64', 'regex:/^[a-f0-9]{64}$/i'],
                'status' => ['nullable', 'in:draft,ready'],
            ]),
            'launcher_content' => $request->validate([
                'brand' => ['nullable', 'string', 'max:64'],
                'brand_subtitle' => ['nullable', 'string', 'max:64'],
                'chrome_title' => ['nullable', 'string', 'max:64'],
                'operation_name' => ['nullable', 'string', 'max:64'],
                'season' => ['nullable', 'string', 'max:64'],
                'description' => ['nullable', 'string', 'max:500'],
                'server_name' => ['nullable', 'string', 'max:64'],
                'copyright' => ['nullable', 'string', 'max:128'],
                'login_sector' => ['nullable', 'string', 'max:64'],
                'login_slogan' => ['nullable', 'string', 'max:128'],
                'support_label' => ['nullable', 'string', 'max:32'],
                'update_description' => ['nullable', 'string', 'max:500'],
                'settings_preferences' => ['nullable', 'string', 'max:64'],
            ]),
            default => abort(404),
        };

        if ($collection === 'modpack_releases' && isset($data['modpack_zip'])) {
            $path = Storage::disk('local')->path($data['modpack_zip']);
            if (! is_file($path)) {
                throw ValidationException::withMessages(['modpack_zip' => 'The uploaded ZIP does not exist.']);
            }
            $data['archive_size'] = filesize($path);
            $data['archive_sha256'] = hash_file('sha256', $path);
        }

        /** @var Model $record */
        $record = $model::query()->create($data);

        return response()->json(['data' => $record], 201);
    }

    public function storeFile(Request $request): JsonResponse
    {
        $request->validate(['file' => ['required', 'file', 'mimes:zip', 'max:2097152']]);

        $file = $request->file('file');
        $name = Str::uuid().'.'.($file->getClientOriginalExtension() ?: 'zip');
        $path = $file->storeAs('modpacks', $name);

        return response()->json([
            'data' => [
                'id' => $path,
                'filename_download' => $file->getClientOriginalName(),
            ],
        ], 201);
    }

    public function activateRelease(ModpackRelease $release): JsonResponse
    {
        abort_unless(in_array($release->status, ['ready', 'published'], true), 422, 'Release must be validated before activation.');
        DB::transaction(function () use ($release): void {
            $previous = ModpackRelease::where('active', true)->value('id');
            ModpackRelease::where('active', true)->update(['active' => false]);
            $release->update(['active' => true, 'status' => 'published', 'published_at' => now()]);
            DB::table('admin_audit_logs')->insert([
                'action' => 'release_published_cli',
                'metadata' => json_encode(['release_id' => $release->id, 'previous_release_id' => $previous]),
                'created_at' => now(), 'updated_at' => now(),
            ]);
        });
        return response()->json(['data' => $release->fresh()]);
    }

    public function asset(string $path): StreamedResponse
    {
        if ($path === '' || str_contains($path, '..') || str_starts_with($path, '/')) {
            abort(404);
        }

        $disk = Storage::disk('local');
        if (! $disk->exists($path)) {
            abort(404);
        }

        return $disk->download($path);
    }

    private function modelFor(string $collection): string
    {
        return match ($collection) {
            'modpack_releases' => ModpackRelease::class,
            'launcher_content' => LauncherContent::class,
            default => abort(404),
        };
    }
}
