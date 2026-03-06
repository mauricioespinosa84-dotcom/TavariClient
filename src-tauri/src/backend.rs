use crate::models::{
    AppBootstrap, BackendInstance, BackendInstances, BackendLauncherConfig,
    BackendManifestEnvelope, BackendManifestEntry, BackendNewsItem, InstanceSummary,
    LauncherAccount,
};
use crate::storage::{load_account, load_settings, save_account_inner};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

const SESSION_GRACE_SECONDS: u64 = 60;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecureHealthResponse {
    #[serde(default)]
    secure_mode: bool,
    #[serde(default)]
    manifest_public_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecureBootstrapResponse {
    launcher_config: BackendLauncherConfig,
    instances: BackendInstances,
    #[serde(default)]
    news: Vec<BackendNewsItem>,
    #[serde(default)]
    is_staff: bool,
    #[serde(default)]
    manifest_public_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SecureSessionRequest {
    kind: String,
    username: String,
    uuid: String,
    access_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecureSessionResponse {
    token: String,
    expires_at: u64,
    #[serde(default)]
    is_staff: bool,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    uuid: Option<String>,
}

pub struct ResolvedBackend {
    pub local_root: Option<PathBuf>,
    pub base_url: String,
    pub source_mode: String,
    pub launcher_config: BackendLauncherConfig,
    pub instances: BackendInstances,
    pub secure_api_base: Option<String>,
    pub secure_manifest_public_key: Option<String>,
    pub secure_auth_token: Option<String>,
    pub secure_is_staff: Option<bool>,
    pub prefetched_news: Option<Vec<BackendNewsItem>>,
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn join_relative(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .filter(|segment| !segment.is_empty())
        .fold(root.to_path_buf(), |mut path, segment| {
            path.push(segment);
            path
        })
}

fn read_json_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&content).map_err(|error| error.to_string())
}

async fn read_json_url<T: serde::de::DeserializeOwned>(
    url: &str,
    bearer_token: Option<&str>,
) -> Result<T, String> {
    let mut request = Client::new().get(url);

    if let Some(token) = bearer_token.filter(|value| !value.trim().is_empty()) {
        request = request.bearer_auth(token);
    }

    let response = request
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;

    response
        .json::<T>()
        .await
        .map_err(|error| error.to_string())
}

async fn post_json_url<T: serde::de::DeserializeOwned, B: Serialize>(
    url: &str,
    body: &B,
) -> Result<T, String> {
    let response = Client::new()
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;

    response
        .json::<T>()
        .await
        .map_err(|error| error.to_string())
}

fn backend_local_available(local_root: &Option<PathBuf>) -> bool {
    local_root
        .as_ref()
        .is_some_and(|root| root.exists() && root.join("launcher").join("config.json").exists())
}

fn strip_base_url(base_url: &str, full_url: &str) -> Option<String> {
    let base = base_url.trim_end_matches('/');
    let target = full_url.trim();
    target
        .strip_prefix(base)
        .map(|relative| relative.trim_start_matches('/').to_string())
}

fn fallback_manifest_relative(instance: &BackendInstance) -> String {
    format!("files/{}/manifest.json", instance.loader.loader_type)
}

pub fn manifest_relative_path(base_url: &str, instance: &BackendInstance) -> String {
    strip_base_url(base_url, &instance.url).unwrap_or_else(|| fallback_manifest_relative(instance))
}

impl ResolvedBackend {
    pub fn instance_source_root(&self, instance: &BackendInstance) -> Option<PathBuf> {
        let local_root = self.local_root.as_ref()?;
        let manifest_relative = manifest_relative_path(&self.base_url, instance);
        let manifest_path = join_relative(local_root, &manifest_relative);
        manifest_path.parent().map(Path::to_path_buf)
    }
}

fn backend_session_valid(account: &LauncherAccount) -> bool {
    account
        .backend_session_token
        .as_deref()
        .is_some_and(|token| !token.trim().is_empty())
        && account
            .backend_session_expires_at
            .unwrap_or_default()
            .saturating_sub(SESSION_GRACE_SECONDS)
            > unix_timestamp()
}

async fn ensure_secure_session(
    app: &AppHandle,
    account: &LauncherAccount,
    api_base: &str,
) -> Result<LauncherAccount, String> {
    if backend_session_valid(account) {
        return Ok(account.clone());
    }

    let request = SecureSessionRequest {
        kind: match account.kind {
            crate::models::AccountKind::Offline => "offline".to_string(),
            crate::models::AccountKind::Microsoft => "microsoft".to_string(),
        },
        username: account.username.clone(),
        uuid: account.uuid.clone(),
        access_token: account.access_token.clone(),
    };
    let url = format!("{}/api/auth/session", api_base.trim_end_matches('/'));
    let session = post_json_url::<SecureSessionResponse, _>(&url, &request).await?;
    let mut updated = account.clone();

    if let Some(username) = session.username {
        updated.username = username;
    }

    if let Some(uuid) = session.uuid {
        updated.uuid = uuid;
    }

    updated.backend_session_token = Some(session.token);
    updated.backend_session_expires_at = Some(session.expires_at);
    updated.backend_session_is_staff = Some(session.is_staff);
    save_account_inner(app, &updated)?;
    Ok(updated)
}

async fn try_secure_health(base_url: &str) -> Result<Option<SecureHealthResponse>, String> {
    let url = format!("{}/api/health", base_url.trim_end_matches('/'));
    let response = match Client::new().get(&url).send().await {
        Ok(response) => response,
        Err(_) => return Ok(None),
    };

    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if !response.status().is_success() {
        return Err(format!(
            "El backend seguro respondio con estado {}.",
            response.status()
        ));
    }

    let health = response
        .json::<SecureHealthResponse>()
        .await
        .map_err(|error| error.to_string())?;

    if !health.secure_mode {
        return Ok(None);
    }

    Ok(Some(health))
}

async fn fetch_secure_bootstrap(
    api_base: &str,
    bearer_token: Option<&str>,
) -> Result<SecureBootstrapResponse, String> {
    let url = format!("{}/api/bootstrap", api_base.trim_end_matches('/'));
    read_json_url::<SecureBootstrapResponse>(&url, bearer_token).await
}

async fn try_resolve_secure_backend(
    app: &AppHandle,
    base_url: &str,
    local_root: Option<PathBuf>,
    account: Option<&LauncherAccount>,
) -> Result<Option<ResolvedBackend>, String> {
    let Some(health) = try_secure_health(base_url).await? else {
        return Ok(None);
    };

    let api_base = base_url.trim_end_matches('/').to_string();
    let session_account = match account {
        Some(account) => Some(ensure_secure_session(app, account, &api_base).await?),
        None => None,
    };
    let token = session_account
        .as_ref()
        .and_then(|account| account.backend_session_token.clone());
    let bootstrap = fetch_secure_bootstrap(&api_base, token.as_deref()).await?;

    Ok(Some(ResolvedBackend {
        local_root,
        base_url: base_url.to_string(),
        source_mode: "backend seguro".to_string(),
        launcher_config: bootstrap.launcher_config,
        instances: bootstrap.instances,
        secure_api_base: Some(api_base),
        secure_manifest_public_key: bootstrap.manifest_public_key.or(health.manifest_public_key),
        secure_auth_token: token,
        secure_is_staff: Some(bootstrap.is_staff),
        prefetched_news: Some(bootstrap.news),
    }))
}

pub async fn resolve_backend(
    app: &AppHandle,
    account: Option<LauncherAccount>,
) -> Result<ResolvedBackend, String> {
    let settings = load_settings(app)?;
    let local_root = settings.backend_local_path.as_deref().map(PathBuf::from);

    if let Some(resolved) = try_resolve_secure_backend(
        app,
        &settings.backend_base_url,
        local_root.clone(),
        account.as_ref(),
    )
    .await?
    {
        return Ok(resolved);
    }

    let can_use_local = settings.prefer_local_backend && backend_local_available(&local_root);

    if can_use_local {
        let root = local_root.expect("validated local backend root");
        let launcher_config =
            read_json_file::<BackendLauncherConfig>(&root.join("launcher").join("config.json"))?;
        let instances =
            read_json_file::<BackendInstances>(&root.join("launcher").join("instances.json"))?;
        return Ok(ResolvedBackend {
            local_root: Some(root),
            base_url: settings.backend_base_url,
            source_mode: "backend local directo".to_string(),
            launcher_config,
            instances,
            secure_api_base: None,
            secure_manifest_public_key: None,
            secure_auth_token: None,
            secure_is_staff: account
                .as_ref()
                .and_then(|active| active.backend_session_is_staff),
            prefetched_news: None,
        });
    }

    let config_url = format!("{}launcher/config.json", settings.backend_base_url);
    let instances_url = format!("{}launcher/instances.json", settings.backend_base_url);

    let launcher_config = read_json_url::<BackendLauncherConfig>(&config_url, None).await?;
    let instances = read_json_url::<BackendInstances>(&instances_url, None).await?;

    Ok(ResolvedBackend {
        local_root,
        base_url: settings.backend_base_url,
        source_mode: "backend remoto cacheado".to_string(),
        launcher_config,
        instances,
        secure_api_base: None,
        secure_manifest_public_key: None,
        secure_auth_token: None,
        secure_is_staff: account
            .as_ref()
            .and_then(|active| active.backend_session_is_staff),
        prefetched_news: None,
    })
}

fn manifest_signature_payload(envelope: &BackendManifestEnvelope) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&serde_json::json!({
        "instanceKey": envelope.instance_key,
        "generatedAt": envelope.generated_at,
        "expiresAt": envelope.expires_at,
        "entries": envelope.entries,
    }))
    .map_err(|error| error.to_string())
}

