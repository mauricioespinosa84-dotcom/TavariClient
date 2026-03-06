use crate::models::{
    normalize_updater_public_key, LauncherAccount, LauncherSettings, DEFAULT_UPDATER_ENDPOINT,
    DEFAULT_UPDATER_PUBLIC_KEY,
};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn state_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let mut dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    dir.push("state");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir)
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let mut path = state_dir(app)?;
    path.push("settings.json");
    Ok(path)
}

fn session_path(app: &AppHandle) -> Result<PathBuf, String> {
    let mut path = state_dir(app)?;
    path.push("session.json");
    Ok(path)
}

fn write_json<T: Serialize>(path: &PathBuf, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let content = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, content).map_err(|error| error.to_string())
}

pub fn sanitize_settings(mut settings: LauncherSettings) -> LauncherSettings {
    settings.backend_base_url = settings.backend_base_url.trim().to_string();
    if !settings.backend_base_url.ends_with('/') {
        settings.backend_base_url.push('/');
    }

    settings.backend_local_path = settings
        .backend_local_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string);

    settings.updater_endpoint = settings
        .updater_endpoint
        .as_deref()
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
        .map(str::to_string)
        .or_else(|| DEFAULT_UPDATER_ENDPOINT.map(str::to_string));

    settings.updater_public_key = settings
        .updater_public_key
        .as_deref()
        .and_then(normalize_updater_public_key)
        .or_else(|| DEFAULT_UPDATER_PUBLIC_KEY.and_then(normalize_updater_public_key));

    settings.data_directory_name = settings.data_directory_name.trim().to_string();
    if settings.data_directory_name.is_empty() {
        settings.data_directory_name = "TavariClient".to_string();
    }

    settings.min_memory_mb = settings.min_memory_mb.max(1024);
    settings.max_memory_mb = settings.max_memory_mb.max(settings.min_memory_mb + 512);
    settings
}

pub fn load_settings(app: &AppHandle) -> Result<LauncherSettings, String> {
    let path = settings_path(app)?;
    if !path.exists() {
        let settings = sanitize_settings(LauncherSettings::default());
        write_json(&path, &settings)?;
        return Ok(settings);
    }

    let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let settings =
        serde_json::from_str::<LauncherSettings>(&content).map_err(|error| error.to_string())?;
    Ok(sanitize_settings(settings))
}

pub fn save_settings_inner(app: &AppHandle, settings: &LauncherSettings) -> Result<(), String> {
    let path = settings_path(app)?;
    write_json(&path, settings)
}

pub fn load_account(app: &AppHandle) -> Result<Option<LauncherAccount>, String> {
    let path = session_path(app)?;
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let account =
        serde_json::from_str::<LauncherAccount>(&content).map_err(|error| error.to_string())?;
    Ok(Some(account))
}

pub fn save_account_inner(app: &AppHandle, account: &LauncherAccount) -> Result<(), String> {
    let path = session_path(app)?;
    write_json(&path, account)
}

pub fn clear_account_inner(app: &AppHandle) -> Result<(), String> {
    let path = session_path(app)?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    settings: LauncherSettings,
) -> Result<LauncherSettings, String> {
    let settings = sanitize_settings(settings);
    save_settings_inner(&app, &settings)?;
    Ok(settings)
}

#[tauri::command]
pub async fn logout(app: AppHandle) -> Result<bool, String> {
    clear_account_inner(&app)?;
    Ok(true)
}
