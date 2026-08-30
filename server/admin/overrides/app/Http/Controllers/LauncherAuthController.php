<?php

namespace App\Http\Controllers;

use App\Models\LauncherSession;
use App\Models\LauncherUser;
use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Hash;
use Illuminate\Support\Str;

class LauncherAuthController extends Controller
{
    public function register(Request $request): JsonResponse
    {
        $data = $request->validate([
            'username' => ['required', 'string', 'min:3', 'max:16', 'regex:/^[A-Za-z0-9_]+$/', 'unique:launcher_users,username'],
            'email' => ['required', 'email', 'max:255', 'unique:launcher_users,email'],
            'password' => ['required', 'string', 'min:8', 'max:1024', 'confirmed'],
            'remember' => ['sometimes', 'boolean'],
            'deviceName' => ['sometimes', 'nullable', 'string', 'max:255'],
        ]);
        $user = LauncherUser::create([
            'username' => $data['username'],
            'email' => $data['email'],
            'password' => $data['password'],
            'status' => 'active',
            'role' => 'player',
        ]);
        $this->audit('registration_succeeded', $user, $request);

        return response()->json(
            $this->issueSession($user, (bool) ($data['remember'] ?? false), $data['deviceName'] ?? null),
            201,
        );
    }

    public function login(Request $request): JsonResponse
    {
        $data = $request->validate([
            'username' => ['required', 'string', 'max:255'],
            'password' => ['required', 'string', 'max:1024'],
            'remember' => ['sometimes', 'boolean'],
            'deviceName' => ['sometimes', 'nullable', 'string', 'max:255'],
        ]);
        $identifier = Str::lower(trim($data['username']));
        $user = LauncherUser::query()
            ->where('username', $identifier)
            ->orWhere('email', $identifier)
            ->first();

        if (! $user || ! Hash::check($data['password'], $user->password) || ! $user->canAuthenticate()) {
            $this->audit('login_failed', $user, $request);
            return response()->json(['message' => 'Invalid credentials.'], 401);
        }

        $user->forceFill(['last_login_at' => now(), 'last_login_ip' => $request->ip()])->save();
        $this->audit('login_succeeded', $user, $request);

        return response()->json($this->issueSession($user, (bool) ($data['remember'] ?? false), $data['deviceName'] ?? null));
    }

    public function me(Request $request): JsonResponse
    {
        $session = $this->accessSession($request);
        if (! $session) {
            return response()->json(['message' => 'Unauthenticated.'], 401);
        }
        $session->update(['last_used_at' => now()]);
        return response()->json(['user' => $this->userPayload($session->user)]);
    }

    public function refresh(Request $request): JsonResponse
    {
        $data = $request->validate(['refreshToken' => ['required', 'string', 'max:255']]);
        $hash = hash('sha256', $data['refreshToken']);

        return DB::transaction(function () use ($hash): JsonResponse {
            $session = LauncherSession::with('user')
                ->where('refresh_token_hash', $hash)
                ->lockForUpdate()
                ->first();
            if (! $session) {
                return response()->json(['message' => 'Session expired.'], 401);
            }
            if ($session->rotated_at) {
                LauncherSession::where('family_id', $session->family_id)
                    ->whereNull('revoked_at')->update(['revoked_at' => now()]);
                return response()->json(['message' => 'Session expired.'], 401);
            }
            if ($session->revoked_at || $session->expires_at->isPast() || ! $session->user->canAuthenticate()) {
                return response()->json(['message' => 'Session expired.'], 401);
            }

            $session->update(['revoked_at' => now(), 'rotated_at' => now()]);
            return response()->json($this->issueSession(
                $session->user,
                $session->remember,
                $session->device_name,
                $session->family_id,
                $session->id,
            ));
        });
    }

    public function logout(Request $request): JsonResponse
    {
        if ($session = $this->accessSession($request, false)) {
            $session->update(['revoked_at' => now()]);
        }
        return response()->json(['status' => 'ok']);
    }

    private function accessSession(Request $request, bool $requireActiveUser = true): ?LauncherSession
    {
        $token = (string) $request->bearerToken();
        $session = $token === '' ? null : LauncherSession::with('user')
            ->where('access_token_hash', hash('sha256', $token))
            ->whereNull('revoked_at')
            ->where('access_expires_at', '>', now())
            ->first();
        if ($session && $requireActiveUser && ! $session->user->canAuthenticate()) {
            $session->update(['revoked_at' => now()]);
            return null;
        }
        return $session;
    }

    private function issueSession(
        LauncherUser $user,
        bool $remember,
        ?string $deviceName,
        ?string $familyId = null,
        ?int $parentSessionId = null,
    ): array
    {
        $access = $this->signedAccessToken($user);
        $refresh = Str::random(64);
        LauncherSession::create([
            'launcher_user_id' => $user->id,
            'access_token_hash' => hash('sha256', $access),
            'refresh_token_hash' => hash('sha256', $refresh),
            'family_id' => $familyId ?? (string) Str::uuid(),
            'parent_session_id' => $parentSessionId,
            'device_name' => $deviceName,
            'remember' => $remember,
            'access_expires_at' => now()->addMinutes(15),
            'expires_at' => now()->addDays($remember ? 30 : 1),
        ]);
        return [
            'accessToken' => $access,
            'refreshToken' => $refresh,
            'expiresIn' => 900,
            'user' => $this->userPayload($user),
        ];
    }

    private function signedAccessToken(LauncherUser $user): string
    {
        $key = (string) config('blockfield.auth_signing_key');
        if (strlen($key) < 32) {
            throw new \RuntimeException('AUTH_SIGNING_KEY is not configured.');
        }
        $payload = rtrim(strtr(base64_encode(json_encode([
            'sub' => $user->id, 'uuid' => $user->minecraft_uuid, 'exp' => now()->addMinutes(15)->timestamp,
            'nonce' => Str::random(24),
        ], JSON_THROW_ON_ERROR)), '+/', '-_'), '=');
        return $payload.'.'.hash_hmac('sha256', $payload, $key);
    }

    private function userPayload(LauncherUser $user): array
    {
        return $user->only(['id', 'username', 'email', 'minecraft_uuid', 'role']);
    }

    private function audit(string $action, ?LauncherUser $user, Request $request): void
    {
        DB::table('admin_audit_logs')->insert([
            'launcher_user_id' => $user?->id,
            'action' => $action,
            'metadata' => json_encode(['ip' => $request->ip()]),
            'created_at' => now(),
            'updated_at' => now(),
        ]);
    }
}
