# skin3d offscreen feasibility renderer

Standalone Cargo workspace for native migration unit u07. It does not change the
launcher. The library is Rust + wgpu only; no window, surface, WebView or JS
runtime is needed to render. The optional [Slint preview example](SLINT.md)
integrates the same renderer into a native window without changing the library API.
[PORTING.md](PORTING.md) maps the port to the installed skinview3d sources. All bundled textures are synthetic, with no account/user data.

## API

```rust,no_run
use skin3d::{Cape, Model, RenderSize, Renderer, Skin};
# fn main() -> skin3d::Result<()> {
let skin = Skin::from_png(include_bytes!("fixtures/uv-skin.png"), Model::Auto)?;
let cape = Cape::from_png(include_bytes!("fixtures/uv-cape.png"))?;
let mut renderer = pollster::block_on(Renderer::new(
    RenderSize { width: 220, height: 300, scale_factor: 1.25 },
    &skin, Some(&cape), false,
))?;
let frame = renderer.render_frame(0.5, 0.0)?;
assert_eq!((frame.width, frame.height), (275, 375));
# Ok(()) }
```

`Skin::from_png` accepts classic/slim/automatic models, modern 64×64 and legacy
64×32 PNGs, including integer HD scales up to 1024px. Automatic inference matches
skinview-utils, including unused black/white/transparent arm areas. Legacy limbs
are mirrored, and fully opaque backgrounds have the upstream outer-layer cleanup.
`Cape::from_png` accepts 64×32, 22×17 and 46×22 layouts with integer scaling.
Decoding has dimension/allocation limits and rejects malformed input.

`render_frame(t_seconds, yaw_drag_offset)` returns a tightly packed, row-major,
top-left RGBA8 buffer with **straight alpha**. In Slint 1.18.1, copy it using
`SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&frame.rgba, frame.width, frame.height)`,
then `Image::from_rgba8(buffer)`; do not use a premultiplied image constructor. Render targets and
FXAA use premultiplied colors internally, then readback unpremultiplies once.
The logical aspect ratio remains 220/300; physical sizes are floor(logical × scale),
giving 220×300, 275×375 and 330×450 at 1×, 1.25× and 1.5×.

The camera is perspective, FOV 50°, near/far 0.1/2000, zoom 0.85, aimed at the
origin. The distance formula and clamp match skinview3d. Wheel/pinch zoom and pan
are absent, as in this launcher's preview. Recreate the renderer to resize or
replace textures. `adapter_info()` identifies the actual backend/device.

### Time and dragging

`t_seconds` is caller-owned **active animation time**, finite and nonnegative,
not a wall-clock timestamp. No time accumulates inside `render_frame`; seeking
and repeated timestamps are supported when there is no later drag event.
Walking uses speed 0.6 (natural angular speed 8) and auto-rotation is +0.6 rad/s.
Animation continues during a drag, while model auto-rotation stops and resumes
without catching up. The extra `yaw_drag_offset` is **camera azimuth** in radians,
additive to the controls, not a second model rotation.

```rust,ignore
renderer.controls.set_dragging(active_time, true)?; // pointer press
renderer.controls.drag_by(dx_css_px, dy_css_px, 300.0)?; // pointer move
renderer.controls.set_dragging(active_time, false)?; // release or cancel
```

Dragging ports OrbitControls' no-damping yaw/pitch math: both axes divide by the
logical canvas **height**, rotation speed 1, polar clamp epsilon 1e-6. Timestamps
must not precede the most recent drag event. Handle pointer capture/release and
one-finger touch in the UI; on hide/cancel, end any active drag at the current
active time. UI event plumbing is outside this crate.

The pure walking function evaluates upstream `animate()` at `progress=t*0.6`.
Upstream `PlayerAnimation.update()` evaluates the old progress, then increments
it; a later visual comparison must align the **evaluated progress**, rather than
assuming the incoming RAF wall time is already the animation progress. GPU math
is f32; byte-identical output is tested on the same adapter, not promised between
different GPUs/drivers.

