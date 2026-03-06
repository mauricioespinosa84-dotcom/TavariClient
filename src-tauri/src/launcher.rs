use crate::backend::{load_manifest, resolve_backend, ResolvedBackend};
use crate::models::{
    AccountKind, BackendInstance, BackendManifestEntry, LaunchOutcome, LauncherAccount,
    MicrosoftDeviceCodeEvent, StatusEvent, SyncProgressEvent,
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
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

static PROJECT_DIRS: Lazy<ProjectDirs> = Lazy::new(|| {
    ProjectDirs::from("com", "Tavari Studios", "Tavari Client")
        .expect("project directories should be available")
});

const SECURE_RUNTIME_TTL_SECONDS: u64 = 6 * 60 * 60;
const OBFUSCATED_NAME_LENGTH: usize = 24;

fn unix_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
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

fn needs_copy(target: &Path, expected_hash: &str) -> Result<bool, String> {
    if !target.exists() {
        return Ok(true);
    }

    Ok(!sha1_file(target)?.eq_ignore_ascii_case(expected_hash))
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
    let client_id = resolved
        .launcher_config
        .client_id
        .clone()
        .filter(|value| !value.trim().is_empty() && !value.contains('<'))
        .ok_or_else(|| {
            "Define client_id en launcher/config.json para login premium.".to_string()
        })?;

    let app_handle = app.clone();
    let mut auth = MicrosoftAuth::new(client_id);
    auth.set_device_code_callback(move |user_code, verification_uri| {
        let _ = app_handle.emit(
            "microsoft-device-code",
            MicrosoftDeviceCodeEvent {
                message: "Abre la pagina de Microsoft y pega el codigo mostrado.".to_string(),
                user_code: user_code.to_string(),
                verification_uri: verification_uri.to_string(),
            },
        );
    });
    auth.set_poll_interval(Duration::from_secs(3));

    emit_status(&app, "Microsoft", "Esperando autorizacion premium.")?;
    let profile = auth
        .authenticate()
        .await
        .map_err(|error| error.to_string())?;
    let account = account_from_profile(profile, AccountKind::Microsoft);

    save_account_inner(&app, &account)?;
    emit_status(
        &app,
        "Sesion premium",
        format!("Bienvenido {}", account.username),
    )?;
    Ok(account)
}

#[tauri::command]
pub async fn launch_instance(
    app: AppHandle,
    instance_key: String,
) -> Result<LaunchOutcome, String> {
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
        max_concurrent_downloads: 64,
        max_retries: 4,
        initial_delay_ms: 50,
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

    emit_status(&app, "Lanzando", "Iniciando Minecraft.")?;
    let profile = launcher_profile(&account);
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

    let launch_result = launch.done().run().await.map_err(|error| error.to_string());
    let secure_cleanup_result = if uses_ephemeral_runtime(&resolved, &instance) {
        cleanup_secure_runtime_dir(&app, &game_dir)
    } else {
        Ok(())
    };

    launch_result?;
    secure_cleanup_result?;

    settings.last_instance_key = Some(instance_key.clone());
    save_settings_inner(&app, &settings)?;

    Ok(LaunchOutcome {
        instance_key,
        game_dir: if is_staff {
            game_dir.to_string_lossy().to_string()
        } else {
            String::new()
        },
        message: format!("{} iniciado correctamente.", instance.name),
    })
}
