//! Remote CMS `content.json` loader and Tauri-parity fallback defaults.
//! Mirrors `src/lib/content.ts`, `src/lib/api-base.ts`, and `src/app/constants.ts`.
use std::{
    sync::{OnceLock, RwLock},
    time::Duration,
};

use serde_json::Value;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use tokio::runtime::Handle;

use crate::{AppWindow, MainFeature};

pub const LANG: &str = "ru";

const DEFAULT_CHROME_TITLE: &str = "BLOCKFIELD LAUNCHER";
const DEFAULT_BRAND_SUBTITLE: &str = "КОМАНДНЫЕ БОИ";
const DEFAULT_SUPPORT_LABEL: &str = "ПОДДЕРЖКА";
const DEFAULT_NETWORK_LABEL: &str = "АККАУНТ BLOCKFIELD";
const DEFAULT_SEASON: &str = "Minecraft 1.21.1 · Fabric";
const DEFAULT_DESCRIPTION: &str = "Командные бои с захватом точек. Выбирайте доступный класс и снаряжение, занимайте позиции и сохраняйте билеты своей команды.";
const DEFAULT_SERVER_NAME: &str = "BLOCKFIELD · ОСНОВНОЙ";
const DEFAULT_UPDATE_DESCRIPTION: &str =
    "Лаунчер скачает обновления и проверит файлы игры. Дождитесь завершения перед запуском.";
const DEFAULT_SETTINGS_PREFERENCES: &str = "/ ПАРАМЕТРЫ ЛАУНЧЕРА";

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedContent {
    pub chrome_title: String,
    pub brand: String,
    pub brand_subtitle: String,
    pub support_label: String,
    pub server_ip: String,
    pub operation_name: String,
    pub season: String,
    pub description: String,
    pub server_name: String,
    pub update_description: String,
    pub settings_preferences: String,
    pub features: Vec<MainFeature>,
}

fn cache() -> &'static RwLock<Option<ResolvedContent>> {
    static CACHE: OnceLock<RwLock<Option<ResolvedContent>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(None))
}

pub fn reset_cache() {
    *cache().write().unwrap_or_else(|e| e.into_inner()) = None;
}

pub fn cached() -> Option<ResolvedContent> {
    cache().read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn default_server_ip() -> String {
    std::env::var("VITE_SERVER_IP")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            option_env!("VITE_SERVER_IP")
                .filter(|v| !v.trim().is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "play.blockfield.pro".to_owned())
}

pub fn default_brand() -> String {
    std::env::var("VITE_BRAND")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            option_env!("VITE_BRAND")
                .filter(|v| !v.trim().is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "BLOCKFIELD".to_owned())
}

pub fn launcher_version_label() -> String {
    format!("ЛАУНЧЕР v{}", env!("CARGO_PKG_VERSION"))
}

/// `contentText` (`src/lib/content.ts:26-31`): first non-whitespace string at `keys`.
pub fn content_text<'a>(content: Option<&'a Value>, keys: &[&str]) -> Option<&'a str> {
    let obj = content?.as_object()?;
    for key in keys {
        if let Some(value) = obj.get(*key).and_then(Value::as_str)
            && !value.trim().is_empty()
        {
            return Some(value);
        }
    }
    None
}

/// `localizedContentText` (`src/lib/content.ts:44-54`): `translations[lang]` -> `translations.en` -> top-level keys.
pub fn localized_content_text<'a>(
    content: Option<&'a Value>,
    lang: &str,
    translation_key: &str,
    keys: &[&str],
) -> Option<&'a str> {
    if let Some(translations) = content.and_then(|c| c.get("translations")) {
        for locale in [lang, "en"] {
            if let Some(value) = translations
                .get(locale)
                .and_then(|table| table.get(translation_key))
                .and_then(Value::as_str)
                && !value.trim().is_empty()
            {
                return Some(value);
            }
        }
    }
    content_text(content, keys)
}

pub fn default_features() -> Vec<MainFeature> {
    vec![
        MainFeature {
            icon: "flag".into(),
            title: "ЗАХВАТ ТОЧЕК".into(),
            description: "Сражайтесь за контроль точек и сохраняйте билеты своей команды.".into(),
        },
        MainFeature {
            icon: "swords".into(),
            title: "КЛАССЫ".into(),
            description: "Штурмовик и марксман. Доступность инженера, снайпера и оператора FPV зависит от настроек и числа игроков.".into(),
        },
        MainFeature {
            icon: "crosshair".into(),
            title: "СНАРЯЖЕНИЕ".into(),
            description: "Выбирайте комплект оружия из вариантов, доступных вашему классу.".into(),
        },
        MainFeature {
            icon: "gamepad-2".into(),
            title: "ИГРОВОЕ МЕНЮ".into(),
            description: "Выбирайте режим, класс и точку высадки через планшет в игре.".into(),
        },
    ]
}

