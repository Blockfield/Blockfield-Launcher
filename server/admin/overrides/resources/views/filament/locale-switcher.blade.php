<form method="GET" action="{{ url()->current() }}" class="flex items-center gap-2 px-2">
    <label for="cms-locale" class="text-sm font-medium text-gray-500 dark:text-gray-400">
        Content locale
    </label>
    <x-filament::input.wrapper>
        <x-filament::input.select
            id="cms-locale"
            name="locale"
            aria-label="Content locale"
            onchange="this.form.submit()"
        >
            @foreach ($locales as $code => $label)
                <option value="{{ $code }}" @selected($locale === $code)>{{ $label }}</option>
            @endforeach
        </x-filament::input.select>
    </x-filament::input.wrapper>
</form>
