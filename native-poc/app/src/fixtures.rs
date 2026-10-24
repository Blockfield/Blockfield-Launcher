use slint::{ModelRc, SharedString, VecModel};

use crate::AppWindow;

pub fn apply(app: &AppWindow) {
    app.set_chrome_title("BLOCKFIELD LAUNCHER".into());
    app.set_brand("BLOCKFIELD".into());
    app.set_subtitle("КОМАНДНЫЕ БОИ".into());
    app.set_username("—".into());
    app.set_role("ИГРОК".into());
    app.set_launcher_label("ЛАУНЧЕР v1.0.19".into());
    app.set_server_label("IP · play.blockfield.pro".into());
    app.set_support_label("ПОДДЕРЖКА".into());
    app.set_network_label("АККАУНТ BLOCKFIELD".into());
    app.set_nav_labels(ModelRc::new(VecModel::from(
        ["ИГРАТЬ", "ОБНОВЛЕНИЯ", "ТОП", "НАСТРОЙКИ"]
            .map(SharedString::from)
            .to_vec(),
    )));
    app.set_dev_build(cfg!(debug_assertions));
}
