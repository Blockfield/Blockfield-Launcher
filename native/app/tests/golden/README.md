# Per-OS Layer 2 goldens

`<state>-<shot>.png`, 1280x720, scale 1x, default renderer, `--freeze-motion`.
Committed only after eyeballing every image against the parity reports.
`linux/` ships with u18; `windows/` + `macos/` are produced by CI
(`workflow_dispatch` with `update_goldens=true`) and committed after review.
