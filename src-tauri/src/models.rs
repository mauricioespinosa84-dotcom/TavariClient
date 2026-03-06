use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DEFAULT_BACKEND_PATH: &str = r"C:\Users\mauri\OneDrive\Documents\GitHub\launcher-backend";
pub const DEFAULT_BACKEND_URL: &str =
    "https://mauricioespinosa84-dotcom.github.io/launcher-backend/";
pub const DEFAULT_UPDATER_ENDPOINT: Option<&str> = match option_env!("TAVARI_UPDATER_ENDPOINT") {
    Some(endpoint) => Some(endpoint),
    None => Some(
        "https://github.com/mauricioespinosa84-dotcom/TavariClient/releases/latest/download/latest.json",
    ),
};
pub const DEFAULT_UPDATER_PUBLIC_KEY: Option<&str> = option_env!("TAVARI_UPDATER_PUBKEY");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSettings {
    pub backend_local_path: Option<String>,
    pub backend_base_url: String,
    pub prefer_local_backend: bool,
    pub updater_endpoint: Option<String>,
    pub updater_public_key: Option<String>,
    pub data_directory_name: String,
    pub min_memory_mb: u32,
    pub max_memory_mb: u32,
    pub last_instance_key: Option<String>,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            backend_local_path: Some(DEFAULT_BACKEND_PATH.to_string()),
            backend_base_url: DEFAULT_BACKEND_URL.to_string(),
            prefer_local_backend: true,
            updater_endpoint: DEFAULT_UPDATER_ENDPOINT.map(str::to_string),
            updater_public_key: DEFAULT_UPDATER_PUBLIC_KEY.map(str::to_string),
            data_directory_name: "TavariClient".to_string(),
            min_memory_mb: 2048,
            max_memory_mb: 4096,
            last_instance_key: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountKind {
    Offline,
    Microsoft,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherAccount {
    pub username: String,
    pub uuid: String,
    pub kind: AccountKind,
    pub access_token: Option<String>,
    pub last_used_at: Option<String>,
    #[serde(default)]
    pub backend_session_token: Option<String>,
    #[serde(default)]
    pub backend_session_expires_at: Option<u64>,
    #[serde(default)]
    pub backend_session_is_staff: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackendLauncherConfig {
    #[serde(default)]
    pub maintenance: bool,
    #[serde(default)]
    pub maintenance_message: Option<String>,
    #[serde(default)]
    pub online: bool,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default, rename = "dataDirectory")]
    pub data_directory: Option<String>,
    #[serde(default)]
    pub rss: Option<String>,
    #[serde(default)]
    pub news_enabled: bool,
    #[serde(default)]
    pub cache_version: Option<String>,
    #[serde(default)]
    pub staff_users: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackendLoaderSpec {
    pub loader_type: String,
    pub loader_version: String,
    pub minecraft_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackendServerStatus {
    #[serde(default, rename = "nameServer")]
    pub name_server: Option<String>,
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackendInstance {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub maintenance: bool,
    #[serde(default)]
    pub maintenancemsg: Option<String>,
    pub loader: BackendLoaderSpec,
    #[serde(default)]
    pub verify: bool,
    #[serde(default)]
    pub ignored: Vec<String>,
    #[serde(default, rename = "whitelistActive")]
    pub whitelist_active: bool,
    #[serde(default)]
    pub whitelist: Vec<String>,
    #[serde(default)]
    pub status: BackendServerStatus,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub jvm_args: Vec<String>,
    #[serde(default)]
    pub game_args: Vec<String>,
    #[serde(default, rename = "staffOnly")]
    pub staff_only: bool,
    #[serde(default)]
    pub staffmsg: Option<String>,
    #[serde(default)]
    pub hidden: bool,
}

pub type BackendInstances = HashMap<String, BackendInstance>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendManifestEntry {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendManifestEnvelope {
    pub instance_key: String,
    pub generated_at: String,
    pub expires_at: u64,
    pub entries: Vec<BackendManifestEntry>,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BackendNewsItem {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default, alias = "publish_date")]
    pub publish_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSummary {
    pub key: String,
    pub name: String,
    pub server_label: Option<String>,
    pub loader_type: String,
    pub loader_version: String,
    pub minecraft_version: String,
    pub maintenance: bool,
    pub staff_only: bool,
    pub hidden: bool,
    pub server_address: Option<String>,
    pub source_mode: String,
    pub background_url: Option<String>,
    pub icon_url: Option<String>,
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBootstrap {
    pub product_name: String,
    pub app_version: String,
    pub settings: LauncherSettings,
    pub account: Option<LauncherAccount>,
    pub launcher_config: BackendLauncherConfig,
    pub instances: Vec<InstanceSummary>,
    pub news: Vec<BackendNewsItem>,
    pub is_staff: bool,
    pub backend_summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEvent {
    pub stage: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgressEvent {
    pub current: usize,
    pub total: usize,
    pub file: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftDeviceCodeEvent {
    pub message: String,
    pub user_code: String,
    pub verification_uri: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupProgressEvent {
    pub status: String,
    pub stage: String,
    pub detail: String,
    pub progress: Option<f64>,
    pub version: Option<String>,
    pub indeterminate: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOutcome {
    pub instance_key: String,
    pub game_dir: String,
    pub message: String,
}