fn verify_manifest_signature(
    envelope: &BackendManifestEnvelope,
    public_key_base64: &str,
) -> Result<(), String> {
    if envelope.expires_at <= unix_timestamp() {
        return Err("El manifest seguro expiro y debe descargarse de nuevo.".to_string());
    }

    let public_key_bytes = BASE64_STANDARD
        .decode(public_key_base64.as_bytes())
        .map_err(|error| error.to_string())?;
    let public_key_array = <[u8; 32]>::try_from(public_key_bytes.as_slice())
        .map_err(|_| "La clave publica del backend seguro no es valida.".to_string())?;
    let public_key = VerifyingKey::from_bytes(&public_key_array)
        .map_err(|error| error.to_string())?;
    let signature_bytes = BASE64_STANDARD
        .decode(envelope.signature.as_bytes())
        .map_err(|error| error.to_string())?;
    let signature = Signature::try_from(signature_bytes.as_slice())
        .map_err(|_| "La firma del manifest seguro no es valida.".to_string())?;

    public_key
        .verify(&manifest_signature_payload(envelope)?, &signature)
        .map_err(|_| "La firma del manifest seguro no coincide.".to_string())
}

pub async fn load_manifest(
    resolved: &ResolvedBackend,
    instance_key: &str,
    instance: &BackendInstance,
) -> Result<Vec<BackendManifestEntry>, String> {
    if let Some(api_base) = &resolved.secure_api_base {
        let token = resolved
            .secure_auth_token
            .as_deref()
            .ok_or_else(|| "La sesion segura del backend expiro. Inicia sesion de nuevo.".to_string())?;
        let mut url = Url::parse(api_base).map_err(|error| error.to_string())?;
        {
            let mut segments = url.path_segments_mut().map_err(|_| {
                "No fue posible construir la URL del manifest seguro.".to_string()
            })?;
            segments.pop_if_empty();
            segments.push("api");
            segments.push("manifest");
            segments.push(instance_key);
        }

        let envelope = read_json_url::<BackendManifestEnvelope>(url.as_str(), Some(token)).await?;
        let public_key = resolved
            .secure_manifest_public_key
            .as_deref()
            .ok_or_else(|| "El backend seguro no entrego la clave publica del manifest.".to_string())?;
        verify_manifest_signature(&envelope, public_key)?;
        return Ok(envelope.entries);
    }

    if resolved.source_mode == "backend local directo" {
        let local_root = resolved
            .instance_source_root(instance)
            .ok_or_else(|| "No se pudo resolver la ruta local del manifest".to_string())?;
        return read_json_file(&local_root.join("manifest.json"));
    }

    read_json_url::<Vec<BackendManifestEntry>>(&instance.url, None).await
}

