//! Native file pickers (G1). Ports the Tauri `@tauri-apps/plugin-dialog`
//! behaviour from `FirstRunScreen.tsx` (`handleBrowse`) and
//! `SettingsScreen.tsx` (`handleBrowse`, `handleBrowsePng`):
//! game-dir is a folder picker whose result gains a `Blockfield` suffix,
//! java is a file picker, skin/cape are PNG-only file pickers.
//!
//! The `pick_*` wrappers own the modal `rfd` dialog (Windows+Linux only,
//! like the rest of the native shell); the `apply_*` helpers own the UI
//! update so tests drive them without opening a dialog. A cancelled dialog
//! is a silent no-op on both screens, exactly like Tauri's `if (selected)`
//! guard; only FirstRun dialog *errors* surface in the UI, and `rfd` folds
//! failures into `None`, so there is no error path to mirror here.
use crate::{AppWindow, SettingsForm};
use slint::ComponentHandle;

/// Tauri FirstRun/Settings error text for a failed browse.
pub const BROWSE_ERROR: &str =
    "Не удалось открыть выбор файла. Укажите путь вручную или повторите.";

/// Port of `selectedGameDirectory` (`src/lib/game-directory.ts`): a parent
/// already named `Blockfield` (any case, trailing separators ignored) is
/// kept as-is, otherwise `Blockfield` is appended with the parent's own
/// separator mix (backslash wins, so Windows/UNC parents stay native).
pub fn selected_game_directory(parent: &str) -> String {
    let trimmed = parent.trim_end_matches(['/', '\\']);
    let base = trimmed.rsplit(['/', '\\']).next().unwrap_or("");
    if base.eq_ignore_ascii_case("blockfield") {
        return parent.to_string();
    }
    let sep = if parent.contains('\\') { '\\' } else { '/' };
    format!("{trimmed}{sep}Blockfield")
}

/// Folder picker for the game directory (`Select Game Directory`).
pub fn pick_game_dir() -> Option<String> {
    rfd::FileDialog::new()
        .set_title("Select Game Directory")
        .pick_folder()
        .map(|path| path.to_string_lossy().into_owned())
}

/// File picker for the Java executable (`Select Java Executable`).
pub fn pick_java() -> Option<String> {
    rfd::FileDialog::new()
        .set_title("Select Java Executable")
        .pick_file()
        .map(|path| path.to_string_lossy().into_owned())
}

/// PNG-only file picker for skin/cape textures (Tauri passes only a filter).
pub fn pick_png() -> Option<String> {
    rfd::FileDialog::new()
        .add_filter("PNG", &["png"])
        .pick_file()
        .map(|path| path.to_string_lossy().into_owned())
}

/// Settings dir/java browse: folder gains the `Blockfield` suffix, java is
/// stored verbatim; either marks the form edited like a manual edit.
pub fn apply_settings_browse(app: &AppWindow, field: &str, picked: Option<String>) {
    let Some(path) = picked else { return };
    let form = app.global::<SettingsForm>();
    let mut data = form.get_data();
    if field == "dir" {
        data.game_dir = selected_game_directory(&path).into();
    } else if field == "java" {
        data.java_path = path.into();
    } else {
        return;
    }
    form.set_data(data);
    form.invoke_edited();
}

/// FirstRun game-dir browse: folder gains the `Blockfield` suffix.
pub fn apply_firstrun_browse_dir(app: &AppWindow, picked: Option<String>) {
    if let Some(path) = picked {
        app.set_firstrun_game_dir(selected_game_directory(&path).into());
    }
}

/// FirstRun java browse: path stored verbatim.
pub fn apply_firstrun_browse_java(app: &AppWindow, picked: Option<String>) {
    if let Some(path) = picked {
        app.set_firstrun_java_path(path.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            i_slint_backend_testing::init_no_event_loop();
        });
    }

    #[test]
    fn game_directory_gains_blockfield_suffix() {
        assert_eq!(
            selected_game_directory("/home/player/Games"),
            "/home/player/Games/Blockfield"
        );
        assert_eq!(selected_game_directory("/"), "/Blockfield");
        assert_eq!(
            selected_game_directory("D:\\Games"),
            "D:\\Games\\Blockfield"
        );
        assert_eq!(selected_game_directory("D:\\"), "D:\\Blockfield");
        assert_eq!(
            selected_game_directory("\\\\server\\share\\Games"),
            "\\\\server\\share\\Games\\Blockfield"
        );
    }

    #[test]
    fn game_directory_keeps_existing_blockfield() {
        for parent in [
            "/games/Blockfield",
            "/games/BlockField/",
            "D:\\Games\\BLOCKFIELD",
            "D:\\Games\\Blockfield\\",
        ] {
            assert_eq!(selected_game_directory(parent), parent);
        }
    }

    #[test]
    fn browse_apply_updates_form_and_firstrun() {
        init();
        let app = AppWindow::new().expect("test window");
        crate::settings_tabs::wire(&app, None);
        apply_settings_browse(&app, "dir", Some("/home/player/Games".into()));
        assert_eq!(
            app.global::<SettingsForm>().get_data().game_dir,
            "/home/player/Games/Blockfield"
        );
        assert!(app.global::<SettingsForm>().get_dirty());
        apply_settings_browse(&app, "java", Some("/usr/bin/java".into()));
        assert_eq!(
            app.global::<SettingsForm>().get_data().java_path,
            "/usr/bin/java"
        );
        // Cancel and unknown fields leave the form untouched.
        let before = app.global::<SettingsForm>().get_data();
        apply_settings_browse(&app, "dir", None);
        apply_settings_browse(&app, "other", Some("x".into()));
        assert_eq!(
            app.global::<SettingsForm>().get_data().game_dir,
            before.game_dir
        );
        apply_firstrun_browse_dir(&app, Some("D:\\Games".into()));
        assert_eq!(app.get_firstrun_game_dir(), "D:\\Games\\Blockfield");
        apply_firstrun_browse_java(&app, Some("C:\\java.exe".into()));
        assert_eq!(app.get_firstrun_java_path(), "C:\\java.exe");
        apply_firstrun_browse_dir(&app, None);
        assert_eq!(app.get_firstrun_game_dir(), "D:\\Games\\Blockfield");
    }
}
