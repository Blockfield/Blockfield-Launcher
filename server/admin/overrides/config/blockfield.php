<?php

return [
    'cms_token' => env('CMS_TOKEN', ''),
    'auth_signing_key' => env('AUTH_SIGNING_KEY', ''),
    'backend_url' => rtrim(env('BLOCKFIELD_API_URL', 'http://blockfield-api:3000/api/launcher/v1'), '/'),
    'reload_token' => env('RELOAD_TOKEN', ''),
    'admin' => [
        'email' => env('FILAMENT_ADMIN_EMAIL', ''),
        'password' => env('FILAMENT_ADMIN_PASSWORD', ''),
        'name' => env('FILAMENT_ADMIN_NAME', 'Blockfield Admin'),
    ],
];
