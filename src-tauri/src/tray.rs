use crate::app_state::AppState;
use crate::database::repositories::provider_repository::ProviderRepository;
use crate::database::repositories::target_repository::TargetRepository;
use crate::error::AppError;
use crate::models::provider::Provider;
use crate::models::provider_switch::ProviderSwitchRequest;
use crate::models::target_app::TargetApp;
use crate::models::tray::TrayMenuStatus;
use crate::services::provider_switch_service::ProviderSwitchService;
use tauri::menu::{Menu, MenuBuilder, SubmenuBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

const TRAY_ID: &str = "ai-switch-main-tray";
const OPEN_ITEM_ID: &str = "ai-switch:tray:open";
const REFRESH_ITEM_ID: &str = "ai-switch:tray:refresh";
const QUIT_ITEM_ID: &str = "ai-switch:tray:quit";
const NO_PROVIDERS_ITEM_ID: &str = "ai-switch:tray:no-providers";
const SWITCH_PREFIX: &str = "ai-switch:tray:switch";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayMenuAction {
    Open,
    Refresh,
    Quit,
    Switch {
        mode: String,
        target_app_id: String,
        provider_id: String,
    },
}

#[derive(Debug, Clone)]
struct TrayMenuData {
    providers: Vec<Provider>,
    targets: Vec<TargetApp>,
}

pub async fn setup_tray(app: &AppHandle) -> Result<TrayMenuStatus, AppError> {
    let data = load_tray_menu_data(app).await?;
    let status = tray_menu_status(&data.providers, &data.targets);
    let menu = build_tray_menu(app, &data)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("AI Switch")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref().to_string();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                handle_tray_menu_event(app, id).await;
            });
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app).map_err(|error| AppError::Adapter {
        code: "tray.setup",
        message: "Could not create tray icon".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    })?;

    Ok(status)
}

pub async fn refresh_tray_menu(app: &AppHandle) -> Result<TrayMenuStatus, AppError> {
    let data = load_tray_menu_data(app).await?;
    let status = tray_menu_status(&data.providers, &data.targets);
    let menu = build_tray_menu(app, &data)?;

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu))
            .map_err(|error| AppError::Adapter {
                code: "tray.menu_refresh",
                message: "Could not refresh tray menu".to_string(),
                details: Some(error.to_string()),
                recoverable: true,
            })?;
    }

    Ok(status)
}

pub fn switch_menu_id(mode: &str, target_app_id: &str, provider_id: &str) -> String {
    format!("{SWITCH_PREFIX}:{mode}:{target_app_id}:{provider_id}")
}

pub fn parse_tray_menu_action(id: &str) -> Option<TrayMenuAction> {
    match id {
        OPEN_ITEM_ID => Some(TrayMenuAction::Open),
        REFRESH_ITEM_ID => Some(TrayMenuAction::Refresh),
        QUIT_ITEM_ID => Some(TrayMenuAction::Quit),
        _ => parse_switch_action(id),
    }
}

fn parse_switch_action(id: &str) -> Option<TrayMenuAction> {
    let mut parts = id.splitn(6, ':');
    let namespace = parts.next()?;
    let tray = parts.next()?;
    let action = parts.next()?;
    let mode = parts.next()?;
    let target_app_id = parts.next()?;
    let provider_id = parts.next()?;

    if namespace != "ai-switch" || tray != "tray" || action != "switch" {
        return None;
    }

    if mode != "sandbox" && mode != "real" {
        return None;
    }

    if target_app_id.is_empty() || provider_id.is_empty() {
        return None;
    }

    Some(TrayMenuAction::Switch {
        mode: mode.to_string(),
        target_app_id: target_app_id.to_string(),
        provider_id: provider_id.to_string(),
    })
}

fn tray_menu_status(providers: &[Provider], targets: &[TargetApp]) -> TrayMenuStatus {
    let real_target_count = targets
        .iter()
        .filter(|target| is_real_tray_target(&target.key))
        .count();
    let switch_item_count = providers.len() * (targets.len() + real_target_count);

    TrayMenuStatus {
        provider_count: providers.len() as i64,
        target_count: targets.len() as i64,
        switch_item_count: switch_item_count as i64,
    }
}

async fn load_tray_menu_data(app: &AppHandle) -> Result<TrayMenuData, AppError> {
    let state = app.state::<AppState>();
    let targets = TargetRepository::ensure_defaults(&state.pool).await?;
    let providers = ProviderRepository::list(&state.pool).await?;

    Ok(TrayMenuData { providers, targets })
}

fn build_tray_menu(app: &AppHandle, data: &TrayMenuData) -> Result<Menu<tauri::Wry>, AppError> {
    let mut root = MenuBuilder::new(app)
        .text(OPEN_ITEM_ID, "Open AI Switch")
        .separator();

    if data.providers.is_empty() {
        root = root.text(NO_PROVIDERS_ITEM_ID, "No providers available");
    } else {
        let mut switch_menu = SubmenuBuilder::new(app, "Switch provider");
        for provider in &data.providers {
            let provider_menu = build_provider_switch_submenu(app, provider, &data.targets)?;
            switch_menu = switch_menu.item(&provider_menu);
        }
        let switch_menu = switch_menu.build().map_err(map_tray_error)?;
        root = root.item(&switch_menu);
    }

    root.separator()
        .text(REFRESH_ITEM_ID, "Refresh tray menu")
        .text(QUIT_ITEM_ID, "Quit")
        .build()
        .map_err(map_tray_error)
}

