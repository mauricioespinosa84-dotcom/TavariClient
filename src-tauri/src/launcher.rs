use crate::backend::{load_manifest, resolve_backend, ResolvedBackend};
use crate::models::{
    AccountKind, BackendInstance, BackendManifestEntry, LaunchOutcome, LauncherAccount,
    GameLifecycleEvent, MicrosoftDeviceCodeEvent, StatusEvent, SyncProgressEvent,
};
use crate::storage::{load_account, load_settings, save_account_inner, save_settings_inner};
use directories::ProjectDirs;
use lighty_launcher::prelude::{
    init_downloader_config, Authenticator, DownloaderConfig, JavaDistribution, Launch, Loader,
    MicrosoftAuth, OfflineAuth, UserProfile, VersionBuilder, KEY_GAME_DIRECTORY,
    KEY_LAUNCHER_NAME, KEY_LAUNCHER_VERSION,
};
use once_cell::sync::Lazy;
use sha1::{Digest, Sha1};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{
    AppHandle, Emitter, Manager, State, Url, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Child;
use tokio::sync::{oneshot, Mutex as AsyncMutex};
use tokio::time::timeout;
use uuid::Uuid;

static PROJECT_DIRS: Lazy<ProjectDirs> = Lazy::new(|| {
    ProjectDirs::from("com", "Tavari Studios", "Tavari Client")
        .expect("project directories should be available")
});

const SECURE_RUNTIME_TTL_SECONDS: u64 = 6 * 60 * 60;
const OBFUSCATED_NAME_LENGTH: usize = 24;
const LAUNCH_RETRY_ATTEMPTS: usize = 3;
const LAUNCH_RETRY_DELAY_MS: u64 = 1800;
const GAME_LAUNCH_READY_AFTER_SECONDS: u64 = 18;
const GAME_PROGRESS_TICK_MS: u64 = 450;
const MICROSOFT_AUTH_WINDOW_LABEL: &str = "microsoft-auth";
const MICROSOFT_COMPAT_CLIENT_ID: &str = "00000000402b5328";
const MICROSOFT_LIVE_AUTHORIZE_URL: &str = "https://login.live.com/oauth20_authorize.srf";
const MICROSOFT_LIVE_TOKEN_URL: &str = "https://login.live.com/oauth20_token.srf";
const MICROSOFT_LIVE_REDIRECT_URI: &str = "https://login.live.com/oauth20_desktop.srf";
const MICROSOFT_LOGIN_TIMEOUT_SECONDS: u64 = 300;
const XBOX_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_AUTH_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const PERSISTED_RUNTIME_PATHS: &[&str] = &[
    ".fabric",
    "config",
    "options.txt",
    "optionsof.txt",
    "optionsshaders.txt",
    "servers.dat",
    "servers.dat_old",
    "saves",
    "screenshots",
];

#[derive(Clone)]
struct RunningGame {
    instance_key: String,
    instance_name: String,
    child: Arc<AsyncMutex<Child>>,
    game_dir: PathBuf,
    cleanup_secure_runtime: bool,
}

#[derive(Clone)]
pub struct GameRuntimeState {
    current: Arc<AsyncMutex<Option<RunningGame>>>,
}

impl Default for GameRuntimeState {
    fn default() -> Self {
        Self {
            current: Arc::new(AsyncMutex::new(None)),
        }
    }
}

fn unix_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn resolve_microsoft_client_id(resolved: &ResolvedBackend) -> (String, bool) {
    let configured = resolved
        .launcher_config
        .client_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.contains('<'))
        .map(ToOwned::to_owned);

    match configured {
        Some(client_id) => (client_id, false),
        None => (MICROSOFT_COMPAT_CLIENT_ID.to_string(), true),
    }
}

fn build_microsoft_live_auth_url(client_id: &str) -> Result<Url, String> {
    let mut authorize_url =
        Url::parse(MICROSOFT_LIVE_AUTHORIZE_URL).map_err(|error| error.to_string())?;
    authorize_url
        .query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", MICROSOFT_LIVE_REDIRECT_URI)
        .append_pair("scope", "XboxLive.signin offline_access")
        .append_pair("prompt", "select_account");
    Ok(authorize_url)
}

fn close_microsoft_auth_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MICROSOFT_AUTH_WINDOW_LABEL) {
        let _ = window.close();
    }
}

fn open_microsoft_auth_window(app: &AppHandle, verification_uri: &str) -> Result<(), String> {
    let target = Url::parse(verification_uri).map_err(|error| error.to_string())?;
    let app_handle = app.clone();

    app.run_on_main_thread(move || {
        close_microsoft_auth_window(&app_handle);

        let builder = WebviewWindowBuilder::new(
            &app_handle,
            MICROSOFT_AUTH_WINDOW_LABEL,
            WebviewUrl::External(target),
        )
        .title("Sign in to Minecraft")
        .inner_size(600.0, 780.0)
        .resizable(false)
        .center()
        .maximizable(false)
        .minimizable(false)
        .focused(true);

        if let Ok(window) = builder.build() {
            let _ = window.set_focus();
        }
    })
    .map_err(|error| error.to_string())
}

