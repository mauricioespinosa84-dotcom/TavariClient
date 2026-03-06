// Copyright (c) 2025 Hamadi
// Licensed under the MIT License

//! Launch builder for configuring game arguments and JVM options

use std::collections::{HashMap, HashSet};
use lighty_auth::UserProfile;
use lighty_java::JavaDistribution;
use crate::errors::InstallerResult;
use lighty_loaders::types::{VersionInfo, Loader, LoaderExtensions};
use crate::arguments::Arguments;
use crate::installer::Installer;

#[cfg(feature = "events")]
use lighty_event::EventBus;

/// Launch builder for configuring launch parameters
///
/// Created by calling `version.launch(&profile, java_distribution)`
pub struct LaunchBuilder<'a, T> {
    pub(crate) version: &'a mut T,
    pub(crate) profile: &'a UserProfile,
    pub(crate) java_distribution: JavaDistribution,
    pub(crate) jvm_overrides: HashMap<String, String>,
    pub(crate) jvm_removals: HashSet<String>,
    pub(crate) arg_overrides: HashMap<String, String>,
    pub(crate) arg_removals: HashSet<String>,
    pub(crate) raw_args: Vec<String>,
    #[cfg(feature = "events")]
    pub(crate) event_bus: Option<&'a EventBus>,
}

impl<'a, T> LaunchBuilder<'a, T>
where
    T: VersionInfo<LoaderType = Loader> + LoaderExtensions + Arguments + Installer,
{
    /// Create a new launch builder
    pub(crate) fn new(
        version: &'a mut T,
        profile: &'a UserProfile,
        java_distribution: JavaDistribution,
    ) -> Self {
        Self {
            version,
            profile,
            java_distribution,
            jvm_overrides: HashMap::new(),
            jvm_removals: HashSet::new(),
            arg_overrides: HashMap::new(),
            arg_removals: HashSet::new(),
            raw_args: Vec::new(),
            #[cfg(feature = "events")]
            event_bus: None,
        }
    }

    /// Set an event bus to receive download progress events
    ///
    /// # Example
    /// ```no_run
    /// let event_bus = EventBus::new(100);
    /// version.launch(&profile, JavaDistribution::Zulu)
    ///     .with_event_bus(&event_bus)
    ///     .run()
    ///     .await?;
    /// ```
    #[cfg(feature = "events")]
    pub fn with_event_bus(mut self, event_bus: &'a EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Configure JVM options
    ///
    /// # Example
    /// ```no_run
    /// version.launch(&profile, JavaDistribution::Zulu)
    ///     .with_jvm_options()
    ///         .set("Xmx", "4G")
    ///         .set("Xms", "2G")
    ///         .set("XX:+UseG1GC", "")
    ///         .done()
    ///     .run()
    ///     .await
    /// ```
    pub fn with_jvm_options(self) -> JvmOptionsBuilder<'a, T> {
        JvmOptionsBuilder {
            parent: self,
            overrides: HashMap::new(),
            removals: HashSet::new(),
        }
    }

    /// Configure game arguments
    ///
    /// # Example
    /// ```no_run
    /// version.launch(&profile, JavaDistribution::Zulu)
    ///     .with_arguments()
    ///         .set(KEY_WIDTH, "1920")
    ///         .set(KEY_HEIGHT, "1080")
    ///         .done()
    ///     .run()
    ///     .await
    /// ```
    pub fn with_arguments(self) -> ArgumentsBuilder<'a, T> {
        ArgumentsBuilder {
            parent: self,
            overrides: HashMap::new(),
            removals: HashSet::new(),
            raw_args: Vec::new(),
        }
    }

    /// Execute the launch
    ///
    /// # Example
    /// ```no_run
    /// version.launch(&profile, JavaDistribution::Zulu).run().await?;
    /// ```
    pub async fn run(self) -> InstallerResult<()> {
        crate::launch::execute_launch(
            self.version,
            self.profile,
            self.java_distribution,
            &self.jvm_overrides,
            &self.jvm_removals,
            &self.arg_overrides,
            &self.arg_removals,
            &self.raw_args,
            #[cfg(feature = "events")]
            self.event_bus,
        )
        .await
    }
}

/// JVM options builder
///
/// Configure JVM options like memory, garbage collection, etc.
pub struct JvmOptionsBuilder<'a, T> {
    parent: LaunchBuilder<'a, T>,
    overrides: HashMap<String, String>,
    removals: HashSet<String>,
}

