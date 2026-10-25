# Port provenance and parity boundaries

The specification is `src/app/components/SkinPreview.tsx:6–67` at launcher commit
9ba1771: 220×300, zoom 0.85, `WalkingAnimation.speed=0.6`, `autoRotateSpeed=0.6`,
zoom disabled, default OrbitControls drag rotation, explicitly selected arm model,
optional cape. No shipped launcher file is changed.

Source inspected: **skinview3d 3.4.2**, **skinview-utils 0.7.1**, **three 0.156.1**,
installed under the main checkout's `node_modules/.pnpm`. The table's paths and
line numbers refer to the published, installed JavaScript, not generated guesses
about TypeScript line numbers. Sources can also be retrieved from the versioned
[skinview3d package](https://unpkg.com/skinview3d@3.4.2/libs/model.js),
[skinview-utils package](https://unpkg.com/skinview-utils@0.7.1/build/process.js), and
[three package](https://unpkg.com/three@0.156.1/examples/jsm/shaders/FXAAShader.js).

| Behaviour                                                                              | Exact installed source location                                                                       | Port                                                                            |
| -------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| Face UV atlas and bottom-face reversal                                                 | `skinview3d/libs/model.js:2–37`                                                                       | `src/model.rs`, `box_vertices`                                                  |
| Box vertex order, normals, triangle winding                                            | `three/src/geometries/BoxGeometry.js:44–49,77–108,128–136`                                            | six planes with the same indexed corners                                        |
| Opaque inner, transparent double-sided outer, alpha test 1e-5, limb polygon offset 1/1 | `skinview3d/libs/model.js:143–158`                                                                    | `src/player.wgsl`; separate biased and double-pass pipelines                    |
| Head 8³/9³, body 8×12×4/8.5×12.5×4.5                                                   | `skinview3d/libs/model.js:159–183`                                                                    | `player_parts`                                                                  |
| Classic/slim arm widths, offsets and atlas origins                                     | `skinview3d/libs/model.js:184–241`                                                                    | width 4/3, outer 4.5/3.5, pivot x ±1/±0.5                                       |
| Leg pivots and layer sizes                                                             | `skinview3d/libs/model.js:242–275`                                                                    | x ±1.9, y −12, z −0.1, mesh y −6                                                |
| Cape size, atlas and mesh offset                                                       | `skinview3d/libs/model.js:347–360`                                                                    | 10×16×1, mesh (0,−8,0.5), atlas 64×32                                           |
| Player/skin/cape hierarchy and cape default yaw/pitch                                  | `skinview3d/libs/model.js:485,513–523`                                                                | skin y=8; cape (0,8,−2), XYZ rotation (walk pitch,π,0)                          |
| Animation speed/progress ordering                                                      | `skinview3d/libs/animation.js:70–81`                                                                  | evaluate at caller's active progress; no dt accumulation                        |
| Walking legs, arms, head bob and cape                                                  | `skinview3d/libs/animation.js:183–207`                                                                | `walking_pose`; natural frequency 8 × speed 0.6                                 |
| Ambient 3 and camera point light 0.6                                                   | `skinview3d/libs/viewer.js:74–85,257–262`                                                             | no directional light; point position follows orbiting camera                    |
| Linear material colors and renderer/composer configuration                             | `skinview3d/libs/viewer.js:262–300`                                                                   | raw UNORM textures/target, linear intermediate, transparent clear               |
| Default camera/zoom/FOV                                                                | `skinview3d/libs/viewer.js:343–345,647–657`                                                           | FOV=50, distance=(4.5+16.5/tan(FOV/2)/0.85), clamp 10..256                      |
| Camera near/far defaults                                                               | `three/src/cameras/PerspectiveCamera.js:6–18`                                                         | 0.1/2000, right-handed projection with wgpu depth 0..1                          |
| Nearest skin/cape filters                                                              | `skinview3d/libs/viewer.js:395–412`                                                                   | nearest sampler, no mipmaps                                                     |
| Auto-rotate pause/resume during pointer/touch rotation                                 | `skinview3d/libs/viewer.js:370–386,552–555`                                                           | explicit press/move/release controls; pause only rotation                       |
| OrbitControls defaults and pointer scaling                                             | `three/examples/jsm/controls/OrbitControls.js:64–74,197–206,226–239,664–680`                          | yaw and polar drag by 2π×delta/height, no damping or zoom                       |
| Pause stops requestAnimationFrame                                                      | `skinview3d/libs/viewer.js:597–618`                                                                   | no internal scheduling; README requires caller to stop requesting hidden frames |
| Legacy limb mirroring and opaque background cleanup                                    | `skinview-utils/build/process.js:16–115`                                                              | `src/texture.rs`; same face copy rectangles and clear areas                     |
| Cape input layouts                                                                     | `skinview-utils/build/process.js:117–140`                                                             | same atlas padding for integer scales                                           |
| Automatic slim inference                                                               | `skinview-utils/build/process.js:172–229`                                                             | four unused regions; transparent OR all black OR all white                      |
| MeshStandard defaults                                                                  | `three/src/materials/MeshStandardMaterial.js:18–20`                                                   | roughness 1, metalness 0; dielectric F0 .04                                     |
| Inverse-square point falloff                                                           | `three/src/renderers/shaders/ShaderChunk/lights_pars_begin.glsl.js:51–79`                             | .6/max(distance²,.01), no cutoff, current nonlegacy lights                      |
| Lambert ambient/direct and GGX direct specular                                         | `three/src/renderers/shaders/ShaderChunk/lights_physical_pars_fragment.glsl.js:66–85,145–190,476–505` | `player.wgsl`; GGX simplifies because roughness=1 and camera/light coincide     |
| Schlick approximation                                                                  | `three/src/renderers/shaders/ShaderChunk/common.glsl.js:131–140`                                      | dotVH=1, same exp2 approximation                                                |
| Double-sided transparent drawing                                                       | `three/src/renderers/WebGLRenderer.js:1456–1466`                                                      | back faces then front faces; alpha blending and depth writes                    |
| FXAA, alpha-aware contrast, 5 samples, threshold .2                                    | `three/examples/jsm/shaders/FXAAShader.js:95–317`                                                     | `src/fxaa.wgsl`, same branches and increasing sample distances                  |

Antialiasing is deliberately **one sample + FXAA**, matching upstream's
WebGLRenderer default (`antialias=false`) and EffectComposer FXAA pass, rather than
adding MSAA. A float32 depth buffer matches its WebGL2 path. The scene's UNORM8
intermediate matches EffectComposer's render target. Neither the CanvasTexture
nor the FXAA ShaderMaterial requests an sRGB conversion here; adding one changes
the existing preview. Ambient diffuse is color × 3/π; the point contribution is
small at the preview's camera distance. Output alpha is unpremultiplied for Slint.

The shader port preserves the upstream algorithm, but no pixel-level comparison
against the browser renderer is claimed. See README's remaining limits. The
`fixtures/upstream-vertices.csv` reference is generated directly from upstream
PlayerObject + WalkingAnimation + Three matrices without a DOM/GPU; its 468 rows
are compared against the Rust mesh at 1e-5 tolerance. It includes cape yaw and
slim arm offsets, which simple silhouette tests cannot establish.

Inspected skinview3d file SHA-256s:

```text
39bccda0d98f869a441992fd9a10c7063a9e1f6f455c9384cd5a44ea2f51cdea  libs/model.js
33fd8a26b393f4ca5c60418146469dc6fdb1b730aea7cdabe7b3c04d6a4b72b7  libs/animation.js
bebb95103fe3bcfc7ff1d81423df4f2aee7f139d324a14ba1528dfa7fd564b88  libs/viewer.js
```

## Attribution

skinview3d and skinview-utils are MIT licensed, Copyright (c) 2014–2018 Kent
Rasmussen; Copyright (c) 2017–2022 Haowei Wen, Sean Boult and contributors. Three.js
is MIT licensed, Copyright © 2010–2023 three.js authors. Its FXAA source includes
the NVIDIA 2014–2015 redistribution notice. Complete notices are retained in
[THIRD_PARTY_LICENSES](THIRD_PARTY_LICENSES). The Rust crate uses the launcher's
GPL-3.0-only license. Synthetic PNG fixtures were authored for this POC; they
contain no third-party character artwork or real user's skin.