async fn request_microsoft_live_auth_code(
    app: &AppHandle,
    client_id: &str,
) -> Result<String, String> {
    let auth_url = build_microsoft_live_auth_url(client_id)?;
    let (sender, receiver) = oneshot::channel::<Result<String, String>>();
    let sender = Arc::new(std::sync::Mutex::new(Some(sender)));
    let app_handle = app.clone();
    let sender_for_build = sender.clone();

    app.run_on_main_thread(move || {
        close_microsoft_auth_window(&app_handle);

        let sender_for_navigation = sender_for_build.clone();
        let sender_for_close = sender_for_build.clone();
        let app_for_navigation = app_handle.clone();
        let redirect_prefix = MICROSOFT_LIVE_REDIRECT_URI.to_string();

        let builder = WebviewWindowBuilder::new(
            &app_handle,
            MICROSOFT_AUTH_WINDOW_LABEL,
            WebviewUrl::External(auth_url),
        )
        .title("Sign in to Minecraft")
        .inner_size(600.0, 780.0)
        .resizable(false)
        .center()
        .maximizable(false)
        .minimizable(false)
        .focused(true)
        .on_navigation(move |url| {
            if !url.as_str().starts_with(&redirect_prefix) {
                return true;
            }

            let result = {
                let mut code = None;
                let mut error = None;

                for (key, value) in url.query_pairs() {
                    if key == "code" {
                        code = Some(value.into_owned());
                        break;
                    }

                    if key == "error" {
                        error = Some(value.into_owned());
                    }
                }

                if let Some(code) = code {
                    Ok(code)
                } else if let Some(error) = error {
                    Err(format!("Microsoft devolvio el error {error}."))
                } else {
                    Err("Microsoft no devolvio un codigo de autorizacion.".to_string())
                }
            };

            if let Some(sender) = sender_for_navigation.lock().ok().and_then(|mut guard| guard.take())
            {
                let _ = sender.send(result);
            }

            close_microsoft_auth_window(&app_for_navigation);
            false
        });

        match builder.build() {
            Ok(window) => {
                let _ = window.set_focus();
                window.on_window_event(move |event| {
                    if matches!(event, WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed)
                    {
                        if let Some(sender) =
                            sender_for_close.lock().ok().and_then(|mut guard| guard.take())
                        {
                            let _ = sender
                                .send(Err("Inicio de sesion de Microsoft cancelado.".to_string()));
                        }
                    }
                });
            }
            Err(error) => {
                if let Some(sender) = sender_for_build.lock().ok().and_then(|mut guard| guard.take())
                {
                    let _ = sender.send(Err(error.to_string()));
                }
            }
        }
    })
    .map_err(|error| error.to_string())?;

    let code = timeout(Duration::from_secs(MICROSOFT_LOGIN_TIMEOUT_SECONDS), receiver)
        .await
        .map_err(|_| "Tiempo agotado durante el inicio de sesion con Microsoft.".to_string())?
        .map_err(|_| "No se recibio respuesta del inicio de sesion de Microsoft.".to_string())??;

    close_microsoft_auth_window(app);
    Ok(code)
}

async fn exchange_microsoft_live_code(client_id: &str, code: &str) -> Result<String, String> {
    let response = reqwest::Client::new()
        .post(MICROSOFT_LIVE_TOKEN_URL)
        .form(&[
            ("client_id", client_id),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", MICROSOFT_LIVE_REDIRECT_URI),
        ])
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(response.text().await.unwrap_or_else(|_| "Microsoft rechazo el codigo de inicio de sesion.".to_string()));
    }

    let token: PopupMicrosoftTokenResponse = response
        .json()
        .await
        .map_err(|error| error.to_string())?;

    Ok(token.access_token)
}

async fn request_xbox_token(ms_token: &str) -> Result<PopupXboxTokenResponse, String> {
    let response = reqwest::Client::new()
        .post(XBOX_AUTH_URL)
        .json(&serde_json::json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_token}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(response.text().await.unwrap_or_else(|_| "Xbox Live rechazo la autenticacion.".to_string()));
    }

    response.json().await.map_err(|error| error.to_string())
}

async fn request_xsts_token(xbox_token: &str) -> Result<PopupXboxTokenResponse, String> {
    let response = reqwest::Client::new()
        .post(XSTS_AUTH_URL)
        .json(&serde_json::json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [xbox_token]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "XSTS rechazo la autenticacion.".to_string());

        if error_text.contains("2148916233") {
            return Err("Esta cuenta de Microsoft no tiene Minecraft Java Edition.".to_string());
        }

        if error_text.contains("2148916238") {
            return Err("Xbox Live no esta disponible para esta region o cuenta.".to_string());
        }

        return Err(error_text);
    }

    response.json().await.map_err(|error| error.to_string())
}

async fn request_minecraft_token(xsts_token: &str, uhs: &str) -> Result<String, String> {
    let response = reqwest::Client::new()
        .post(MC_AUTH_URL)
        .json(&serde_json::json!({
            "identityToken": format!("XBL3.0 x={uhs};{xsts_token}")
        }))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(response.text().await.unwrap_or_else(|_| "Minecraft rechazo el token de Xbox.".to_string()));
    }

    let token: PopupMinecraftTokenResponse = response
        .json()
        .await
        .map_err(|error| error.to_string())?;

    Ok(token.access_token)
}

async fn request_minecraft_profile(mc_token: &str) -> Result<PopupMinecraftProfile, String> {
    let response = reqwest::Client::new()
        .get(MC_PROFILE_URL)
        .bearer_auth(mc_token)
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(response.text().await.unwrap_or_else(|_| "Minecraft no devolvio un perfil valido.".to_string()));
    }

    response.json().await.map_err(|error| error.to_string())
}

fn format_popup_minecraft_uuid(uuid: &str) -> String {
    if uuid.len() != 32 {
        return uuid.to_string();
    }

    format!(
        "{}-{}-{}-{}-{}",
        &uuid[0..8],
        &uuid[8..12],
        &uuid[12..16],
        &uuid[16..20],
        &uuid[20..32]
    )
}