fn normalize_backend_text(value: &str) -> String {
    if !value.contains('Ã') && !value.contains('Â') && !value.contains('â') {
        return value.to_string();
    }

    let bytes = value
        .chars()
        .map(|character| u32::from(character))
        .map(u8::try_from)
        .collect::<Result<Vec<_>, _>>();

    match bytes
        .map_err(|_| ())
        .and_then(|bytes| String::from_utf8(bytes).map_err(|_| ()))
    {
        Ok(decoded) => decoded,
        Err(_) => value.to_string(),
    }
}

fn normalize_news_item(item: BackendNewsItem) -> BackendNewsItem {
    BackendNewsItem {
        title: normalize_backend_text(&item.title),
        content: normalize_backend_text(&item.content),
        author: item.author.map(|author| normalize_backend_text(&author)),
        publish_date: item.publish_date,
    }
}

fn normalize_news(mut news: Vec<BackendNewsItem>) -> Vec<BackendNewsItem> {
    news = news
        .into_iter()
        .map(normalize_news_item)
        .collect::<Vec<_>>();
    news.sort_by(|left, right| right.publish_date.cmp(&left.publish_date));
    news
}

pub async fn load_news(resolved: &ResolvedBackend) -> Result<Vec<BackendNewsItem>, String> {
    if let Some(news) = &resolved.prefetched_news {
        return Ok(normalize_news(news.clone()));
    }

    let news = if resolved.source_mode == "backend local directo" {
        let local_root = resolved
            .local_root
            .as_ref()
            .ok_or_else(|| "No se encontro el backend local".to_string())?;
        let news_path = local_root.join("launcher").join("news.json");
        if !news_path.exists() {
            Vec::new()
        } else {
            read_json_file::<Vec<BackendNewsItem>>(&news_path)?
        }
    } else {
        let news_url = format!("{}launcher/news.json", resolved.base_url);
        read_json_url::<Vec<BackendNewsItem>>(&news_url, None)
            .await
            .unwrap_or_default()
    };

    Ok(normalize_news(news))
}

