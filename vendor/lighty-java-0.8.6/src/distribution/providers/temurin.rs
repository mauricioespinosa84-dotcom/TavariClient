// Copyright (c) 2025 Hamadi
// Licensed under the MIT License

//! Adoptium Temurin distribution provider
//!
//! API Documentation: https://api.adoptium.net/q/swagger-ui/

use crate::errors::DistributionResult;
use lighty_core::system::{ARCHITECTURE, OS};

/// Builds Adoptium Temurin download URL
///
/// Temurin provides comprehensive JRE support for all Java versions
pub fn build_temurin_url(version: &u8) -> DistributionResult<String> {
    let os_name = OS.get_adoptium_name()?;
    let arch = ARCHITECTURE.get_simple_name()?;

    Ok(format!(
        "https://api.adoptium.net/v3/binary/latest/{}/ga/{}/{}/jre/hotspot/normal/eclipse?project=jdk",
        version, os_name, arch
    ))
}
