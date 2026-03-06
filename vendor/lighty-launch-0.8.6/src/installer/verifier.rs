// Copyright (c) 2025 Hamadi
// Licensed under the MIT License

//! File verification and cache checking utilities

use std::path::PathBuf;
use tokio::fs;
use lighty_core::verify_file_sha1;

/// Verifies if a file exists and matches the expected SHA1 hash
///
/// Returns true if the file needs to be downloaded
pub async fn needs_download(path: &PathBuf, sha1: Option<&String>, name: &str) -> bool {
    if !path.exists() {
        return true;
    }

    if let Some(hash) = sha1 {
        match verify_file_sha1(path, hash).await {
            Ok(true) => false,
            _ => {
                lighty_core::trace_warn!("[Installer] SHA1 mismatch for {}, re-downloading...", name);
                let _ = fs::remove_file(path).await;
                true
            }
        }
    } else {
        false
    }
}
