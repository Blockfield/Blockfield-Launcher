# Blockfield Launcher

## Git workflow

Changes to `main` must go through a pull request. Never push or update `main` directly, including through GitHub APIs.

## After every Rust change

Run both, fix before claiming done:

```sh
cargo fmt
cargo clippy -- -D warnings
```

Target: `src-tauri/`
