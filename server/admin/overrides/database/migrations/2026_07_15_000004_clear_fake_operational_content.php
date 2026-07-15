<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Support\Facades\DB;

return new class extends Migration
{
    public function up(): void
    {
        DB::table('launcher_content')->update([
            'server_ip' => null, 'server_region' => null, 'operators' => null, 'ping' => null,
            'region' => null, 'launcher_version' => null, 'coordinates' => null,
            'operator_handle' => null, 'operator_initials' => null, 'operator_rank' => null,
            'network_status' => null,
        ]);
    }

    public function down(): void {}
};
