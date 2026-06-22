<?php

namespace App\Http\Controllers;

use App\Models\LauncherContent;
use App\Models\LauncherUpdate;
use App\Models\ModpackRelease;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\Storage;
use Illuminate\Support\Str;
use Symfony\Component\HttpFoundation\StreamedResponse;

class CmsApiController extends Controller
{
    public function items(Request $request, string $collection): JsonResponse
    {
        $model = $this->modelFor($collection);
        $query = $model::query()->where('status', 'published');
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

        return response()->json(['data' => $query->get()]);
    }

    public function storeItem(Request $request, string $collection): JsonResponse
    {
        $model = $this->modelFor($collection);

        /** @var Model $record */
        $record = $model::query()->create($request->all());

        return response()->json(['data' => $record], 201);
    }

    public function storeFile(Request $request): JsonResponse
    {
        $request->validate(['file' => ['required', 'file']]);

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
            'launcher_updates' => LauncherUpdate::class,
            'launcher_content' => LauncherContent::class,
            default => abort(404),
        };
    }
}
