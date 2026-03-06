// Copyright (c) 2025 Hamadi
// Licensed under the MIT License

//! Global launch configuration
//!
//! Configure username, UUID, and Java distribution globally instead of passing them to each launch call.

use once_cell::sync::OnceCell;
use lighty_java::JavaDistribution;

/// Launch configuration
///
/// Configure these parameters once and reuse them across all launches.
#[derive(Debug, Clone)]
pub struct LaunchConfig {
    /// Username for authentication
    pub username: String,

    /// Player UUID (with dashes)
    pub uuid: String,

    /// Java distribution to use
    pub java_distribution: JavaDistribution,
}

impl LaunchConfig {
    /// Create a new launch configuration
    ///
    /// # Arguments
    /// - `username`: Player username
    /// - `uuid`: Player UUID (format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)
    /// - `java_distribution`: Java distribution to download/use
    pub fn new(
        username: impl Into<String>,
        uuid: impl Into<String>,
        java_distribution: JavaDistribution,
    ) -> Self {
        Self {
            username: username.into(),
            uuid: uuid.into(),
            java_distribution,
        }
    }
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            username: "Hamadi".to_string(),
            uuid: "00000000-0000-0000-0000-000000000000".to_string(),
            java_distribution: JavaDistribution::Temurin,
        }
    }
}

static LAUNCH_CONFIG: OnceCell<LaunchConfig> = OnceCell::new();

/// Initialize the global launch configuration
///
/// This function must be called before using `launch()`.
/// If not called, default values will be used.
///
/// # Arguments
/// - `config`: Launch configuration
///
/// # Example
/// ```no_run
/// use lighty_launch::launch_config::{init_launch_config, LaunchConfig};
/// use lighty_java::JavaDistribution;
///
/// init_launch_config(LaunchConfig::new(
///     "Steve",
///     "12345678-1234-5678-1234-567812345678",
///     JavaDistribution::Zulu
/// ));
/// ```
pub fn init_launch_config(config: LaunchConfig) {
    LAUNCH_CONFIG.set(config).ok();
}

/// Get the current launch configuration
///
/// If not initialized via `init_launch_config()`, returns default values.
pub(crate) fn get_config() -> LaunchConfig {
    LAUNCH_CONFIG.get_or_init(LaunchConfig::default).clone()
}