async fn authenticate_microsoft_popup(
    app: &AppHandle,
    client_id: &str,
) -> Result<UserProfile, String> {
    let code = request_microsoft_live_auth_code(app, client_id).await?;
    emit_status(app, "Microsoft", "Intercambiando codigo de acceso.")?;

    let ms_token = exchange_microsoft_live_code(client_id, &code).await?;
    emit_status(app, "Xbox Live", "Validando la cuenta de Microsoft.")?;

    let xbox_token = request_xbox_token(&ms_token).await?;
    let xsts_token = request_xsts_token(&xbox_token.token).await?;

    let uhs = xsts_token
        .display_claims
        .get("xui")
        .and_then(|xui| xui.get(0))
        .and_then(|user| user.get("uhs"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Microsoft no devolvio el identificador UHS de Xbox.".to_string())?;

    emit_status(app, "Minecraft", "Cargando el perfil premium.")?;
    let minecraft_token = request_minecraft_token(&xsts_token.token, uhs).await?;
    let minecraft_profile = request_minecraft_profile(&minecraft_token).await?;

    Ok(UserProfile {
        id: None,
        username: minecraft_profile.name,
        uuid: format_popup_minecraft_uuid(&minecraft_profile.id),
        access_token: Some(minecraft_token),
        email: None,
        email_verified: true,
        money: None,
        role: None,
        banned: false,
    })
}

fn emit_status(
    app: &AppHandle,
    stage: impl Into<String>,
    detail: impl Into<String>,
) -> Result<(), String> {
    app.emit(
        "launcher-status",
        StatusEvent {
            stage: stage.into(),
            detail: detail.into(),
        },
    )
    .map_err(|error| error.to_string())
}

fn emit_sync_progress(
    app: &AppHandle,
    current: usize,
    total: usize,
    file: impl Into<String>,
) -> Result<(), String> {
    app.emit(
        "sync-progress",
        SyncProgressEvent {
            current,
            total,
            file: file.into(),
        },
    )
    .map_err(|error| error.to_string())
}

fn clamp_progress(progress: Option<f64>) -> Option<f64> {
    progress.map(|value| value.clamp(0.0, 1.0))
}

fn emit_game_lifecycle(
    app: &AppHandle,
    status: impl Into<String>,
    stage: impl Into<String>,
    detail: impl Into<String>,
    progress: Option<f64>,
    can_close: bool,
    instance_key: Option<String>,
) -> Result<(), String> {
    app.emit(
        "game-lifecycle",
        GameLifecycleEvent {
            status: status.into(),
            stage: stage.into(),
            detail: detail.into(),
            progress: clamp_progress(progress),
            can_close,
            instance_key,
        },
    )
    .map_err(|error| error.to_string())
}

fn sanitize_instance_key(instance_key: &str) -> String {
    instance_key
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => character,
            _ => '_',
        })
        .collect()
}

fn game_root_dir(app: &AppHandle, instance_key: &str) -> Result<PathBuf, String> {
    let settings = load_settings(app)?;
    let mut root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    root.push(settings.data_directory_name);
    root.push("instances");
    root.push(sanitize_instance_key(instance_key));
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root)
}

fn secure_runtime_root(app: &AppHandle) -> Result<PathBuf, String> {
    let mut root = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?;
    root.push("secure-runtime");
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root)
}

fn purge_stale_secure_runtime_dirs(app: &AppHandle) -> Result<(), String> {
    let root = secure_runtime_root(app)?;
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(SECURE_RUNTIME_TTL_SECONDS))
        .unwrap_or(UNIX_EPOCH);

    for entry in fs::read_dir(&root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let modified = entry
            .metadata()
            .map_err(|error| error.to_string())?
            .modified()
            .unwrap_or(UNIX_EPOCH);

        if modified <= cutoff {
            let _ = fs::remove_dir_all(path);
        }
    }

    Ok(())
}

