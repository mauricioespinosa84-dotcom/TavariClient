// Copyright (c) 2025 Hamadi
// Licensed under the MIT License

//! API response structures for different Java distribution providers
//!
//! These structures are minimal and only deserialize the fields we need,
//! ignoring all other fields from the API responses for better performance.

use serde::Deserialize;

/// Zulu API response structure
/// Only deserializes the download_url field, ignoring all other fields
#[derive(Debug, Deserialize)]
pub(super) struct ZuluPackage {
    pub download_url: String,
    // All other fields are ignored automatically by serde
}

/// Foojay API response structure (for Liberica)
/// Only deserializes the result array
#[derive(Debug, Deserialize)]
pub(super) struct FoojayResponse {
    pub result: Vec<FoojayPackage>,
    // Other top-level fields are ignored
}

/// Individual package in Foojay response
/// Only deserializes the links object
#[derive(Debug, Deserialize)]
pub(super) struct FoojayPackage {
    pub links: FoojayLinks,
    // Other package fields are ignored
}

/// Links object in Foojay package
/// Only deserializes the download redirect URL
#[derive(Debug, Deserialize)]
pub(super) struct FoojayLinks {
    pub pkg_download_redirect: String,
    // Other link fields are ignored
}
