// Copyright (c) 2025 Hamadi
// Licensed under the MIT License

//! Mod installation module

use lighty_loaders::types::{VersionInfo, version_metadata::Mods};
use lighty_core::time_it;
use crate::errors::InstallerResult;
use super::verifier::needs_download;
use super::downloader::download_with_concurrency_limit;

#[cfg(feature = "events")]
use lighty_event::EventBus;

/// Collects mods that need to be downloaded
pub async fn collect_mod_tasks(
    version: &impl VersionInfo,
    mods: &[Mods],
) -> Vec<(String, std::path::PathBuf)> {
    // Don't create mods directory if there are no mods
    if mods.is_empty() {
        return Vec::new();
    }

    let parent_path = version.game_dirs().join("mods");

    // Create mods directory only if there are mods to install
    lighty_core::mkdir!(&parent_path);

    let mut tasks = Vec::new();

    for _mod in mods {
        let Some(url) = &_mod.url else { continue };
        let Some(path_str) = &_mod.path else { continue };

        let path = parent_path.join(path_str);

        if needs_download(&path, _mod.sha1.as_ref(), &_mod.name).await {
            tasks.push((url.clone(), path));
        }
    }

    tasks
}

/// Downloads mods from pre-collected tasks
pub async fn download_mods(
    tasks: Vec<(String, std::path::PathBuf)>,
    #[cfg(feature = "events")] event_bus: Option<&EventBus>,
) -> InstallerResult<()> {
    if tasks.is_empty() {
        lighty_core::trace_info!("[Installer] ✓ All mods already cached and verified");
        return Ok(());
    }

    lighty_core::trace_info!("[Installer] Downloading {} mods...", tasks.len());
    time_it!("Mods download", {
        download_with_concurrency_limit(
            tasks,
            #[cfg(feature = "events")]
            event_bus,
        )
        .await?
    });
    lighty_core::trace_info!("[Installer] ✓ Mods installed");
    Ok(())
}