pub fn resolve_features(content: Option<&Value>, lang: &str) -> Vec<MainFeature> {
    let Some(items) = content
        .and_then(|c| c.get("features"))
        .and_then(Value::as_array)
    else {
        return default_features();
    };
    let mut resolved = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let Some(raw_title) = item
            .get("title")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let Some(raw_desc) = item
            .get("desc")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let title_key = format!("feature.{index}.title");
        let desc_key = format!("feature.{index}.desc");
        let title = localized_content_text(content, lang, &title_key, &[]).unwrap_or(raw_title);
        let desc = localized_content_text(content, lang, &desc_key, &[]).unwrap_or(raw_desc);
        let icon = match item.get("icon").and_then(Value::as_str) {
            Some("swords") => "swords",
            Some("truck") => "truck",
            Some("crosshair") => "crosshair",
            _ => "flag",
        };
        resolved.push(MainFeature {
            icon: icon.into(),
            title: title.into(),
            description: desc.into(),
        });
    }
    if resolved.is_empty() {
        default_features()
    } else {
        resolved
    }
}

pub fn resolve(content: Option<&Value>, lang: &str) -> ResolvedContent {
    let brand_fallback = default_brand();
    let server_ip_fallback = default_server_ip();
    ResolvedContent {
        chrome_title: localized_content_text(
            content,
            lang,
            "content.chromeTitle",
            &["chromeTitle", "chrome_title"],
        )
        .unwrap_or(DEFAULT_CHROME_TITLE)
        .to_owned(),
        brand: content_text(content, &["brand"])
            .unwrap_or(&brand_fallback)
            .to_owned(),
        brand_subtitle: localized_content_text(
            content,
            lang,
            "content.brandSubtitle",
            &["brandSubtitle", "brand_subtitle"],
        )
        .unwrap_or(DEFAULT_BRAND_SUBTITLE)
        .to_owned(),
        support_label: localized_content_text(
            content,
            lang,
            "content.supportLabel",
            &["supportLabel", "support_label"],
        )
        .unwrap_or(DEFAULT_SUPPORT_LABEL)
        .to_owned(),
        server_ip: content_text(content, &["serverIp", "server_ip"])
            .unwrap_or(&server_ip_fallback)
            .to_owned(),
        operation_name: localized_content_text(
            content,
            lang,
            "content.operationName",
            &["operationName", "operation_name"],
        )
        .unwrap_or(&brand_fallback)
        .to_owned(),
        season: localized_content_text(content, lang, "content.season", &["season"])
            .unwrap_or(DEFAULT_SEASON)
            .to_owned(),
        description: localized_content_text(content, lang, "content.description", &["description"])
            .unwrap_or(DEFAULT_DESCRIPTION)
            .to_owned(),
        server_name: localized_content_text(
            content,
            lang,
            "content.serverName",
            &["serverName", "server_name"],
        )
        .unwrap_or(DEFAULT_SERVER_NAME)
        .to_owned(),
        update_description: localized_content_text(
            content,
            lang,
            "content.updateDescription",
            &["updateDescription", "update_description"],
        )
        .unwrap_or(DEFAULT_UPDATE_DESCRIPTION)
        .to_owned(),
        settings_preferences: localized_content_text(
            content,
            lang,
            "content.settingsPreferences",
            &["settingsPreferences", "settings_preferences"],
        )
        .unwrap_or(DEFAULT_SETTINGS_PREFERENCES)
        .to_owned(),
        features: resolve_features(content, lang),
    }
}

/// Apply Tauri-identical non-fixture defaults for real `Backend::Core` runs.
pub fn apply_shell_defaults(app: &AppWindow) {
    app.set_username("—".into());
    app.set_role("ИГРОК".into());
    app.set_launcher_label(launcher_version_label().into());
    app.set_network_label(DEFAULT_NETWORK_LABEL.into());
    app.set_update_data(crate::update::default_data());
    app.set_launcher_update_data(crate::update::default_launcher_data());
    app.set_nav_labels(ModelRc::new(VecModel::from(
        ["ИГРАТЬ", "ОБНОВЛЕНИЯ", "ТОП", "НАСТРОЙКИ"]
            .map(SharedString::from)
            .to_vec(),
    )));
    apply_cached_or_default(app);
}

pub fn apply_cached_or_default(app: &AppWindow) {
    let resolved = cached().unwrap_or_else(|| resolve(None, LANG));
    apply_resolved(app, &resolved);
}

