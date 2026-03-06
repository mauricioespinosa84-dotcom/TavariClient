use crate::models::StartupProgressEvent;
use crate::storage::load_settings;
use reqwest::Url;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;
use tokio::time::sleep;

const SPLASH_WINDOW_LABEL: &str = "splashscreen";
const MAIN_WINDOW_LABEL: &str = "main";

#[derive(Default)]
pub struct StartupState {
    started: Mutex<bool>,
}

fn emit_startup_progress(
    app: &AppHandle,
    status: impl Into<String>,
    stage: impl Into<String>,
    detail: impl Into<String>,
    progress: Option<f64>,
    version: Option<String>,
    indeterminate: bool,
) {
    if let Some(window) = app.get_webview_window(SPLASH_WINDOW_LABEL) {
        let _ = window.emit(
            "startup-progress",
            StartupProgressEvent {
                status: status.into(),
                stage: stage.into(),
                detail: detail.into(),
                progress,
                version,
                indeterminate,
            },
        );
    }
}

fn show_main_window(app: &AppHandle) -> Result<(), String> {
    let main_window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "No se encontro la ventana principal.".to_string())?;

    main_window.show().map_err(|error| error.to_string())?;
    let _ = main_window.set_focus();
    Ok(())
}

fn close_splash_window(app: &AppHandle) {
    if let Some(splash_window) = app.get_webview_window(SPLASH_WINDOW_LABEL) {
        let _ = splash_window.close();
    }
}

fn read_updater_config(app: &AppHandle) -> Result<Option<(Url, String)>, String> {
    let settings = load_settings(app)?;
    let Some(endpoint) = settings.updater_endpoint else {
        return Ok(None);
    };
    let Some(public_key) = settings.updater_public_key else {
        return Ok(None);
    };

    let endpoint = Url::parse(&endpoint)
        .map_err(|error| format!("La URL del actualizador no es valida: {error}"))?;

    Ok(Some((endpoint, public_key)))
}

async fn finish_startup(app: &AppHandle) -> Result<(), String> {
    emit_startup_progress(
        app,
        "ready",
        "Listo",
        "Abriendo Tavari Client.",
        Some(1.0),
        None,
        false,
    );
    sleep(Duration::from_millis(520)).await;
    show_main_window(app)?;
    close_splash_window(app);
    Ok(())
}

async fn check_for_updates(app: &AppHandle) -> Result<bool, String> {
    if cfg!(debug_assertions) {
        emit_startup_progress(
            app,
            "loading",
            "Modo desarrollo",
            "Omitiendo auto update en desarrollo.",
            Some(0.26),
            None,
            false,
        );
        sleep(Duration::from_millis(420)).await;
        return Ok(false);
    }

    let Some((endpoint, public_key)) = read_updater_config(app)? else {
        emit_startup_progress(
            app,
            "loading",
            "Sin update remoto",
            "Cargando Tavari Client.",
            Some(0.26),
            None,
            false,
        );
        sleep(Duration::from_millis(320)).await;
        return Ok(false);
    };

    emit_startup_progress(
        app,
        "checking",
        "Buscando update",
        "Comprobando si existe una nueva version.",
        Some(0.12),
        None,
        true,
    );

    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|error| error.to_string())?
        .pubkey(public_key)
        .build()
        .map_err(|error| error.to_string())?;

    let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
        emit_startup_progress(
            app,
            "loading",
            "Cliente al dia",
            "No hay actualizaciones pendientes.",
            Some(0.42),
            None,
            false,
        );
        sleep(Duration::from_millis(360)).await;
        return Ok(false);
    };

    let announced_version = update.version.clone();
    let downloaded = AtomicU64::new(0);

    emit_startup_progress(
        app,
        "downloading",
        "Actualizando Tavari Client",
        format!("Descargando la version {announced_version}."),
        Some(0.0),
        Some(announced_version.clone()),
        false,
    );

    update
        .download_and_install(
            |chunk_length, content_length| {
                let downloaded_now =
                    downloaded.fetch_add(chunk_length as u64, Ordering::Relaxed) + chunk_length as u64;
                let progress = content_length
                    .filter(|total| *total > 0)
                    .map(|total| (downloaded_now as f64 / total as f64).clamp(0.0, 1.0))
                    .or(Some(0.0));

                emit_startup_progress(
                    app,
                    "downloading",
                    "Actualizando Tavari Client",
                    format!(
                        "Descargando la version {announced_version}. {} KB recibidos.",
                        downloaded_now / 1024
                    ),
                    progress,
                    Some(announced_version.clone()),
                    content_length.is_none(),
                );
            },
            || {
                emit_startup_progress(
                    app,
                    "installing",
                    "Instalando update",
                    "Aplicando la nueva version y preparando el reinicio.",
                    Some(1.0),
                    Some(announced_version.clone()),
                    false,
                );
            },
        )
        .await
        .map_err(|error| error.to_string())?;

    emit_startup_progress(
        app,
        "restarting",
        "Reiniciando launcher",
        "La actualizacion termino. Reiniciando Tavari Client.",
        Some(1.0),
        Some(announced_version),
        false,
    );

    sleep(Duration::from_millis(760)).await;
    app.restart();
}

async fn run_startup_flow(app: AppHandle) {
    emit_startup_progress(
        &app,
        "loading",
        "Iniciando",
        "Preparando Tavari Client.",
        Some(0.08),
        None,
        true,
    );
    sleep(Duration::from_millis(280)).await;

    match check_for_updates(&app).await {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            emit_startup_progress(
                &app,
                "error",
                "Update omitido",
                format!("No se pudo revisar actualizaciones. {error}"),
                Some(0.44),
                None,
                false,
            );
            sleep(Duration::from_millis(700)).await;
        }
    }

    emit_startup_progress(
        &app,
        "loading",
        "Cargando interfaz",
        "Montando el launcher principal.",
        Some(0.72),
        None,
        false,
    );
    sleep(Duration::from_millis(420)).await;

    if let Err(error) = finish_startup(&app).await {
        emit_startup_progress(
            &app,
            "error",
            "Error de inicio",
            error,
            Some(1.0),
            None,
            false,
        );
        let _ = show_main_window(&app);
        close_splash_window(&app);
    }
}

#[tauri::command]
pub async fn startup_ready(
    app: AppHandle,
    startup_state: State<'_, StartupState>,
) -> Result<bool, String> {
    let mut started = startup_state
        .started
        .lock()
        .map_err(|_| "No se pudo iniciar el splash.".to_string())?;

    if *started {
        return Ok(false);
    }

    *started = true;
    drop(started);

    tauri::async_runtime::spawn(run_startup_flow(app));
    Ok(true)
}
