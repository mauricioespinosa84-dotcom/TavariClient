// Copyright (c) 2025 Hamadi
// Licensed under the MIT License

//! Lighty Java - Java Runtime Management
//!
//! This crate provides functionality for managing Java Runtime Environments (JRE)
//! including downloading, installing, and executing Java processes.
//!
//! ## Features
//! - Support for multiple Java distributions (Temurin, GraalVM, Zulu, Liberica)
//! - Cross-platform JRE download and installation
//! - Java process execution with I/O streaming
//! - File size verification for download integrity
//!
//! ## License
//! This implementation is original work licensed under MIT.
//! It does not derive from GPL-licensed code.
//!
//! ## Clean Room Implementation
//! The distribution management system was implemented from scratch using only
//! publicly documented APIs from Adoptium, Oracle, Azul, and Foojay.

mod distribution;
pub mod jre_downloader;
pub mod runtime;
pub mod errors;

use serde::{Deserialize, Serialize};

pub use errors::{
    JreError, JreResult,
    JavaRuntimeError, JavaRuntimeResult,
    DistributionError, DistributionResult,
};

// ============================================================================
// Public Types
// ============================================================================

/// Selection method for Java distribution
#[derive(Deserialize, Serialize, Clone)]
#[serde(tag = "type", content = "value")]
pub enum DistributionSelection {
    #[serde(rename = "automatic")]
    Automatic(String),
    #[serde(rename = "custom")]
    Custom(String),
    #[serde(rename = "manual")]
    Manual(JavaDistribution),
}

impl Default for DistributionSelection {
    fn default() -> Self {
        DistributionSelection::Automatic(String::new())
    }
}

/// Available Java distributions
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub enum JavaDistribution {
    #[serde(rename = "temurin")]
    Temurin,
    #[serde(rename = "graalvm")]
    GraalVM,
    #[serde(rename = "zulu")]
    Zulu,
    #[serde(rename = "liberica")]
    Liberica,
}

impl Default for JavaDistribution {
    fn default() -> Self {
        // Temurin is the default as it supports all Java versions
        JavaDistribution::Temurin
    }
}

impl JavaDistribution {
    /// Returns the canonical name of this distribution
    pub fn get_name(&self) -> &str {
        match self {
            JavaDistribution::Temurin => "temurin",
            JavaDistribution::GraalVM => "graalvm",
            JavaDistribution::Zulu => "zulu",
            JavaDistribution::Liberica => "liberica",
        }
    }

    /// Checks if this distribution supports the given Java version
    pub fn supports_version(&self, version: u8) -> bool {
        match self {
            JavaDistribution::Temurin => true, // Supports all versions (8, 11, 17, 21, etc.)
            JavaDistribution::GraalVM => version >= 17, // Only 17+ (JDK only, no JRE)
            JavaDistribution::Zulu => true, // Supports all versions
            JavaDistribution::Liberica => true, // Supports all versions
        }
    }

    /// Gets download URL for the distribution
    ///
    /// Queries the respective API or builds direct download URLs for each distribution.
    pub async fn get_download_url(&self, jre_version: &u8) -> DistributionResult<String> {
        distribution::get_download_url(self, jre_version).await
    }

}