fn secure_runtime_dir(app: &AppHandle, instance_key: &str) -> Result<PathBuf, String> {
    purge_stale_secure_runtime_dirs(app)?;

    let mut hasher = Sha1::new();
    hasher.update(instance_key.as_bytes());
    let obfuscated_prefix = format!("{:x}", hasher.finalize());
    let mut runtime_dir = secure_runtime_root(app)?;
    runtime_dir.push(format!(
        "{}-{}",
        &obfuscated_prefix[..12],
        Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&runtime_dir).map_err(|error| error.to_string())?;
    Ok(runtime_dir)
}

fn clear_readonly_recursive(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;

    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            clear_readonly_recursive(&entry.path())?;
        }
    }

    let mut permissions = metadata.permissions();
    if permissions.readonly() {
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions).map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn cleanup_secure_runtime_dir(app: &AppHandle, game_dir: &Path) -> Result<(), String> {
    let secure_root = secure_runtime_root(app)?;
    if !game_dir.starts_with(&secure_root) {
        return Ok(());
    }

    if game_dir.exists() {
        clear_readonly_recursive(game_dir)?;
        fs::remove_dir_all(game_dir).map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn uses_ephemeral_runtime(resolved: &ResolvedBackend, instance: &BackendInstance) -> bool {
    if resolved.source_mode == "backend local directo" {
        return false;
    }

    resolved.secure_api_base.is_some() || instance.staff_only || instance.hidden
}

fn should_obfuscate_runtime_entry(entry: &BackendManifestEntry, ephemeral_runtime: bool) -> bool {
    if !ephemeral_runtime {
        return false;
    }

    matches!(
        entry.path.split('/').next(),
        Some("mods" | "resourcepacks" | "shaderpacks")
    )
}

fn obfuscated_runtime_filename(entry: &BackendManifestEntry) -> String {
    let mut hasher = Sha1::new();
    hasher.update(entry.path.as_bytes());
    hasher.update(entry.hash.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    let extension = Path::new(&entry.path)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty());

    match extension {
        Some(extension) => format!("{}.{}", &digest[..OBFUSCATED_NAME_LENGTH], extension),
        None => digest[..OBFUSCATED_NAME_LENGTH].to_string(),
    }
}

fn target_path_for_entry(
    game_dir: &Path,
    entry: &BackendManifestEntry,
    ephemeral_runtime: bool,
) -> PathBuf {
    let parts = entry
        .path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    if should_obfuscate_runtime_entry(entry, ephemeral_runtime) && parts.len() >= 2 {
        let mut path = game_dir.join(parts[0]);

        for segment in &parts[1..parts.len() - 1] {
            path.push(segment);
        }

        path.push(obfuscated_runtime_filename(entry));
        return path;
    }

    parts.into_iter().fold(game_dir.to_path_buf(), |mut path, segment| {
        path.push(segment);
        path
    })
}

fn harden_runtime_file(path: &Path, ephemeral_runtime: bool) -> Result<(), String> {
    if !ephemeral_runtime || !path.exists() {
        return Ok(());
    }

    let mut permissions = fs::metadata(path)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).map_err(|error| error.to_string())
}

fn backend_game_dir(
    app: &AppHandle,
    resolved: &ResolvedBackend,
    instance_key: &str,
    instance: &BackendInstance,
) -> Result<PathBuf, String> {
    if uses_ephemeral_runtime(resolved, instance) {
        return secure_runtime_dir(app, instance_key);
    }

    if resolved.source_mode == "backend local directo" {
        return resolved
            .instance_source_root(instance)
            .filter(|path| path.exists())
            .ok_or_else(|| {
                format!(
                    "No se encontro la carpeta local del backend para la instancia {instance_key}"
                )
            });
    }

    game_root_dir(app, instance_key)
}

fn java_root_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let mut root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    root.push("TavariClient");
    root.push("runtime");
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root)
}

fn validate_staff_access(
    instance: &BackendInstance,
    account: &LauncherAccount,
    staff_users: &[String],
) -> Result<(), String> {
    if !instance.staff_only {
        return Ok(());
    }

    if account.backend_session_is_staff.unwrap_or(false)
        || staff_users
            .iter()
            .any(|user| user.eq_ignore_ascii_case(&account.username))
    {
        return Ok(());
    }

    Err(instance
        .staffmsg
        .clone()
        .unwrap_or_else(|| "Esta instancia solo esta disponible para staff.".to_string()))
}

fn account_is_staff(account: &LauncherAccount, staff_users: &[String]) -> bool {
    if account.backend_session_is_staff.unwrap_or(false) {
        return true;
    }

    staff_users
        .iter()
        .any(|user| user.eq_ignore_ascii_case(&account.username))
}

fn loader_from_instance(instance: &BackendInstance) -> Result<Loader, String> {
    match instance.loader.loader_type.to_lowercase().as_str() {
        "fabric" => Ok(Loader::Fabric),
        "forge" => Err(
            "Forge todavia no esta soportado por el runner de lighty-launcher 0.8.6. Usa la instancia Fabric o migra esta instancia a NeoForge/Fabric para este launcher."
                .to_string(),
        ),
        "quilt" => Ok(Loader::Quilt),
        "none" | "vanilla" => Ok(Loader::Vanilla),
        other => Err(format!("Loader no soportado: {other}")),
    }
}

fn sha1_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn sha1_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn supports_text_hash_variants(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some(
            "cfg"
                | "conf"
                | "csv"
                | "ini"
                | "json"
                | "lang"
                | "mcmeta"
                | "md"
                | "properties"
                | "snbt"
                | "toml"
                | "txt"
                | "xml"
                | "yaml"
                | "yml"
        )
    )
}

fn text_hash_matches(path: &Path, expected_hash: &str) -> Result<bool, String> {
    if !supports_text_hash_variants(path) {
        return Ok(false);
    }

    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return Ok(false),
    };
    let lf = text.replace("\r\n", "\n").replace('\r', "\n");

    if sha1_bytes(lf.as_bytes()).eq_ignore_ascii_case(expected_hash) {
        return Ok(true);
    }

    let crlf = lf.replace('\n', "\r\n");
    Ok(sha1_bytes(crlf.as_bytes()).eq_ignore_ascii_case(expected_hash))
}

fn hash_matches(path: &Path, expected_hash: &str) -> Result<bool, String> {
    if sha1_file(path)?.eq_ignore_ascii_case(expected_hash) {
        return Ok(true);
    }

    text_hash_matches(path, expected_hash)
}

fn needs_copy(target: &Path, expected_hash: &str) -> Result<bool, String> {
    if !target.exists() {
        return Ok(true);
    }

    Ok(!hash_matches(target, expected_hash)?)
}

fn is_retryable_launch_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("503 service unavailable")
        || normalized.contains("http 503")
        || normalized.contains("status 503")
        || normalized.contains("connection reset")
        || normalized.contains("timed out")
        || normalized.contains("timeout")
        || normalized.contains("temporarily unavailable")
}

async fn download_to_path(
    url: &str,
    path: &Path,
    bearer_token: Option<&str>,
) -> Result<(), String> {
    let mut request = reqwest::Client::new().get(url);

    if let Some(token) = bearer_token.filter(|value| !value.trim().is_empty()) {
        request = request.bearer_auth(token);
    }

    let response = request
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;

    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, bytes).map_err(|error| error.to_string())
}