### Slint visibility / frame pacing contract

- The UI owns one animation timer, targeting 60 fps only while the preview is
  mounted, visible and its window is not minimized. Stop that timer immediately
  when any condition becomes false; do not submit frames, poll the device, or
  repeatedly post UI callbacks while hidden.
- Advance active time only while visible; resume from the retained value without
  accumulating hidden wall time. Retain the last image for repaint. This matches
  the intent of skinview3d `renderPaused`; the current React component itself does
  not implement hidden-window pacing.
- Each render call has one scene pass, one FXAA pass, then one GPU readback. Run
  it off the UI thread, permit at most one frame in flight, and drop obsolete
  requests instead of queuing a backlog. Marshal only the finished buffer to
  Slint. The crate itself has no timer, worker, watcher, surface or window.
- On resize/asset changes, recreate the renderer at the new size. On adapter or
  render failure, surface the error and retain/clear the last image according to
  the existing UI policy; never spin retrying. Actual Slint/device-loss lifecycle
  handling belongs to the integration unit.

## Checks and frame dumps

From this directory (or use `--manifest-path native-poc/skin3d/Cargo.toml` from the
checkout root), always limit Rust builds to four jobs:

```sh
cargo fmt --check
CARGO_BUILD_JOBS=4 cargo clippy -- -D warnings
CARGO_BUILD_JOBS=4 cargo clippy --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 cargo test
CARGO_BUILD_JOBS=4 cargo build --release --examples
```

Headless Linux software check; DISPLAY and WAYLAND_DISPLAY are removed, Vulkan
is forced to Mesa's software ICD, and wgpu requests a fallback adapter. No display
server, desktop observation, container, privileged device access or OS install is
required on the tested host:

```sh
env -u DISPLAY -u WAYLAND_DISPLAY \
  VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json \
  LP_NUM_THREADS=4 \
  CARGO_BUILD_JOBS=4 timeout 120 cargo test --test offscreen -- --ignored

env -u DISPLAY -u WAYLAND_DISPLAY \
  VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json \
  LP_NUM_THREADS=4 \
  timeout 120 target/release/examples/render-frames \
  --software --frames 121 --fixture classic --output /absolute/report/path
```

The example writes frames 000–120 at t=0..2 seconds, 60 fps, for all three scales.
`--frames N` allows 1..121; `--fixture slim` and `--fixture legacy` exercise the
other safe bundled skins. `timings.txt` records the adapter and warmed render +
GPU readback + CPU unpack/unpremultiply latency (PNG encoding/file I/O excluded),
mean/median/p95/max, and a byte-identical repeat-frame assertion. Use the process
ownership helper prescribed by the repository when launching long-running jobs.

The ordinary suite is adapter-independent. The opt-in GPU test checks a nonempty
silhouette, transparent background, fractional alpha, animation, both drag input
paths, invalid timestamps, and identical repeated frames. CPU tests cover UV
pixel picks/orientation/winding, arm width/inflation, legacy mirrors/alpha cleanup,
model inference, DPI sizing and exact walking/drag angles. An upstream-generated
CSV checks 468 model/cape vertices across classic/slim, three times and six faces,
including world positions, normals and UVs. Its optional regeneration script needs
only the installed upstream npm packages, not a browser:

```sh
node fixtures/generate-reference.mjs /path/to/node_modules/skinview3d
```

## Limits to carry into the next unit

No visual parity claim: comparison against skinview3d recordings and real Slint
integration are assigned to later units. Metal/DX12 and hardware GPUs have not
been tested here. Browser canvas alpha rounding, pixel rasterization/depth bias,
transparent sorting ties and f32 vs JS f64 may differ slightly. The material and
FXAA equations are ported, but exact cross-backend pixels still need comparison.
Elytra/ears/name tags and other animation classes are absent because the launcher
does not request them. Fractional/non-integer HD skin sizes are rejected; the
supported Minecraft layouts use integer scales. No CPU rasterizer fallback is
needed when the software wgpu adapter works.
