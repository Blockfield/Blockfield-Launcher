# Blockfield Launcher

## After every Rust change

Run both in the root workspace and in `native/app/`, fix before claiming done:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --manifest-path native/app/Cargo.toml --all
cargo clippy --manifest-path native/app/Cargo.toml --all-targets -- -D warnings
```