async fn sync_instance_files(
    app: &AppHandle,
    resolved: &ResolvedBackend,
    instance_key: &str,
    instance: &BackendInstance,
    is_staff: bool,
) -> Result<PathBuf, String> {
    let game_dir = backend_game_dir(app, resolved, instance_key, instance)?;
    let ephemeral_runtime = uses_ephemeral_runtime(resolved, instance);

    if resolved.source_mode == "backend local directo" {
        emit_sync_progress(
            app,
            1,
            1,
            if is_staff {
                format!("Usando instancia local del backend: {}", game_dir.display())
            } else {
                "Preparando archivos del cliente.".to_string()
            },
        )?;
        return Ok(game_dir);
    }

    let manifest = load_manifest(resolved, instance_key, instance).await?;
    let local_source_root = if resolved.source_mode == "backend local directo" {
        resolved.instance_source_root(instance)
    } else {
        None
    };
    let secure_token = resolved.secure_auth_token.as_deref();

    for (index, entry) in manifest.iter().enumerate() {
        let target = target_path_for_entry(&game_dir, entry, ephemeral_runtime);

        emit_sync_progress(
            app,
            index + 1,
            manifest.len(),
            if is_staff {
                entry.path.clone()
            } else {
                "Procesando archivos del cliente.".to_string()
            },
        )?;

        if !needs_copy(&target, &entry.hash)? {
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        if let Some(source_root) = &local_source_root {
            let source = entry
                .path
                .split('/')
                .fold(source_root.clone(), |mut path, segment| {
                    path.push(segment);
                    path
                });

            if source.exists() {
                fs::copy(source, &target).map_err(|error| error.to_string())?;
            } else {
                download_to_path(&entry.url, &target, secure_token).await?;
            }
        } else {
            download_to_path(&entry.url, &target, secure_token).await?;
        }

        harden_runtime_file(&target, ephemeral_runtime)?;

        if needs_copy(&target, &entry.hash)? {
            return Err("Hash invalido despues de sincronizar archivos del cliente.".to_string());
        }
    }

    Ok(game_dir)
}

fn copy_missing_recursive(source: &Path, target: &Path) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }

    if source.is_dir() {
        fs::create_dir_all(target).map_err(|error| error.to_string())?;

        for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let source_path = entry.path();
            let target_path = target.join(entry.file_name());
            copy_missing_recursive(&source_path, &target_path)?;
        }

        return Ok(());
    }

    if target.exists() {
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    fs::copy(source, target).map_err(|error| error.to_string())?;
    Ok(())
}

fn migrate_legacy_runtime_data(game_dir: &Path) -> Result<(), String> {
    let legacy_runtime_dir = game_dir.join("runtime");
    if !legacy_runtime_dir.exists() {
        return Ok(());
    }

    for relative in [
        ".fabric",
        "options.txt",
        "servers.dat",
        "resourcepacks",
        "shaderpacks",
        "saves",
        "screenshots",
    ] {
        copy_missing_recursive(
            &legacy_runtime_dir.join(relative),
            &game_dir.join(relative),
        )?;
    }

    Ok(())
}

fn replace_with_copy(source: &Path, target: &Path) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }

    if source.is_dir() {
        if target.exists() {
            clear_readonly_recursive(target)?;
            if target.is_dir() {
                fs::remove_dir_all(target).map_err(|error| error.to_string())?;
            } else {
                fs::remove_file(target).map_err(|error| error.to_string())?;
            }
        }

        fs::create_dir_all(target).map_err(|error| error.to_string())?;
        for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            replace_with_copy(&entry.path(), &target.join(entry.file_name()))?;
        }
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    if target.exists() {
        clear_readonly_recursive(target)?;
        if target.is_dir() {
            fs::remove_dir_all(target).map_err(|error| error.to_string())?;
        } else {
            fs::remove_file(target).map_err(|error| error.to_string())?;
        }
    }

    fs::copy(source, target).map_err(|error| error.to_string())?;
    Ok(())
}

fn persistent_profile_dir(app: &AppHandle, instance_key: &str) -> Result<PathBuf, String> {
    let settings = load_settings(app)?;
    let mut root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    root.push(settings.data_directory_name);
    root.push("profiles");
    root.push(sanitize_instance_key(instance_key));
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root)
}

fn hydrate_persistent_profile(
    app: &AppHandle,
    instance_key: &str,
    game_dir: &Path,
) -> Result<(), String> {
    let profile_dir = persistent_profile_dir(app, instance_key)?;

    for relative in PERSISTED_RUNTIME_PATHS {
        copy_missing_recursive(&profile_dir.join(relative), &game_dir.join(relative))?;
    }

    Ok(())
}

fn persist_runtime_profile(
    app: &AppHandle,
    instance_key: &str,
    game_dir: &Path,
) -> Result<(), String> {
    let profile_dir = persistent_profile_dir(app, instance_key)?;

    for relative in PERSISTED_RUNTIME_PATHS {
        replace_with_copy(&game_dir.join(relative), &profile_dir.join(relative))?;
    }

    Ok(())
}

fn launcher_profile(account: &LauncherAccount) -> UserProfile {
    UserProfile {
        id: None,
        username: account.username.clone(),
        uuid: account.uuid.clone(),
        access_token: account.access_token.clone(),
        email: None,
        email_verified: matches!(account.kind, AccountKind::Microsoft),
        money: None,
        role: None,
        banned: false,
    }
}

