fn main() {
    println!("cargo:rerun-if-env-changed=BLOCKFIELD_DISCORD_APPLICATION_ID");
    let mut attributes = tauri_build::Attributes::new();
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        // Tauri's resource manifest reaches app binaries, but mock-app unit tests need v6 too.
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:\"type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'\""
        );
    }
    tauri_build::try_build(attributes).expect("failed to build Tauri resources")
}
