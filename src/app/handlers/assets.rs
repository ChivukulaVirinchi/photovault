use iced::Task;

use crate::components::toast::Toast;

use super::super::messages::Message;
use super::super::state::PhotoVault;
use super::toasts;

pub(crate) fn install_asset_pack(app: &mut PhotoVault) -> Task<Message> {
    if app.asset_install_in_progress {
        return Task::none();
    }

    app.asset_install_in_progress = true;
    app.asset_install_error = None;

    Task::perform(
        async { crate::bootstrap::install_asset_pack().await },
        Message::AssetPackInstalled,
    )
}

pub(crate) fn asset_pack_installed(
    app: &mut PhotoVault,
    result: Result<String, String>,
) -> Task<Message> {
    app.asset_install_in_progress = false;
    app.asset_health = crate::bootstrap::asset_health();
    app.ml_available =
        !app.asset_health.missing_face_models && !app.asset_health.missing_onnx_runtime;

    match result {
        Ok(msg) => {
            app.asset_install_error = None;
            app.show_asset_install_prompt = app.asset_health.missing_any();
            toasts::show(app, Toast::success(msg))
        }
        Err(err) => {
            app.asset_install_error = Some(err.clone());
            app.show_asset_install_prompt = true;
            toasts::show(app, Toast::error("Failed to install optional assets", err))
        }
    }
}

pub(crate) fn dismiss_asset_install_prompt(app: &mut PhotoVault) -> Task<Message> {
    app.show_asset_install_prompt = false;
    Task::none()
}