fn account_from_profile(profile: UserProfile, kind: AccountKind) -> LauncherAccount {
    LauncherAccount {
        username: profile.username,
        uuid: profile.uuid,
        kind,
        access_token: profile.access_token,
        last_used_at: Some(unix_timestamp()),
        backend_session_token: None,
        backend_session_expires_at: None,
        backend_session_is_staff: None,
    }
}

fn emit_launch_log_progress(app: &AppHandle, instance_key: &str, line: &str) {
    let normalized = line.trim().to_ascii_lowercase();

    let next_state = if normalized.contains("setting user:")
        || normalized.contains("xsts")
        || normalized.contains("access token")
    {
        Some((
            "launching",
            "Autenticando Minecraft",
            "Aplicando la sesion del jugador.",
            0.84,
        ))
    } else if normalized.contains("loading for game minecraft")
        || normalized.contains("fabricloader")
        || normalized.contains("mixin")
    {
        Some((
            "launching",
            "Cargando mods",
            "Inicializando mods y librerias del cliente.",
            0.88,
        ))
    } else if normalized.contains("resource")
        || normalized.contains("reload")
        || normalized.contains("stitch")
    {
        Some((
            "launching",
            "Cargando recursos",
            "Preparando texturas, packs y recursos del juego.",
            0.92,
        ))
    } else if normalized.contains("openal")
        || normalized.contains("sound engine started")
        || normalized.contains("backend library: lwjgl")
        || normalized.contains("starting up sound engine")
        || normalized.contains("window initialized")
    {
        let _ = emit_game_lifecycle(
            app,
            "running",
            "Juego abierto",
            "Juego iniciado correctamente.",
            Some(1.0),
            true,
            Some(instance_key.to_string()),
        );
        None
    } else {
        None
    };

    if let Some((status, stage, detail, progress)) = next_state {
        let _ = emit_game_lifecycle(
            app,
            status,
            stage,
            detail,
            Some(progress),
            false,
            Some(instance_key.to_string()),
        );
    }
}

async fn drain_game_output<R>(app: AppHandle, instance_key: String, mut reader: R)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut buffer = [0_u8; 4096];
    let mut pending = String::new();

    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => {
                pending.push_str(&String::from_utf8_lossy(&buffer[..read]));

                while let Some(line_break) = pending.find('\n') {
                    let line = pending[..line_break].trim().to_string();
                    pending = pending[line_break + 1..].to_string();

                    if !line.is_empty() {
                        emit_launch_log_progress(&app, &instance_key, &line);
                    }
                }
            }
            Err(_) => break,
        }
    }

    let last_line = pending.trim();
    if !last_line.is_empty() {
        emit_launch_log_progress(&app, &instance_key, last_line);
    }
}

async fn clear_running_game(runtime_state: &GameRuntimeState, instance_key: &str) {
    let mut current = runtime_state.current.lock().await;

    if current
        .as_ref()
        .is_some_and(|game| game.instance_key == instance_key)
    {
        *current = None;
    }
}

async fn finalize_running_game(
    app: &AppHandle,
    runtime_state: &GameRuntimeState,
    running_game: &RunningGame,
) {
    clear_running_game(runtime_state, &running_game.instance_key).await;

    if running_game.cleanup_secure_runtime {
        let _ = persist_runtime_profile(app, &running_game.instance_key, &running_game.game_dir);
        let _ = cleanup_secure_runtime_dir(app, &running_game.game_dir);
    }
}

async fn monitor_running_game(
    app: AppHandle,
    runtime_state: GameRuntimeState,
    running_game: RunningGame,
) {
    let launch_started = tokio::time::Instant::now();
    let ready_after = Duration::from_secs(GAME_LAUNCH_READY_AFTER_SECONDS);
    let mut announced_running = false;

    loop {
        tokio::time::sleep(Duration::from_millis(GAME_PROGRESS_TICK_MS)).await;

        let wait_result = {
            let mut child = running_game.child.lock().await;
            child.try_wait().map_err(|error| error.to_string())
        };

        match wait_result {
            Ok(Some(status)) => {
                let (event_status, stage, detail, progress) = if status.success() {
                    if announced_running {
                        (
                            "stopped",
                            "Juego cerrado",
                            "Minecraft se cerro correctamente.".to_string(),
                            Some(1.0),
                        )
                    } else {
                        (
                            "error",
                            "Arranque interrumpido",
                            "Minecraft se cerro antes de completar el inicio.".to_string(),
                            None,
                        )
                    }
                } else {
                    let exit_code = status.code().unwrap_or(-1);
                    (
                        "error",
                        "Minecraft finalizo con error",
                        format!(
                            "{} termino con codigo de salida {exit_code}.",
                            running_game.instance_name
                        ),
                        None,
                    )
                };

                let _ = emit_game_lifecycle(
                    &app,
                    event_status,
                    stage,
                    detail,
                    progress,
                    false,
                    Some(running_game.instance_key.clone()),
                );
                finalize_running_game(&app, &runtime_state, &running_game).await;
                break;
            }
            Ok(None) => {
                if announced_running {
                    continue;
                }

                let elapsed = launch_started.elapsed();
                if elapsed >= ready_after {
                    announced_running = true;
                    let _ = emit_game_lifecycle(
                        &app,
                        "running",
                        "Juego abierto",
                        "Juego iniciado correctamente.",
                        Some(1.0),
                        true,
                        Some(running_game.instance_key.clone()),
                    );
                    continue;
                }

                let ratio = elapsed.as_secs_f64() / ready_after.as_secs_f64();
                let progress = 0.82 + (ratio * 0.16);
                let detail = if elapsed.as_secs() >= 10 {
                    "Esperando la ventana de Minecraft."
                } else {
                    "Inicializando procesos del juego."
                };

                let _ = emit_game_lifecycle(
                    &app,
                    "launching",
                    "Abriendo juego",
                    detail,
                    Some(progress),
                    false,
                    Some(running_game.instance_key.clone()),
                );
            }
            Err(error) => {
                let _ = emit_game_lifecycle(
                    &app,
                    "error",
                    "Error del launcher",
                    format!("No fue posible seguir el proceso de Minecraft: {error}"),
                    None,
                    false,
                    Some(running_game.instance_key.clone()),
                );
                finalize_running_game(&app, &runtime_state, &running_game).await;
                break;
            }
        }
    }
}

