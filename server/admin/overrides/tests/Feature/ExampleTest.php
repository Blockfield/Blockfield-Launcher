<?php

test('the root redirects to the admin panel', function (): void {
    $this->get('/')->assertRedirect('/admin');
});