pub fn apply_resolved(app: &AppWindow, resolved: &ResolvedContent) {
    app.set_chrome_title(resolved.chrome_title.as_str().into());
    app.set_brand(resolved.brand.as_str().into());
    app.set_subtitle(resolved.brand_subtitle.as_str().into());
    app.set_support_label(resolved.support_label.as_str().into());
    app.set_settings_subtitle(resolved.settings_preferences.as_str().into());
    let mut main = app.get_main_data();
    main.operation_name = resolved.operation_name.as_str().into();
    main.season = resolved.season.as_str().into();
    main.description = resolved.description.as_str().into();
    main.server_name = resolved.server_name.as_str().into();
    main.server_ip_fallback = resolved.server_ip.as_str().into();
    app.set_main_data(main);
    app.set_main_features(ModelRc::new(VecModel::from(resolved.features.clone())));
    let mut update = app.get_update_data();
    update.operation = resolved.operation_name.as_str().into();
    update.description = resolved.update_description.as_str().into();
    app.set_update_data(update);
    crate::backend::sync_footer_server_label(app);
}

/// Base URL matching `src/lib/api-base.ts`: `VITE_BLOCKFIELD_PACK_URL`, or `https://blockfield.pro` in debug builds.
pub fn content_base_url() -> Option<String> {
    launcher_app::ops::pack_check::pack_base_url()
        .ok()
        .or_else(|| cfg!(debug_assertions).then(|| "https://blockfield.pro".to_owned()))
}

pub async fn fetch_from_base(base: &str) -> Option<Value> {
    let url = format!("{}/content.json", base.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<Value>().await.ok()
}

/// Fetch remote `content.json` once at startup and overlay all UI strings.
pub fn spawn_fetch(weak: Weak<AppWindow>, handle: &Handle) {
    let Some(base) = content_base_url() else {
        return;
    };
    handle.spawn(async move {
        let Some(raw) = fetch_from_base(&base).await else {
            return;
        };
        let resolved = resolve(Some(&raw), LANG);
        *cache().write().unwrap_or_else(|e| e.into_inner()) = Some(resolved.clone());
        let _ = weak.upgrade_in_event_loop(move |app| {
            apply_resolved(&app, &resolved);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn localized_content_text_prefers_lang_then_en_then_top_level() {
        let content = json!({
            "operation_name": "TOP_OP",
            "description": "   ",
            "serverName": "TOP_SERVER",
            "translations": {
                "en": {
                    "content.operationName": "EN_OP",
                    "content.serverName": "EN_SERVER",
                    "content.description": "   "
                },
                "ru": {
                    "content.operationName": "RU_OP",
                    "content.serverName": " "
                }
            }
        });
        assert_eq!(
            localized_content_text(
                Some(&content),
                "ru",
                "content.operationName",
                &["operationName", "operation_name"]
            ),
            Some("RU_OP")
        );
        // Blank ru value falls back to en before top-level keys.
        assert_eq!(
            localized_content_text(
                Some(&content),
                "ru",
                "content.serverName",
                &["serverName", "server_name"]
            ),
            Some("EN_SERVER")
        );
        // Blank everywhere returns None.
        assert_eq!(
            localized_content_text(
                Some(&content),
                "ru",
                "content.description",
                &["description"]
            ),
            None
        );
    }

    #[test]
    fn resolve_applies_live_cms_fields_and_features() {
        let live = json!({
            "brand": "BLOCKFIELD",
            "serverName": "BLOCKFIELD",
            "serverIp": "play.blockfield.pro:25565",
            "updateDescription": "Лаунчер проверяет файлы сборки и скачивает необходимые обновления.",
            "features": [
                { "icon": "flag", "title": "ЗАХВАТ ТОЧЕК", "desc": "Desc 1" },
                { "icon": "unknown", "title": "BAD_ICON", "desc": "Desc 2" },
                { "icon": "swords", "title": "", "desc": "Skipped empty title" },
                { "icon": "flag", "title": "ИГРОВОЕ МЕНЮ", "desc": "Выбирайте комнату, класс и точку появления через меню в игре." }
            ],
            "translations": {
                "ru": {
                    "feature.3.title": "МЕНЮ В ИГРЕ"
                }
            }
        });
        let resolved = resolve(Some(&live), "ru");
        assert_eq!(resolved.server_name, "BLOCKFIELD");
        assert_eq!(resolved.server_ip, "play.blockfield.pro:25565");
        assert_eq!(
            resolved.update_description,
            "Лаунчер проверяет файлы сборки и скачивает необходимые обновления."
        );
        assert_eq!(resolved.features.len(), 3);
        assert_eq!(resolved.features[1].icon, "flag");
        assert_eq!(resolved.features[2].title, "МЕНЮ В ИГРЕ");
        assert_eq!(
            resolved.features[2].description,
            "Выбирайте комнату, класс и точку появления через меню в игре."
        );
    }

    #[test]
    fn resolve_empty_features_falls_back_to_four_defaults() {
        let resolved = resolve(Some(&json!({"features": []})), "ru");
        assert_eq!(resolved.features.len(), 4);
        assert_eq!(resolved.features[3].icon, "gamepad-2");
    }
}