#[tauri::command]
pub async fn login_offline(app: AppHandle, username: String) -> Result<LauncherAccount, String> {
    let mut auth = OfflineAuth::new(username.trim());
    let profile = auth
        .authenticate()
        .await
        .map_err(|error| error.to_string())?;
    let account = account_from_profile(profile, AccountKind::Offline);

    save_account_inner(&app, &account)?;
    emit_status(
        &app,
        "Sesion offline",
        format!("Perfil {} listo.", account.username),
    )?;
    Ok(account)
}

#[tauri::command]
pub async fn login_microsoft(app: AppHandle) -> Result<LauncherAccount, String> {
    let resolved = resolve_backend(&app, None).await?;
    let (client_id, using_compatibility_client_id) = resolve_microsoft_client_id(&resolved);

    let profile = if using_compatibility_client_id {
        emit_status(&app, "Microsoft", "Abriendo ventana de inicio de sesion premium.")?;
        authenticate_microsoft_popup(&app, &client_id).await?
    } else {
        let app_handle = app.clone();
        let mut auth = MicrosoftAuth::new(client_id);
        auth.set_device_code_callback(move |user_code, verification_uri| {
            let _ = open_microsoft_auth_window(&app_handle, verification_uri);
            let _ = app_handle.emit(
                "microsoft-device-code",
                MicrosoftDeviceCodeEvent {
                    message:
                        "Se abrio una ventana de Microsoft. Si no aparece, usa el enlace y escribe el codigo."
                            .to_string(),
                    user_code: user_code.to_string(),
                    verification_uri: verification_uri.to_string(),
                },
            );
        });
        auth.set_poll_interval(Duration::from_secs(3));

        emit_status(&app, "Microsoft", "Esperando autorizacion premium.")?;
        let profile = auth.authenticate().await.map_err(|error| error.to_string());
        close_microsoft_auth_window(&app);
        profile?
    };

    let account = account_from_profile(profile, AccountKind::Microsoft);

    save_account_inner(&app, &account)?;
    emit_status(
        &app,
        "Sesion premium",
        format!("Bienvenido {}", account.username),
    )?;
    Ok(account)
}

#[derive(Debug, serde::Deserialize)]
struct PopupMicrosoftTokenResponse {
    access_token: String,
}

#[derive(Debug, serde::Deserialize)]
struct PopupXboxTokenResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: serde_json::Value,
}

#[derive(Debug, serde::Deserialize)]
struct PopupMinecraftTokenResponse {
    access_token: String,
}

#[derive(Debug, serde::Deserialize)]
struct PopupMinecraftProfile {
    id: String,
    name: String,
}