impl<'a, T> JvmOptionsBuilder<'a, T>
where
    T: VersionInfo<LoaderType = Loader> + LoaderExtensions + Arguments + Installer,
{
    /// Set a JVM option
    ///
    /// The `-` prefix is added automatically based on the key format:
    /// - `Xmx`, `Xms` → `-Xmx`, `-Xms`
    /// - `XX:+UseG1GC` → `-XX:+UseG1GC`
    /// - `Djava.library.path` → `-Djava.library.path`
    ///
    /// # Arguments
    /// - `key`: JVM option key (without the `-` prefix)
    /// - `value`: Option value (empty string for flags)
    ///
    /// # Example
    /// ```no_run
    /// .set("Xmx", "4G")                        // → -Xmx4G
    /// .set("Xms", "2G")                        // → -Xms2G
    /// .set("XX:+UseG1GC", "")                  // → -XX:+UseG1GC
    /// .set("Djava.library.path", "/path")      // → -Djava.library.path=/path
    /// ```
    pub fn set(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.overrides.insert(key.into(), value.into());
        self
    }

    /// Remove a JVM option
    ///
    /// # Arguments
    /// - `key`: JVM option key to remove
    pub fn remove(mut self, key: impl Into<String>) -> Self {
        self.removals.insert(key.into());
        self
    }

    /// Finish configuring JVM options and return to the launch builder
    pub fn done(self) -> LaunchBuilder<'a, T> {
        let mut parent = self.parent;
        parent.jvm_overrides = self.overrides;
        parent.jvm_removals = self.removals;
        parent
    }
}

/// Game arguments builder
///
/// Configure game arguments like resolution, game directory, etc.
pub struct ArgumentsBuilder<'a, T> {
    parent: LaunchBuilder<'a, T>,
    overrides: HashMap<String, String>,
    removals: HashSet<String>,
    raw_args: Vec<String>,
}

impl<'a, T> ArgumentsBuilder<'a, T>
where
    T: VersionInfo<LoaderType = Loader> + LoaderExtensions + Arguments + Installer,
{
    /// Set a game argument or placeholder value
    ///
    /// This method intelligently handles two cases:
    /// - If the key is a known placeholder constant (like KEY_LAUNCHER_NAME), it overrides the placeholder value
    /// - Otherwise, it adds a raw argument with automatic `--` prefix
    ///
    /// # Arguments
    /// - `key`: Either a placeholder constant or a custom argument name
    /// - `value`: The value for the argument
    ///
    /// # Example
    /// ```no_run
    /// use lighty_launch::arguments::KEY_LAUNCHER_NAME;
    ///
    /// .set(KEY_LAUNCHER_NAME, "MyLauncher")   // Override ${launcher_name}
    /// .set("width", "1920")                   // Adds --width 1920
    /// .set("height", "1080")                  // Adds --height 1080
    /// .set("fullscreen", "")                  // Adds --fullscreen (no value)
    /// ```
    pub fn set(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key_str = key.into();
        let value_str = value.into();

        // Liste des clés de placeholders connues
        const KNOWN_PLACEHOLDERS: &[&str] = &[
            "auth_player_name", "auth_uuid", "auth_access_token", "auth_xuid",
            "clientid", "user_type", "user_properties",
            "version_name", "version_type",
            "game_directory", "assets_root", "natives_directory", "library_directory",
            "assets_index_name", "launcher_name", "launcher_version",
            "classpath", "classpath_separator",
        ];

        // Si c'est un placeholder connu, override
        if KNOWN_PLACEHOLDERS.contains(&key_str.as_str()) {
            self.overrides.insert(key_str, value_str);
        } else {
            // Sinon, ajouter comme argument brut avec préfixe --
            let formatted_arg = if key_str.starts_with("--") {
                key_str
            } else if key_str.starts_with('-') {
                format!("-{}", key_str)
            } else {
                format!("--{}", key_str)
            };

            self.raw_args.push(formatted_arg);

            if !value_str.is_empty() {
                self.raw_args.push(value_str);
            }
        }

        self
    }

    /// Remove a game argument
    ///
    /// # Arguments
    /// - `key`: Argument key to remove
    pub fn remove(mut self, key: impl Into<String>) -> Self {
        self.removals.insert(key.into());
        self
    }

    /// Finish configuring arguments and return to the launch builder
    pub fn done(self) -> LaunchBuilder<'a, T> {
        let mut parent = self.parent;
        parent.arg_overrides = self.overrides;
        parent.arg_removals = self.removals;
        parent.raw_args = self.raw_args;
        parent
    }
}

