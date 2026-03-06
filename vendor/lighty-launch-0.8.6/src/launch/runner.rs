use lighty_core::time_it;
use lighty_java::jre_downloader::jre_download;
use lighty_java::{JavaDistribution, JreError};
use lighty_loaders::types::version_metadata::Version;
use crate::errors::{InstallerError, InstallerResult};
use crate::installer::Installer;
use super::builder::LaunchBuilder;
use lighty_loaders::types::{Loader, LoaderExtensions, VersionInfo};
use lighty_auth::UserProfile;
use std::sync::Arc;
use std::path::PathBuf;
use lighty_loaders::types::version_metadata::VersionMetaData;
use tokio::sync::oneshot;
use lighty_java::jre_downloader::find_java_binary;
use lighty_java::runtime::JavaRuntime;
use crate::arguments::Arguments;
use std::collections::{HashMap,HashSet};

#[cfg(feature = "events")]
use lighty_event::EventBus;

pub trait Launch {
    /// Launch the game with a builder pattern
    ///
    /// # Arguments
    /// - `profile`: User profile from authentication
    /// - `java_distribution`: Java distribution to use
    ///
    /// # Returns
    /// A `LaunchBuilder` for configuring JVM options and game arguments
    ///
    /// # Example
    /// ```no_run
    /// // Simple launch
    /// version.launch(&profile, JavaDistribution::Zulu).await?;
    ///
    /// // With custom options
    /// version.launch(&profile, JavaDistribution::Zulu)
    ///     .with_jvm_options()
    ///         .set("Xmx", "4G")
    ///         .done()
    ///     .with_arguments()
    ///         .set(KEY_WIDTH, "1920")
    ///         .done()
    ///     .await?;
    /// ```
    fn launch<'a>(&'a mut self, profile: &'a UserProfile, java_distribution: JavaDistribution) -> LaunchBuilder<'a, Self>
    where
        Self: Sized;
}

// Implémentation générique pour tout type implémentant VersionInfo + les traits nécessaires
impl<T> Launch for T
where
    T: VersionInfo<LoaderType = Loader> + LoaderExtensions + Arguments + Installer,
{
    fn launch<'a>(&'a mut self, profile: &'a UserProfile, java_distribution: JavaDistribution) -> LaunchBuilder<'a, Self> {
        LaunchBuilder::new(self, profile, java_distribution)
    }
}

/// Internal function to execute the launch process
pub(crate) async fn execute_launch<T>(
    version: &mut T,
    profile: &UserProfile,
    java_distribution: JavaDistribution,
    jvm_overrides: &std::collections::HashMap<String, String>,
    jvm_removals: &std::collections::HashSet<String>,
    arg_overrides: &std::collections::HashMap<String, String>,
    arg_removals: &std::collections::HashSet<String>,
    raw_args: &[String],
    #[cfg(feature = "events")] event_bus: Option<&EventBus>,
) -> InstallerResult<()>
where
    T: VersionInfo<LoaderType = Loader> + LoaderExtensions + Arguments + Installer,
{
        let username = &profile.username;
        let uuid = &profile.uuid;
        // 1. Préparer les métadonnées du loader
        let metadata = prepare_metadata(
            version,
            #[cfg(feature = "events")]
            event_bus,
        ).await?;

        let version_data = extract_version(&metadata)?;

        // 2. S'assurer que Java est installé
        let java_path = ensure_java_installed(
            version,
            version_data,
            &java_distribution,
            #[cfg(feature = "events")]
            event_bus,
        ).await?;

        // 3. Installer les dépendances Minecraft
        time_it!("Install delay", version.install(
            version_data,
            #[cfg(feature = "events")]
            event_bus,
        ).await?);

        // 4. Lancer le jeu
        execute_game(version, version_data, username, uuid, java_path, arg_overrides, arg_removals, jvm_overrides, jvm_removals, raw_args).await
}

/// Récupère les métadonnées complètes du loader
async fn prepare_metadata<T>(
    builder: &mut T,
    #[cfg(feature = "events")] event_bus: Option<&EventBus>,
) -> InstallerResult<Arc<VersionMetaData>>
where
    T: VersionInfo<LoaderType = Loader> + LoaderExtensions,
{
    lighty_core::trace_debug!("[Launch] Fetching metadata for loader: {:?}", builder.loader());


    let loader_name = format!("{:?}", builder.loader());

    #[cfg(feature = "events")]
    if let Some(bus) = event_bus {
        bus.emit(lighty_event::Event::Loader(lighty_event::LoaderEvent::FetchingData {
            loader: loader_name.clone(),
            minecraft_version: builder.minecraft_version().to_string(),
            loader_version: builder.loader_version().to_string(),
        }));
    }

    let metadata = match builder.loader() {
        Loader::Vanilla => builder.get_complete().await?,
        Loader::Fabric => builder.get_fabric_complete().await?,
        Loader::Quilt => builder.get_quilt_complete().await?,
        Loader::NeoForge => builder.get_neoforge_complete().await?,
        Loader::LightyUpdater => builder.get_lighty_updater_complete().await?,
        _ => return Err(InstallerError::UnsupportedLoader(format!("{:?}", builder.loader()))),
    };

    #[cfg(feature = "events")]
    if let Some(bus) = event_bus {
        bus.emit(lighty_event::Event::Loader(lighty_event::LoaderEvent::DataFetched {
            loader: loader_name,
            minecraft_version: builder.minecraft_version().to_string(),
            loader_version: builder.loader_version().to_string(),
        }));
    }

    lighty_core::trace_info!("[Launch] Metadata fetched successfully for {:?}", builder.loader());
    Ok(metadata)
}