fn build_server_address(instance: &BackendInstance) -> Option<String> {
    let ip = instance.status.ip.as_deref()?;
    let port = instance.status.port?;
    Some(format!("{ip}:{port}"))
}

fn asset_url(base_url: &str, path: &Option<String>) -> Option<String> {
    let value = path.as_deref()?.trim();
    if value.is_empty() {
        return None;
    }

    if value.starts_with("http://") || value.starts_with("https://") {
        return Some(value.to_string());
    }

    Some(format!(
        "{}{}",
        base_url.trim_end_matches('/'),
        if value.starts_with('/') {
            value.to_string()
        } else {
            format!("/{}", value)
        }
    ))
}

fn account_is_staff(account: Option<&LauncherAccount>, staff_users: &[String]) -> bool {
    let Some(account) = account else {
        return false;
    };

    if account.backend_session_is_staff.unwrap_or(false) {
        return true;
    }

    staff_users
        .iter()
        .any(|user| user.eq_ignore_ascii_case(&account.username))
}

fn sanitize_launcher_config_for_ui(
    mut launcher_config: BackendLauncherConfig,
    is_staff: bool,
) -> BackendLauncherConfig {
    launcher_config.client_id = None;
    launcher_config.staff_users.clear();

    if !is_staff {
        launcher_config.cache_version = None;
    }

    launcher_config
}

