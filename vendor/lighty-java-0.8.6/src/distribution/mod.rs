// Copyright (c) 2025 Hamadi
// Licensed under the MIT License

//! Java Distribution Management
//!
//! This module provides support for different Java distributions (Temurin, GraalVM, Zulu, Liberica).
//! The implementation is based on publicly documented APIs.

mod api_models;
mod providers;

use crate::errors::DistributionResult;
use crate::JavaDistribution;

/// Gets download URL for the distribution
///
/// Queries the respective API or builds direct download URLs for each distribution.
pub(crate) async fn get_download_url(distribution: &JavaDistribution, jre_version: &u8) -> DistributionResult<String> {
    match distribution {
        JavaDistribution::Temurin => providers::build_temurin_url(jre_version),
        JavaDistribution::GraalVM => providers::build_graalvm_url(jre_version),
        JavaDistribution::Zulu => providers::build_zulu_url(jre_version).await,
        JavaDistribution::Liberica => providers::build_liberica_url(jre_version).await,
    }
}
