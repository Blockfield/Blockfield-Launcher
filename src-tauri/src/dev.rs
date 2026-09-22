//! Developer build extras, off unless the build itself sets `BLOCKFIELD_DEV_BUILD=1`.
//!
//! A production build compiles `option_env!` to `None`, so every function here is a constant
//! `false`/`None`/empty and the dev branch can be merged into main without changing behaviour.
//! Everything else is a runtime environment variable, read only by a dev build:
//!
//! | Variable | Effect |
//! |---|---|
//! | `BLOCKFIELD_DEV_SERVER` | `host:port` to auto-connect to, or `off` to land in the main menu |
//! | `BLOCKFIELD_DEV_FREEZE_PACK` | never run packwiz-installer: local mods, configs and jars stay |
//! | `BLOCKFIELD_DEV_JVM_ARGS` | extra JVM arguments (whitespace separated), e.g. the testbot port |
//! | `BLOCKFIELD_DEV_GAME_DIR` | default game directory of a fresh dev profile |
//! | `VITE_BLOCKFIELD_PACK_URL` | pack host; works in any build, a dev pack lives here |

use serde::Serialize;

pub const CONFIG_FILE: &str = "launcher-config-dev.json";

fn truthy(value: &str) -> bool {
    matches!(value.trim(), "1" | "true" | "yes" | "on")
}

/// True only in a build made with `BLOCKFIELD_DEV_BUILD=1`.
pub fn is_dev_build() -> bool {
    option_env!("BLOCKFIELD_DEV_BUILD").is_some_and(truthy)
}

fn var(name: &str) -> Option<String> {
    if !is_dev_build() {
        return None;
    }
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// `Some(None)` — start without auto-connect; `Some(Some(addr))` — connect there instead of the pack server.
pub fn server_override() -> Option<Option<String>> {
    var("BLOCKFIELD_DEV_SERVER").map(|value| (!truthy_off(&value)).then_some(value))
}

fn truthy_off(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "off" | "none" | "-")
}

/// Keep the game directory exactly as the developer left it: no download, no prune, no update prompt.
pub fn freeze_pack() -> bool {
    var("BLOCKFIELD_DEV_FREEZE_PACK").is_some_and(|value| truthy(&value))
}

pub fn jvm_args() -> Vec<String> {
    var("BLOCKFIELD_DEV_JVM_ARGS").map_or_else(Vec::new, |value| {
        value.split_whitespace().map(String::from).collect()
    })
}

pub fn game_dir_override() -> Option<String> {
    var("BLOCKFIELD_DEV_GAME_DIR")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevStatus {
    pub server: String,
    pub freeze_pack: bool,
    pub jvm_args: String,
    pub pack_url: String,
}

/// `None` in a production build: the UI then renders nothing.
pub fn status(pack_url: &str) -> Option<DevStatus> {
    is_dev_build().then(|| DevStatus {
        server: match server_override() {
            Some(Some(address)) => address,
            Some(None) => "off".to_string(),
            None => String::new(),
        },
        freeze_pack: freeze_pack(),
        jvm_args: jvm_args().join(" "),
        pack_url: pack_url.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_build_ignores_every_dev_variable() {
        // The test binary is built without BLOCKFIELD_DEV_BUILD unless CI asks for a dev build.
        if is_dev_build() {
            return;
        }
        unsafe {
            std::env::set_var("BLOCKFIELD_DEV_SERVER", "127.0.0.1:25599");
            std::env::set_var("BLOCKFIELD_DEV_FREEZE_PACK", "1");
            std::env::set_var("BLOCKFIELD_DEV_JVM_ARGS", "-Dbf.testbot.port=47777");
        }
        assert!(server_override().is_none());
        assert!(!freeze_pack());
        assert!(jvm_args().is_empty());
        assert!(status("https://blockfield.pro").is_none());
    }

    #[test]
    fn off_means_no_auto_connect() {
        assert!(truthy_off("off") && truthy_off("NONE"));
        assert!(!truthy_off("127.0.0.1:25599"));
    }
}