/// S'assure que Java est installé et retourne le chemin vers l'exécutable
async fn ensure_java_installed<T>(
    builder: &T,
    version: &Version,
    java_distribution: &JavaDistribution,
    #[cfg(feature = "events")] event_bus: Option<&EventBus>,
) -> InstallerResult<PathBuf>
where
    T: VersionInfo,
{
    let java_version = version.java_version.major_version;

    // Vérifier si Java est déjà installé
    match find_java_binary(builder.java_dirs(), java_distribution, &java_version).await {
        Ok(path) => {
            lighty_core::trace_info!("[Java] Java {} already installed at: {:?}", java_version, path);

            #[cfg(feature = "events")]
            if let Some(bus) = event_bus {
                bus.emit(lighty_event::Event::Java(lighty_event::JavaEvent::JavaAlreadyInstalled {
                    distribution: java_distribution.get_name().to_string(),
                    version: java_version,
                    binary_path: path.to_string_lossy().to_string(),
                }));
            }

            Ok(path)
        }
        Err(_) => {
            lighty_core::trace_info!("[Java] Java {} not found, downloading...", java_version);

            #[cfg(feature = "events")]
            if let Some(bus) = event_bus {
                bus.emit(lighty_event::Event::Java(lighty_event::JavaEvent::JavaNotFound {
                    distribution: java_distribution.get_name().to_string(),
                    version: java_version,
                }));
            }

            #[cfg(feature = "events")]
            let path = jre_download(
                builder.java_dirs(),
                java_distribution,
                &java_version,
                |current, total| {
                    lighty_core::trace_debug!("[Java] Download progress: {}/{}", current, total);
                },
                event_bus,
            ).await.map_err(|e| InstallerError::DownloadFailed(format!("JRE download failed: {}", e)))?;

            #[cfg(not(feature = "events"))]
            let path = jre_download(
                builder.java_dirs(),
                java_distribution,
                &java_version,
                |current, total| {
                    lighty_core::trace_debug!("[Java] Download progress: {}/{}", current, total);
                },
            ).await.map_err(|e : JreError | InstallerError::DownloadFailed(format!("JRE download failed: {}", e)))?;

            lighty_core::trace_info!("[Java] Java {} installed successfully", java_version);
            Ok(path)
        }
    }
}

/// Lance le jeu avec les arguments appropriés
async fn execute_game<T>(
    builder: &T,
    version: &Version,
    username: &str,
    uuid: &str,
    java_path: PathBuf,
    arg_overrides: &HashMap<String, String>,
    arg_removals: &HashSet<String>,
    jvm_overrides: &HashMap<String, String>,
    jvm_removals: &HashSet<String>,
    raw_args: &[String],
) -> InstallerResult<()>
where
    T: VersionInfo + Arguments,
{
    // Construire les arguments
    let arguments = builder.build_arguments(version, username, uuid, arg_overrides, arg_removals, jvm_overrides, jvm_removals, raw_args);
    
    lighty_core::trace_debug!("[Launch] Launch arguments: {:?}", arguments);

    // Créer JavaRuntime avec le chemin vers java.exe
    let java_runtime = JavaRuntime::new(java_path);
    lighty_core::trace_info!("[Launch] Executing game...");

    match java_runtime.execute(arguments, builder.game_dirs()).await {
        Ok(mut child) => {
            let (_tx, rx) = oneshot::channel::<()>();

            if let Some(pid) = child.id() {
                lighty_core::trace_info!("[Launch] Game launched successfully, PID: {}", pid);
            } else {
                lighty_core::trace_info!("[Launch] Game launched successfully, PID unavailable");
            }

            // Affiche les logs Java en temps réel dans le terminal
            let print_output = |_: &(), buf: &[u8]| -> lighty_java::JavaRuntimeResult<()> {
                print!("{}", String::from_utf8_lossy(buf));
                Ok(())
            };

            if let Err(e) = java_runtime
                .handle_io(&mut child, print_output, print_output, rx, &())
                .await
            {
                lighty_core::trace_error!("[Launch] IO error: {}", e);
                return Err(InstallerError::DownloadFailed(format!("Launch failed: {}", e)));
            }



            // tx.send(()); // <- à utiliser si tu veux forcer l'arrêt du process plus tard
            Ok(())
        }
        Err(e) => {
            lighty_core::trace_error!("[Launch] Failed to launch game: {}", e);
            Err(InstallerError::DownloadFailed(format!("Launch failed: {}", e)))
        }
    }
}

/// Extrait l'objet Version depuis VersionMetaData
fn extract_version(metadata: &VersionMetaData) -> InstallerResult<&Version> {
    match metadata {
        VersionMetaData::Version(v) => Ok(v),
        _ => Err(InstallerError::InvalidMetadata),
    }
}