fn backend_summary_for_user(
    resolved: &ResolvedBackend,
    instances_len: usize,
    is_staff: bool,
) -> String {
    if is_staff {
        return format!(
            "{} - {} instancia(s) - cache {}",
            resolved.source_mode,
            instances_len,
            resolved
                .launcher_config
                .cache_version
                .clone()
                .unwrap_or_else(|| "sin version".to_string())
        );
    }

    if instances_len == 0 {
        return "Cliente listo - sin instancias disponibles.".to_string();
    }

    "Cliente listo para jugar.".to_string()
}

pub fn build_instance_summaries(resolved: &ResolvedBackend, is_staff: bool) -> Vec<InstanceSummary> {
    let mut instances = resolved
        .instances
        .iter()
        .map(|(key, instance)| InstanceSummary {
            key: key.clone(),
            name: instance.name.clone(),
            server_label: instance.status.name_server.clone(),
            loader_type: instance.loader.loader_type.clone(),
            loader_version: instance.loader.loader_version.clone(),
            minecraft_version: instance.loader.minecraft_version.clone(),
            maintenance: instance.maintenance,
            staff_only: instance.staff_only,
            hidden: instance.hidden,
            server_address: build_server_address(instance),
            source_mode: if is_staff {
                resolved.source_mode.clone()
            } else {
                "Tavari Client".to_string()
            },
            background_url: asset_url(&resolved.base_url, &instance.background),
            icon_url: asset_url(&resolved.base_url, &instance.icon),
            thumbnail_url: asset_url(&resolved.base_url, &instance.thumbnail),
        })
        .collect::<Vec<_>>();

    instances.sort_by(|left, right| left.name.cmp(&right.name));
    instances
}

#[tauri::command]
pub async fn get_bootstrap(app: AppHandle) -> Result<AppBootstrap, String> {
    let settings = load_settings(&app)?;
    let account_before = load_account(&app)?;
    let resolved = resolve_backend(&app, account_before.clone()).await?;
    let account = load_account(&app)?;
    let is_staff = resolved
        .secure_is_staff
        .unwrap_or_else(|| account_is_staff(account.as_ref(), &resolved.launcher_config.staff_users));
    let instances = build_instance_summaries(&resolved, is_staff);
    let news = if resolved.launcher_config.news_enabled {
        load_news(&resolved).await?
    } else {
        Vec::new()
    };
    let public_backend_summary = backend_summary_for_user(&resolved, instances.len(), is_staff);
    let _legacy_backend_summary = format!(
        "{} · {} instancia(s) · cache {}",
        resolved.source_mode,
        instances.len(),
        resolved
            .launcher_config
            .cache_version
            .clone()
            .unwrap_or_else(|| "sin version".to_string())
    );

    let backend_summary = format!(
        "{} - {} instancia(s) - cache {}",
        resolved.source_mode,
        instances.len(),
        resolved
            .launcher_config
            .cache_version
            .clone()
            .unwrap_or_else(|| "sin version".to_string())
    );

    Ok(AppBootstrap {
        product_name: app.package_info().name.clone(),
        app_version: app.package_info().version.to_string(),
        settings,
        account,
        launcher_config: sanitize_launcher_config_for_ui(resolved.launcher_config, is_staff),
        instances,
        news,
        is_staff,
        backend_summary: if is_staff {
            backend_summary
        } else {
            public_backend_summary
        },
    })
}