fn build_provider_switch_submenu(
    app: &AppHandle,
    provider: &Provider,
    targets: &[TargetApp],
) -> Result<tauri::menu::Submenu<tauri::Wry>, AppError> {
    let mut provider_menu = SubmenuBuilder::new(app, &provider.name);
    let mut sandbox_menu = SubmenuBuilder::new(app, "Sandbox");

    for target in targets {
        sandbox_menu = sandbox_menu.text(
            switch_menu_id("sandbox", &target.id, &provider.id),
            &target.display_name,
        );
    }

    let sandbox_menu = sandbox_menu.build().map_err(map_tray_error)?;
    provider_menu = provider_menu.item(&sandbox_menu);

    let real_targets: Vec<&TargetApp> = targets
        .iter()
        .filter(|target| is_real_tray_target(&target.key))
        .collect();
    if !real_targets.is_empty() {
        let mut real_menu = SubmenuBuilder::new(app, "Real config");
        for target in real_targets {
            real_menu = real_menu.text(
                switch_menu_id("real", &target.id, &provider.id),
                format!("{} config", target.display_name),
            );
        }
        let real_menu = real_menu.build().map_err(map_tray_error)?;
        provider_menu = provider_menu.separator().item(&real_menu);
    }

    provider_menu.build().map_err(map_tray_error)
}

async fn handle_tray_menu_event(app: AppHandle, id: String) {
    match parse_tray_menu_action(&id) {
        Some(TrayMenuAction::Open) => show_main_window(&app),
        Some(TrayMenuAction::Refresh) => {
            if let Err(error) = refresh_tray_menu(&app).await {
                eprintln!("AI Switch tray refresh failed: {error}");
            }
        }
        Some(TrayMenuAction::Quit) => app.exit(0),
        Some(TrayMenuAction::Switch {
            mode,
            target_app_id,
            provider_id,
        }) => {
            if let Err(error) =
                switch_provider_from_tray(&app, target_app_id, provider_id, mode).await
            {
                eprintln!("AI Switch tray switch failed: {error}");
            }
        }
        None => {}
    }
}

async fn switch_provider_from_tray(
    app: &AppHandle,
    target_app_id: String,
    provider_id: String,
    mode: String,
) -> Result<(), AppError> {
    let state = app.state::<AppState>();
    ProviderSwitchService::switch_provider(
        &state.pool,
        &state.paths,
        ProviderSwitchRequest {
            target_app_id,
            provider_id,
            mode,
        },
    )
    .await?;
    refresh_tray_menu(app).await?;
    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn is_real_tray_target(target_key: &str) -> bool {
    matches!(
        target_key,
        "claude_code" | "codex" | "gemini_cli" | "opencode"
    )
}

fn map_tray_error(error: tauri::Error) -> AppError {
    AppError::Adapter {
        code: "tray.menu_build",
        message: "Could not build tray menu".to_string(),
        details: Some(error.to_string()),
        recoverable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::provider::Provider;
    use crate::models::target_app::TargetApp;

    #[test]
    fn parses_switch_menu_id() {
        let id = switch_menu_id("real", "target-codex", "provider-1");

        let action = parse_tray_menu_action(&id).expect("action");

        assert_eq!(
            action,
            TrayMenuAction::Switch {
                mode: "real".to_string(),
                target_app_id: "target-codex".to_string(),
                provider_id: "provider-1".to_string(),
            }
        );
    }

    #[test]
    fn rejects_unknown_menu_ids() {
        assert!(parse_tray_menu_action("ai-switch:tray:switch:delete:t:p").is_none());
        assert!(parse_tray_menu_action("other").is_none());
    }

    #[test]
    fn counts_sandbox_and_real_switch_items() {
        let providers = vec![provider("provider-1"), provider("provider-2")];
        let targets = vec![
            target("target-codex", "codex"),
            target("target-gemini", "gemini_cli"),
            target("target-opencode", "opencode"),
            target("target-claude", "claude_code"),
        ];

        let status = tray_menu_status(&providers, &targets);

        assert_eq!(status.provider_count, 2);
        assert_eq!(status.target_count, 4);
        assert_eq!(status.switch_item_count, 16);
    }

    fn provider(id: &str) -> Provider {
        Provider {
            id: id.to_string(),
            name: id.to_string(),
            kind: "openai_compatible".to_string(),
            base_url: Some("https://api.example.com/v1".to_string()),
            model_config_json: "{}".to_string(),
            target_options_json: "{}".to_string(),
            secret_ref: None,
            status: "ok".to_string(),
            sort_order: 0,
            created_at: "2026-07-13T00:00:00Z".to_string(),
            updated_at: "2026-07-13T00:00:00Z".to_string(),
        }
    }

    fn target(id: &str, key: &str) -> TargetApp {
        TargetApp {
            id: id.to_string(),
            key: key.to_string(),
            display_name: key.to_string(),
            enabled: 1,
            sort_order: 0,
            created_at: "2026-07-13T00:00:00Z".to_string(),
            updated_at: "2026-07-13T00:00:00Z".to_string(),
        }
    }
}
