fn main() {
    println!("cargo:rerun-if-env-changed=BLOCKFIELD_DISCORD_APPLICATION_ID");
    tauri_build::build()
}
