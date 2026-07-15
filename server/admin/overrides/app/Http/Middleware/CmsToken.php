<?php

namespace App\Http\Middleware;

use Closure;
use Illuminate\Http\Request;
use Symfony\Component\HttpFoundation\Response;

class CmsToken
{
    public function handle(Request $request, Closure $next): Response
    {
        $token = (string) config('blockfield.cms_token', '');

        if (strlen($token) < 32) {
            abort(503, 'CMS authentication is not configured.');
        }

        if (! hash_equals($token, (string) $request->bearerToken())) {
            abort(401);
        }

        return $next($request);
    }
}
