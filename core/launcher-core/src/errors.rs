//! Friendly error mapping, ported 1:1 from `src/lib/errors.ts:1-141`.
//! Russian strings are byte-identical to the TS; proven by `tests/parity.rs`.

/// Where the failed call came from; picks the fallback/network message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorContext {
    ModpackCheck,
    ModpackDownload,
    Launch,
    LauncherUpdate,
    SettingsSave,
    SettingsLoad,
    SkinUpload,
    Account,
    SelectProfile,
}

impl ErrorContext {
    /// Parse the TS `ErrorContext` string union member.
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "modpack-check" => Some(Self::ModpackCheck),
            "modpack-download" => Some(Self::ModpackDownload),
            "launch" => Some(Self::Launch),
            "launcher-update" => Some(Self::LauncherUpdate),
            "settings-save" => Some(Self::SettingsSave),
            "settings-load" => Some(Self::SettingsLoad),
            "skin-upload" => Some(Self::SkinUpload),
            "account" => Some(Self::Account),
            "select-profile" => Some(Self::SelectProfile),
            _ => None,
        }
    }

    fn fallback(self) -> &'static str {
        match self {
            Self::ModpackCheck => "Не удалось проверить обновление модпака.",
            Self::ModpackDownload => "Не удалось скачать обновление модпака.",
            Self::Launch => "Не удалось запустить игру.",
            Self::LauncherUpdate => "Не удалось проверить или установить обновление лаунчера.",
            Self::SettingsSave => "Не удалось сохранить настройки. Откройте подробности ошибки.",
            Self::SettingsLoad => {
                "Не удалось загрузить настройки. Проверьте доступ к папке настроек и повторите."
            }
            Self::SkinUpload => "Не удалось загрузить скин. Откройте подробности ошибки.",
            Self::Account => "Не удалось выполнить запрос к серверу аккаунтов.",
            Self::SelectProfile => "Не удалось переключить профиль.",
        }
    }

    fn network(self) -> &'static str {
        match self {
            Self::ModpackCheck | Self::ModpackDownload | Self::Launch => {
                "Нет доступа к серверу сборки blockfield.pro — проверьте интернет и повторите."
            }
            Self::LauncherUpdate => {
                "Нет доступа к серверу обновлений лаунчера — проверьте интернет и повторите."
            }
            Self::SettingsSave => "Не удалось сохранить настройки. Откройте подробности ошибки.",
            Self::SettingsLoad => {
                "Не удалось загрузить настройки. Проверьте доступ к папке настроек и повторите."
            }
            Self::SkinUpload => {
                "Нет доступа к серверу скинов skins.blockfield.pro — проверьте интернет и повторите."
            }
            Self::Account => {
                "Нет доступа к серверу аккаунтов skins.blockfield.pro — проверьте интернет и повторите."
            }
            Self::SelectProfile => "Не удалось переключить профиль. Проверьте интернет и повторите.",
        }
    }
}

/// Re-login marker passed through untouched by `friendlyError`.
pub const SESSION_EXPIRED: &str = "SESSION_EXPIRED";

const DRASL_ERRORS: [(&str, &str); 4] = [
    ("Invalid credentials", "Неверный никнейм или пароль."),
    ("That username is taken", "Этот никнейм уже занят."),
    ("That player name is taken", "Этот никнейм уже занят."),
    (
        "Invalid password",
        "Пароль слишком короткий — нужно не меньше 8 символов.",
    ),
];

// Substrings of reqwest/OS failures surfaced through Rust `format!("...: {e}")`.
const NETWORK_HINTS: [&str; 7] = [
    "error trying to connect",
    "error sending request",
    "dns error",
    "connection refused",
    "timed out",
    "network is unreachable",
    "tcp connect",
];

fn looks_like_network_error(raw: &str) -> bool {
    let lower = raw.to_lowercase();
    NETWORK_HINTS.iter().any(|hint| lower.contains(hint))
}

const PROFILE_ERRORS: [&str; 10] = [
    "Укажите адрес мастерской в настройках выбранного профиля.",
    "Мастерской нужен отдельный адрес: смена порта игрового адреса не меняет профиль.",
    "Для игры и мастерской нужны отдельные папки, без вложения одной в другую.",
    "Нужен адрес сервера: хост и необязательный порт, без ссылки или пробелов.",
    "Закройте игру перед сменой профиля.",
    "Закройте игру перед изменением настроек.",
    "Дождитесь завершения текущей операции.",
    "Профиль изменился. Откройте настройки заново.",
    "Профиль изменился. Повторите проверку файлов.",
    "Профиль изменён. Повторите проверку.",
];

const SETTINGS_ERRORS: [(&str, &str); 7] = [
    (
        "Game directory must be a safe absolute path",
        "Выберите отдельную папку для игры, например D:\\Blockfield, а не корень диска.",
    ),
    (
        "Game directory cannot be created:",
        "Не удалось создать папку игры. Выберите другую папку или проверьте доступ к выбранному пути.",
    ),
    (
        "Game directory is not writable:",
        "Не удалось записать файл в папку игры. Выберите другую папку или проверьте права на запись.",
    ),
    (
        "Selected Java executable does not exist",
        "Выбранный файл Java не найден. Очистите поле для автоматического выбора или укажите существующий файл.",
    ),
    (
        "Failed to run selected Java:",
        "Не удалось запустить выбранную Java. Очистите поле для автоматического выбора или выберите другую Java.",
    ),
    (
        "Selected Java must be a working Java 17 or 21 runtime",
        "Для игры нужна Java 17 или 21. Очистите поле для автоматического выбора или выберите подходящую Java.",
    ),
    (
        "RAM must be between",
        "Выбранный объём RAM недопустим. Уменьшите его с учётом памяти компьютера; минимум — 2 ГБ.",
    ),
];

/// Short user-facing message plus the raw text for a details view.
pub struct FriendlyError {
    pub message: String,
    pub raw: String,
}

/// `friendlyError(raw, context)`; `raw` is the already-stringified error.
pub fn friendly_error(raw: &str, context: ErrorContext) -> FriendlyError {
    let message = if let Some(hit) = PROFILE_ERRORS.iter().find(|m| raw.contains(*m)) {
        (*hit).to_owned()
    } else if raw.contains(SESSION_EXPIRED) {
        "Сессия истекла — войдите снова.".to_owned()
    } else if raw.contains("Drasl account server unavailable") {
        "Сервер аккаунтов недоступен, попробуйте позже.".to_owned()
    } else if context == ErrorContext::Account && raw.contains("Drasl: ") {
        // `String.match(/Drasl: (.*)/)`: first hit, up to end of line.
        let rest = &raw[raw.find("Drasl: ").unwrap() + "Drasl: ".len()..];
        let drasl = rest.split(['\n', '\r']).next().unwrap_or(rest);
        match DRASL_ERRORS
            .iter()
            .find(|(prefix, _)| drasl.starts_with(prefix))
        {
            Some((_, message)) => (*message).to_owned(),
            None => drasl.to_owned(),
        }
    } else if context == ErrorContext::SettingsSave {
        match SETTINGS_ERRORS
            .iter()
            .find(|(prefix, _)| raw.starts_with(prefix))
        {
            Some((_, message)) => (*message).to_owned(),
            None if looks_like_network_error(raw) => context.network().to_owned(),
            None => context.fallback().to_owned(),
        }
    } else if looks_like_network_error(raw) {
        context.network().to_owned()
    } else {
        context.fallback().to_owned()
    };
    FriendlyError {
        message,
        raw: raw.to_owned(),
    }
}
