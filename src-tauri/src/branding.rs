//! Брендинг окна игры в доке/таскбаре: иконка и имя «Blockfield».
//!
//! Деление ответственности с модом (WindowBranding, APP_ID `blockfield`):
//! - Linux: мод ставит Wayland app_id и X11 WM_CLASS хинтами до создания окна,
//!   лаунчер кладёт рядом NoDisplay-.desktop с тем же StartupWMClass и иконки
//!   hicolor, чтобы док сопоставил окно с иконкой и именем;
//! - macOS: лаунчер дописывает -Xdock:name/-Xdock:icon после ванильных JVM-аргументов;
//! - Windows: мод задаёт AppUserModelID процесса, лаунчер пробрасывает его значением.

use std::path::{Path, PathBuf};

/// Один id для дока/таскбара: совпадает с WindowBranding.APP_ID в моде.
pub const APP_ID: &str = "blockfield";
pub const APP_NAME: &str = "Blockfield";
/// Совпадает с WindowBranding.APP_USER_MODEL_ID в моде. Резервирует имя группы окна на
/// Windows через -Dblockfield.aumid: нативного шима пока нет (см. комментарий у константы
/// в моде), поэтому сегодня реально брендируют иконка и заголовок, а имя группы остаётся
/// за javaw.exe.
pub const WINDOWS_AUMID: &str = "com.blockfield.Blockfield";

pub const DESKTOP_FILE_NAME: &str = "blockfield.desktop";
pub const ICON_NAME: &str = "blockfield";
pub const MACOS_ICON_FILE: &str = "blockfield-icon.png";

/// JVM-аргументы брендинга для `os` (`std::env::consts::OS`): пусто везде, кроме macOS/Windows.
pub fn game_jvm_args(os: &str, game_dir: &Path) -> Vec<String> {
    match os {
        "macos" => vec![
            format!("-Xdock:name={APP_NAME}"),
            format!("-Xdock:icon={}", game_dir.join(MACOS_ICON_FILE).display()),
        ],
        "windows" => vec![format!("-Dblockfield.aumid={WINDOWS_AUMID}")],
        _ => Vec::new(),
    }
}

/// Точка входа из launch_game: на Linux ставит .desktop+иконки, на macOS — иконку для -Xdock.
pub fn ensure_game_branding(_game_dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    ensure_macos_icon(_game_dir)?;
    #[cfg(target_os = "linux")]
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        ensure_linux_branding_at(&home)?;
    }
    Ok(())
}

/// Убирает файлы ensure_linux_branding_at. В uninstall лаунчера (сейчас только Windows)
/// Linux-ветки нет, поэтому на Linux файлы остаются до ручного удаления — честно сказано в PR.
pub fn remove_linux_branding_at(home: &Path) -> Result<(), String> {
    let _ = std::fs::remove_file(desktop_path(home));
    for size in icon_sizes() {
        let _ = std::fs::remove_file(icon_path(home, size));
    }
    Ok(())
}

pub fn desktop_entry() -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name={APP_NAME}\n\
         Icon={ICON_NAME}\n\
         NoDisplay=true\n\
         StartupWMClass={APP_ID}\n"
    )
}

fn icon_sizes() -> [u32; 3] {
    [32, 128, 256]
}

fn icon_bytes(size: u32) -> &'static [u8] {
    match size {
        32 => include_bytes!("../icons/32x32.png"),
        128 => include_bytes!("../icons/128x128.png"),
        _ => include_bytes!("../icons/256x256.png"),
    }
}

fn desktop_path(home: &Path) -> PathBuf {
    home.join(".local/share/applications")
        .join(DESKTOP_FILE_NAME)
}

fn icon_path(home: &Path, size: u32) -> PathBuf {
    home.join(format!(
        ".local/share/icons/hicolor/{size}x{size}/apps/{ICON_NAME}.png"
    ))
}

/// Идемпотентно: пишет только изменившееся, возвращает true если что-то записал.
pub fn ensure_linux_branding_at(home: &Path) -> Result<bool, String> {
    let mut changed = false;
    changed |= write_if_different(&desktop_path(home), desktop_entry().as_bytes())?;
    for size in icon_sizes() {
        changed |= write_if_different(&icon_path(home, size), icon_bytes(size))?;
    }
    Ok(changed)
}

#[cfg(target_os = "macos")]
fn ensure_macos_icon(game_dir: &Path) -> Result<(), String> {
    ensure_macos_icon_at(game_dir, include_bytes!("../icons/256x256.png"))?;
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn ensure_macos_icon_at(game_dir: &Path, icon: &[u8]) -> Result<bool, String> {
    write_if_different(&game_dir.join(MACOS_ICON_FILE), icon)
}

fn write_if_different(path: &Path, data: &[u8]) -> Result<bool, String> {
    if let Ok(existing) = std::fs::read(path) {
        if existing == data {
            return Ok(false);
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
    }
    std::fs::write(path, data).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jvm_args_brand_macos_and_windows_only() {
        let dir = Path::new("/games/blockfield");
        let macos = game_jvm_args("macos", dir);
        assert_eq!(
            macos,
            vec![
                "-Xdock:name=Blockfield".to_string(),
                "-Xdock:icon=/games/blockfield/blockfield-icon.png".to_string(),
            ]
        );
        assert_eq!(
            game_jvm_args("windows", dir),
            vec!["-Dblockfield.aumid=com.blockfield.Blockfield".to_string()]
        );
        assert!(game_jvm_args("linux", dir).is_empty());
    }

    #[test]
    fn desktop_entry_maps_window_to_blockfield() {
        let entry = desktop_entry();
        assert!(entry.contains("\nName=Blockfield\n"));
        assert!(entry.contains("\nStartupWMClass=blockfield\n"));
        assert!(entry.contains("\nIcon=blockfield\n"));
        assert!(entry.contains("\nNoDisplay=true\n"));
    }

    #[test]
    fn linux_branding_is_idempotent_and_removable() {
        let home = tempfile::tempdir().unwrap();
        assert!(ensure_linux_branding_at(home.path()).unwrap());
        assert!(!ensure_linux_branding_at(home.path()).unwrap());
        assert!(desktop_path(home.path()).is_file());
        for size in icon_sizes() {
            assert!(icon_path(home.path(), size).is_file(), "{size}");
        }
        // Чужой desktop с тем же именем перезаписывается нашим.
        std::fs::write(desktop_path(home.path()), b"[Desktop Entry]\n").unwrap();
        assert!(ensure_linux_branding_at(home.path()).unwrap());
        remove_linux_branding_at(home.path()).unwrap();
        assert!(!desktop_path(home.path()).exists());
        for size in icon_sizes() {
            assert!(!icon_path(home.path(), size).exists(), "{size}");
        }
        // Повторное удаление — не ошибка.
        remove_linux_branding_at(home.path()).unwrap();
    }

    #[test]
    fn macos_icon_write_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let icon = include_bytes!("../icons/256x256.png");
        assert!(ensure_macos_icon_at(dir.path(), icon).unwrap());
        assert!(!ensure_macos_icon_at(dir.path(), icon).unwrap());
        assert_eq!(
            std::fs::read(dir.path().join(MACOS_ICON_FILE)).unwrap(),
            icon
        );
    }
}
