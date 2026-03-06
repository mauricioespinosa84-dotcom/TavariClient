// Copyright (c) 2025 Hamadi
// Licensed under the MIT License

//! Game installation orchestration module
//!
//! This module coordinates the installation of all game components:
//! - Libraries (game dependencies)
//! - Natives (platform-specific binaries)
//! - Client JAR (game executable)
//! - Assets (textures, sounds, etc.)
//! - Mods (optional modifications)

use super::{libraries, natives, client, assets, mods};
use lighty_loaders::types::{VersionInfo, version_metadata::Version};
use lighty_core::{mkdir, time_it};
use crate::errors::InstallerResult;

#[cfg(feature = "events")]
use lighty_event::{EventBus, Event, LaunchEvent};

// Re-export the trait
pub use self::installer_trait::Installer;

mod installer_trait {
    use super::*;
    use std::future::Future;

    /// Installation trait for version builders
    pub trait Installer {
        fn install(
            &self,
            builder: &Version,
            #[cfg(feature = "events")] event_bus: Option<&EventBus>,
        ) -> impl Future<Output = InstallerResult<()>> + Send;
    }
}

impl<T: VersionInfo> Installer for T {
    /// Installs all dependencies in parallel
    async fn install(
        &self,
        builder: &Version,
        #[cfg(feature = "events")] event_bus: Option<&EventBus>,
    ) -> InstallerResult<()> {
        lighty_core::trace_info!("[Installer] Starting installation for {}", self.name());

        create_directories(self).await;

        // Phase 1: Collect all tasks (single SHA1 verification pass)
        lighty_core::trace_info!("[Installer] Verifying installed files...");
        let (library_tasks, client_task, asset_tasks, mod_tasks, (native_download_tasks, native_extract_paths)) = tokio::join!(
            libraries::collect_library_tasks(self, &builder.libraries),
            client::collect_client_task(self, builder.client.as_ref()),
            assets::collect_asset_tasks(self, builder.assets.as_ref()),
            mods::collect_mod_tasks(self, builder.mods.as_deref().unwrap_or(&[])),
            natives::collect_native_tasks(self, builder.natives.as_deref().unwrap_or(&[])),
        );

        // Count total downloads needed
        let total_downloads = library_tasks.len()
            + client_task.as_ref().map(|_| 1).unwrap_or(0)
            + asset_tasks.len()
            + mod_tasks.len()
            + native_download_tasks.len();

        // Phase 2: Decide if installation is needed
        if total_downloads == 0 {
            // Everything is already installed, just need to extract natives
            #[cfg(feature = "events")]
            if let Some(bus) = event_bus {
                bus.emit(Event::Launch(LaunchEvent::IsInstalled {
                    version: self.name().to_string(),
                }));
            }

            lighty_core::trace_info!("[Installer] ✓ All files already up-to-date");

            // Still need to extract natives (they're cleaned on each run)
            if !native_extract_paths.is_empty() {
                natives::download_and_extract_natives(
                    self,
                    native_download_tasks,
                    native_extract_paths,
                    #[cfg(feature = "events")]
                    event_bus,
                )
                    .await?;
            }

            lighty_core::trace_info!("[Installer] Installation completed successfully!");
            return Ok(());
        }

        // Phase 3: Download needed files
        #[cfg(feature = "events")]
        let total_bytes = calculate_download_size(
            builder,
            &library_tasks,
            &client_task,
            &asset_tasks,
            &mod_tasks,
            &native_download_tasks,
        );

        #[cfg(feature = "events")]
        if let Some(bus) = event_bus {
            bus.emit(Event::Launch(LaunchEvent::InstallStarted {
                version: self.name().to_string(),
                total_bytes,
            }));
        }

        lighty_core::trace_info!("[Installer] Downloading {} file(s)...", total_downloads);

        time_it!("Total installation", {
            // Download and install in parallel
            tokio::try_join!(
                libraries::download_libraries(
                    library_tasks,
                    #[cfg(feature = "events")]
                    event_bus
                ),
                natives::download_and_extract_natives(
                    self,
                    native_download_tasks,
                    native_extract_paths,
                    #[cfg(feature = "events")]
                    event_bus
                ),
                mods::download_mods(
                    mod_tasks,
                    #[cfg(feature = "events")]
                    event_bus
                ),
                client::download_client(
                    client_task,
                    #[cfg(feature = "events")]
                    event_bus
                ),
                assets::download_assets(
                    asset_tasks,
                    #[cfg(feature = "events")]
                    event_bus
                ),
            )?;
        });

        #[cfg(feature = "events")]
        if let Some(bus) = event_bus {
            bus.emit(Event::Launch(LaunchEvent::InstallCompleted {
                version: self.name().to_string(),
                total_bytes,
            }));
        }

        lighty_core::trace_info!("[Installer] Installation completed successfully!");
        Ok(())
    }
}

/// Creates necessary installation directories
async fn create_directories(version: &impl VersionInfo) {
    let parent_path = version.game_dirs().to_path_buf();
    mkdir!(parent_path.join("libraries"));
    mkdir!(parent_path.join("natives"));
    mkdir!(parent_path.join("assets").join("objects"));
}

/// Calculates the total size of files that need to be downloaded (from tasks)
#[cfg(feature = "events")]
fn calculate_download_size(
    builder: &Version,
    library_tasks: &[(String, std::path::PathBuf)],
    client_task: &Option<(String, std::path::PathBuf)>,
    asset_tasks: &[(String, std::path::PathBuf)],
    mod_tasks: &[(String, std::path::PathBuf)],
    native_download_tasks: &[(String, std::path::PathBuf)],
) -> u64 {
    let mut total = 0u64;

    // Libraries - match tasks with builder.libraries to get size
    for (url, _) in library_tasks {
        if let Some(lib) = builder.libraries.iter().find(|l| l.url.as_ref() == Some(url)) {
            total += lib.size.unwrap_or(0);
        }
    }

    // Client
    if client_task.is_some() {
        if let Some(client) = &builder.client {
            total += client.size.unwrap_or(0);
        }
    }

    // Assets - match by URL
    if let Some(assets) = &builder.assets {
        for (url, _) in asset_tasks {
            if let Some(asset) = assets.objects.values().find(|a| a.url.as_ref() == Some(url)) {
                total += asset.size;
            }
        }
    }

    // Mods
    if let Some(mods) = &builder.mods {
        for (url, _) in mod_tasks {
            if let Some(_mod) = mods.iter().find(|m| m.url.as_ref() == Some(url)) {
                total += _mod.size.unwrap_or(0);
            }
        }
    }

    // Natives
    if let Some(natives) = &builder.natives {
        for (url, _) in native_download_tasks {
            if let Some(native) = natives.iter().find(|n| n.url.as_ref() == Some(url)) {
                total += native.size.unwrap_or(0);
            }
        }
    }

    total
}
