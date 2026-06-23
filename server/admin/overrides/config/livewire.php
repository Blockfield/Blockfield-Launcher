<?php

// ponytail: override Livewire default max:12288 (12 MB) — modpacks are 200-500 MB.
return [
    'temporary_file_upload' => [
        'rules' => ['required', 'file', 'max:2097152'], // 2 GB
        'max_upload_time' => 300, // 5 minutes for large modpacks
    ],
];
