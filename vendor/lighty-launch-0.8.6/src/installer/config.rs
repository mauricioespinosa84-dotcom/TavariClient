// Copyright (c) 2025 Hamadi
// Licensed under the MIT License

//! Configuration for the installer module

use once_cell::sync::OnceCell;

/// Configuration pour le téléchargement de fichiers
#[derive(Debug, Clone, Copy)]
pub struct DownloaderConfig {
    /// Nombre maximum de téléchargements concurrents (défaut: 50)
    pub max_concurrent_downloads: usize,
    /// Nombre maximum de tentatives en cas d'échec (défaut: 3)
    pub max_retries: u32,
    /// Délai initial en millisecondes avant retry (défaut: 20ms, augmente exponentiellement)
    pub initial_delay_ms: u64,
}

impl Default for DownloaderConfig {
    fn default() -> Self {
        Self {
            max_concurrent_downloads: 50,
            max_retries: 3,
            initial_delay_ms: 20,
        }
    }
}

/// Configuration globale du downloader
static DOWNLOADER_CONFIG: OnceCell<DownloaderConfig> = OnceCell::new();

/// Initialise la configuration du downloader (à appeler une seule fois au démarrage)
///
/// # Example
///
/// ```no_run
/// use lighty_launch::installer::config::{DownloaderConfig, init_downloader_config};
///
/// init_downloader_config(DownloaderConfig {
///     max_concurrent_downloads: 100,
///     max_retries: 5,
///     initial_delay_ms: 50,
/// });
/// ```
pub fn init_downloader_config(config: DownloaderConfig) {
    DOWNLOADER_CONFIG.set(config).ok();
}

/// Récupère la configuration actuelle du downloader
pub(crate) fn get_config() -> DownloaderConfig {
    *DOWNLOADER_CONFIG.get_or_init(DownloaderConfig::default)
}
