# Tavari Client

Launcher nuevo en Rust + Tauri para Tavari Studios.

## Objetivo

- Login premium y no premium al abrir.
- Consumir `launcher-backend` para cargar configuracion e instancias.
- Sincronizar mods/config por manifest.
- Lanzar Minecraft desde Tauri usando un core Rust.

## Backend esperado

Por defecto usa:

- Local: `C:\Users\mauri\OneDrive\Documents\GitHub\launcher-backend`
- Remoto: `https://mauricioespinosa84-dotcom.github.io/launcher-backend/`

## Scripts

- `npm run dev`
- `npm run build`
- `npm run rust:check`
- `npm run rust:fmt`
- `npm run release:version -- 1.0.1`

## Releases

- Workflow automatico: [.github/workflows/release.yml](./.github/workflows/release.yml)
- Guia completa de updater y GitHub Releases: [docs/github-releases.md](./docs/github-releases.md)
