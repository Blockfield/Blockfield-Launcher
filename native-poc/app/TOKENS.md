# Shell token mapping

Values follow the incumbent CSS/utility declarations. Component arbitrary values supplement theme.css where the theme has no named token. No visual-world changes. Motion defaults were checked against the exact installed-version npm source: `tailwindcss@4.3.3/package/theme.css:436,440,457,492-493`.

| Slint token | Value | React/CSS source |
| --- | --- | --- |
| `background` | `#070604` | `src/styles/theme.css:5` |
| `foreground` | `#f3e7d0` | `src/styles/theme.css:6` |
| `chrome` | `#0b0906` | `src/app/components/WindowChrome.tsx:52` |
| `card` | `#11100d` | `src/styles/theme.css:7` |
| `secondary` | `#18130d` | `src/styles/theme.css:13` |
| `primary` | `#f5a524` | `src/styles/theme.css:11` |
| `secondary-foreground` | `#c7ae86` | `src/styles/theme.css:14` |
| `muted-foreground` | `#8e7a5e` | `src/styles/theme.css:16` |
| `faint` | `#5e5040` | `src/app/components/Shell.tsx:114` |
| `border` | `#2a2116` | `src/styles/theme.css:21` |
| `ring` | `#8a571c` | `src/styles/theme.css:27` |
| `success` | `#82d66b` | `src/styles/theme.css:29` |
| `destructive` | `#ff5c5c` | `src/styles/theme.css:19` |
| `close-hover` | `#5a1714` | `src/app/components/WindowChrome.tsx:81` |
| `neutral-100` | `#f5f5f5` | `src/app/components/Logo.tsx:23` |
| `neutral-200` | `#e5e5e5` | `src/app/components/Shell.tsx:170` |
| `panel-gradient` | `@linear-gradient(135deg, #11100d 0%, #11100d 50%, #0b0906 100%)` | `src/app/components/ui-bits.tsx:103` |
| `base-size` | `16px` | `src/styles/theme.css:4` |
| `radius-sm` | `4px` | `src/styles/theme.css:110` |
| `radius-md` | `6px` | `src/styles/theme.css:111` |
| `radius-lg` | `8px` | `src/styles/theme.css:33` |
| `radius-xl` | `12px` | `src/styles/theme.css:113` |
| `spacing` | `4px` | `src/app/components/Shell.tsx:86 (gap-1, Tailwind 4 spacing base)` |
| `chrome-height` | `36px` | `src/app/components/WindowChrome.tsx:52 (h-9)` |
| `header-height` | `56px` | `src/app/components/Shell.tsx:76 (h-14)` |
| `footer-height` | `32px` | `src/app/components/Shell.tsx:114 (h-8)` |
| `grid-step` | `48px` | `src/app/components/Backdrop.tsx:18` |
| `text-xs` | `9px` | `src/app/components/Logo.tsx:26` |
| `text-label` | `10px` | `src/app/components/Shell.tsx:114` |
| `text-small` | `11px` | `src/app/components/Shell.tsx:103` |
| `text-brand` | `13px` | `src/app/components/Logo.tsx:23` |
| `weight-normal` | `400` | `src/styles/theme.css:26` |
| `weight-medium` | `600` | `src/styles/theme.css:25` |
| `tracking-chrome` | `3px` | `src/app/components/WindowChrome.tsx:56 (0.3em × 10px)` |
| `tracking-nav` | `2.8px` | `src/app/components/Shell.tsx:169 (0.28em × 10px)` |
| `tracking-brand` | `2.86px` | `src/app/components/Logo.tsx:23 (0.22em × 13px)` |
| `tracking-subtitle` | `1.62px` | `src/app/components/Logo.tsx:26 (0.18em × 9px)` |
| `tracking-role` | `1.08px` | `src/app/components/Shell.tsx:104 (0.12em × 9px)` |
| `tracking-user` | `1.1px` | `src/app/components/Shell.tsx:103 (tracking-widest = 0.1em × 11px)` |
| `tracking-footer` | `1.6px` | `src/app/components/Shell.tsx:114 (0.16em × 10px)` |
| `transition` | `150ms` | `src/app/components/Shell.tsx:169 (transition-colors, Tailwind 4 default)` |
| `transition-easing` | `cubic-bezier(0.4, 0, 0.2, 1)` | `src/app/components/Shell.tsx:169 (Tailwind 4 --default-transition-timing-function)` |
| `pulse-half` | `1000ms` | `src/app/components/ui-bits.tsx:23 (Tailwind pulse 2s, 50% opacity 0.5)` |
| `pulse-easing` | `cubic-bezier(0.4, 0, 0.6, 1)` | `src/app/components/ui-bits.tsx:23 (Tailwind --animate-pulse)` |
| `sans` | `"Inter"` | `src/styles/theme.css:131` |
| `mono` | `"IBM Plex Mono"` | `src/styles/fonts.css:1` |

Layout uses 1px borders, header padding 20px, chrome padding 12px, nav padding 14px/gap 8px, brand gap 10px and nav-group gap 24px exactly as the source utilities. Sizes below 1280×720 are excluded by the original native window minimum (`src-tauri/tauri.conf.json:16-19`).

Motion: color transitions use 150ms cubic-bezier(0.4,0,0.2,1). StatusDot interpolates opacity 1↔0.5 over each 1000ms half with cubic-bezier(0.4,0,0.6,1), matching Tailwind pulse. Shell footer uses pulse=false, preserving `Shell.tsx:146`; use StatusDot pulse=true in MainScreen/OperationBar.

Static decoration (radial radius explicitly uses the CSS farthest-corner distance, rather than Slint’s bounding-box-center default): GridBackdrop uses 48px grid at 6% amber × intensity, radial amber at (20%,20%) 5%→transparent 50%, green at (80%,80%) 4%→transparent 55% (`Backdrop.tsx:5-21`). TopoBackdrop reuses 18 rings and 12 verticals at opacity .07 (`icons/TopoPattern.tsx:1-45`). CornerTicks are 14px, 6px arms, 1px stroke (`icons/Tick.tsx:1-17`). GlowPanel is the original 1px outer tint plus panel gradient (`ui-bits.tsx:84-109`).

Logo: all 17 original SVG paths/colors are copied from `icons/LogoMark.tsx:21-83`. PlayerHead uses the original default SVG (`PlayerHead.tsx:2,22-28`), without remote account requests in this fixture-only POC.

Buttons inherit 600 weight from `theme.css:173-177`; this includes the brand wordmark/subtitle because Logo is inside Shell’s home button. Non-button user/footer text remains 400.

The footer updater button is **16px**, despite the footer’s 10px class: `theme.css:173-177` sets button font-size explicitly and its child spans inherit it (`Shell.tsx:116-126`). The IP/support/network text remains 10px. This was also confirmed in u05’s compiled frontend CSS; the inventory’s general footer size description is not an override of the actual cascade.