#[tauri::command]
pub async fn launch_instance(
    app: AppHandle,
    game_state: State<'_, GameRuntimeState>,
    instance_key: String,
) -> Result<LaunchOutcome, String> {
    if game_state.current.lock().await.is_some() {
        return Err("Ya hay un juego en ejecucion. Cierralo antes de iniciar otra instancia.".to_string());
    }

    let mut settings = load_settings(&app)?;
    let account_before =
        load_account(&app)?.ok_or_else(|| "Inicia sesion antes de jugar.".to_string())?;
    let resolved = resolve_backend(&app, Some(account_before.clone())).await?;
    let account = load_account(&app)?
        .unwrap_or(account_before);
    let instance = resolved
        .instances
        .get(&instance_key)
        .cloned()
        .ok_or_else(|| format!("Instancia no encontrada: {instance_key}"))?;

    if instance.maintenance {
        return Err(instance
            .maintenancemsg
            .unwrap_or_else(|| "La instancia esta en mantenimiento.".to_string()));
    }

    let is_staff = resolved
        .secure_is_staff
        .unwrap_or_else(|| account_is_staff(&account, &resolved.launcher_config.staff_users));
    validate_staff_access(&instance, &account, &resolved.launcher_config.staff_users)?;
    let cleanup_secure_runtime = uses_ephemeral_runtime(&resolved, &instance);

    emit_game_lifecycle(
        &app,
        "launching",
        "Preparando",
        "Preparando cliente.",
        Some(0.04),
        false,
        Some(instance_key.clone()),
    )?;

    emit_status(
        &app,
        "Sincronizando",
        if is_staff {
            format!("Preparando {}", instance.name)
        } else {
            "Preparando cliente.".to_string()
        },
    )?;
    let game_dir = sync_instance_files(&app, &resolved, &instance_key, &instance, is_staff).await?;
    migrate_legacy_runtime_data(&game_dir)?;
    if cleanup_secure_runtime {
        hydrate_persistent_profile(&app, &instance_key, &game_dir)?;
    }

    emit_status(
        &app,
        if uses_ephemeral_runtime(&resolved, &instance) {
            "Runtime protegido"
        } else if resolved.source_mode == "backend local directo" && is_staff {
            "Backend local"
        } else {
            "Preparando cliente"
        },
        if uses_ephemeral_runtime(&resolved, &instance) {
            "Montando archivos del cliente en un runtime temporal protegido."
        } else if resolved.source_mode == "backend local directo" && is_staff {
            "Usando la instancia directamente desde el backend local."
        } else {
            "Verificando archivos y entorno del juego."
        },
    )?;
    init_downloader_config(DownloaderConfig {
        max_concurrent_downloads: 8,
        max_retries: 8,
        initial_delay_ms: 250,
    });

    let loader = loader_from_instance(&instance)?;
    let java_dir = java_root_dir(&app)?;
    let mut version = VersionBuilder::new(
        &instance_key,
        loader,
        &instance.loader.loader_version,
        &instance.loader.minecraft_version,
        &PROJECT_DIRS,
    )
    .with_custom_game_dir(game_dir.clone())
    .with_custom_java_dir(java_dir);

    let profile = launcher_profile(&account);
    let mut launch_result = Err("No fue posible iniciar Minecraft.".to_string());

    for attempt in 1..=LAUNCH_RETRY_ATTEMPTS {
        emit_status(
            &app,
            if attempt == 1 {
                "Lanzando".to_string()
            } else {
                format!("Reintentando {attempt}/{LAUNCH_RETRY_ATTEMPTS}")
            },
            if attempt == 1 {
                "Iniciando Minecraft.".to_string()
            } else {
                "Reintentando el arranque despues de un error temporal del backend.".to_string()
            },
        )?;
        emit_game_lifecycle(
            &app,
            "launching",
            if attempt == 1 {
                "Lanzando"
            } else {
                "Reintentando"
            },
            if attempt == 1 {
                "Iniciando Minecraft."
            } else {
                "Reintentando el arranque despues de un error temporal del backend."
            },
            Some(0.82),
            false,
            Some(instance_key.clone()),
        )?;

        let mut launch = version
            .launch(&profile, JavaDistribution::Temurin)
            .with_jvm_options()
            .set("Xms", format!("{}M", settings.min_memory_mb))
            .set("Xmx", format!("{}M", settings.max_memory_mb))
            .done()
            .with_arguments()
            .set(KEY_LAUNCHER_NAME, "Tavari Client")
            .set(KEY_LAUNCHER_VERSION, env!("CARGO_PKG_VERSION"))
            .set(KEY_GAME_DIRECTORY, game_dir.to_string_lossy().to_string())
            .set("width", "1600")
            .set("height", "900");

        if let Some(ip) = instance.status.ip.as_deref() {
            launch = launch.set("server", ip);
        }

        if let Some(port) = instance.status.port {
            launch = launch.set("port", port.to_string());
        }

        match launch.done().spawn().await.map_err(|error| error.to_string()) {
            Ok(child) => {
                launch_result = Ok(child);
                break;
            }
            Err(error) if attempt < LAUNCH_RETRY_ATTEMPTS && is_retryable_launch_error(&error) => {
                emit_status(
                    &app,
                    "Backend ocupado",
                    "GitHub devolvio un error temporal. Reintentando automaticamente.",
                )?;
                tokio::time::sleep(Duration::from_millis(
                    LAUNCH_RETRY_DELAY_MS * attempt as u64,
                ))
                .await;
            }
            Err(error) => {
                launch_result = Err(error);
                break;
            }
        }
    }

    let mut child = match launch_result {
        Ok(child) => child,
        Err(error) => {
            if cleanup_secure_runtime {
                let _ = cleanup_secure_runtime_dir(&app, &game_dir);
            }
            return Err(error);
        }
    };

    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(drain_game_output(app.clone(), instance_key.clone(), stdout));
    }

    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(drain_game_output(app.clone(), instance_key.clone(), stderr));
    }

    let running_game = RunningGame {
        instance_key: instance_key.clone(),
        instance_name: instance.name.clone(),
        child: Arc::new(AsyncMutex::new(child)),
        game_dir: game_dir.clone(),
        cleanup_secure_runtime,
    };

    {
        let mut current = game_state.current.lock().await;
        if current.is_some() {
            if cleanup_secure_runtime {
                let _ = cleanup_secure_runtime_dir(&app, &game_dir);
            }
            return Err("Ya hay un juego en ejecucion. Cierralo antes de iniciar otra instancia.".to_string());
        }
        *current = Some(running_game.clone());
    }

    emit_game_lifecycle(
        &app,
        "launching",
        "Lanzando",
        "Proceso de Minecraft creado. Esperando inicializacion.",
        Some(0.82),
        false,
        Some(instance_key.clone()),
    )?;

    tokio::spawn(monitor_running_game(
        app.clone(),
        game_state.inner().clone(),
        running_game,
    ));

    settings.last_instance_key = Some(instance_key.clone());
    save_settings_inner(&app, &settings)?;

    Ok(LaunchOutcome {
        instance_key,
        game_dir: if is_staff {
            game_dir.to_string_lossy().to_string()
        } else {
            String::new()
        },
        message: format!("{} se esta abriendo.", instance.name),
    })
}

#[tauri::command]
pub async fn close_running_game(
    app: AppHandle,
    game_state: State<'_, GameRuntimeState>,
) -> Result<(), String> {
    let running_game = game_state
        .current
        .lock()
        .await
        .clone()
        .ok_or_else(|| "No hay un juego en ejecucion.".to_string())?;

    emit_game_lifecycle(
        &app,
        "closing",
        "Cerrando juego",
        "Solicitando cierre de Minecraft.",
        Some(1.0),
        false,
        Some(running_game.instance_key.clone()),
    )?;

    let mut child = running_game.child.lock().await;
    child.kill().await.map_err(|error| error.to_string())
}
