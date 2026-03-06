// Copyright (c) 2025 Hamadi
// Licensed under the MIT License

//! Azul Zulu distribution provider
//!
//! API Documentation: https://api.azul.com/metadata/v1/docs/

use crate::errors::{DistributionError, DistributionResult};
use crate::distribution::api_models::ZuluPackage;
use lighty_core::system::{ARCHITECTURE, OS};
use lighty_core::hosts::HTTP_CLIENT;

/// Builds Zulu download URL using their API
///
/// Queries the Azul API to get the latest JRE package for the specified version
pub async fn build_zulu_url(version: &u8) -> DistributionResult<String> {
    let os_name = OS.get_zulu_name()?;
    let arch_name = ARCHITECTURE.get_zulu_arch()?;
    let ext = OS.get_zulu_ext()?;

    // Build API URL to get latest JRE package
    let api_url = format!(
        "https://api.azul.com/metadata/v1/zulu/packages?os={}&arch={}&archive_type={}&java_package_type=jre&release_status=ga&java_version={}&latest=true",
        os_name, arch_name, ext, version
    );

    // Fetch from API
    let response = HTTP_CLIENT
        .get(&api_url)
        .header("User-Agent", "Lighty-Launcher-Rust")
        .send()
        .await
        .map_err(|e| DistributionError::ApiError {
            distribution: "Zulu",
            error: e.to_string(),
        })?;

    let packages: Vec<ZuluPackage> = response
        .json()
        .await
        .map_err(|e| DistributionError::JsonParseError {
            distribution: "Zulu",
            error: e.to_string(),
        })?;

    // Take the first (latest) package without cloning
    packages
        .into_iter()
        .next()
        .map(|pkg| pkg.download_url)
        .ok_or(DistributionError::NoPackagesFound {
            distribution: "Zulu",
        })
}
