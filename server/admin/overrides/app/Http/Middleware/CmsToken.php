<?php

namespace App\Http\Middleware;

use Closure;
use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\Response;

class CmsToken
{
    public function handle(Request $request, Closure $next): Response
    {
        $token = env('CMS_TOKEN');

        if ($token && ! hash_equals($token, (string) $request->bearerToken())) {
            abort(401);
        }

        return $next($request);
    }
}
