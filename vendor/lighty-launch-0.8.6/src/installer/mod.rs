// Copyright (c) 2025 Hamadi
// Licensed under the MIT License

pub mod installer;
pub mod config;
mod downloader;
mod verifier;
mod libraries;
mod mods;
mod natives;
mod client;
mod assets;

// Re-export the Installer trait
pub use installer::Installer;
