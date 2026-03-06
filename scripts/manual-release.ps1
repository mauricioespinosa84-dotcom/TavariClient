[CmdletBinding()]
param(
    [string]$Version,
    [string]$Repo = "mauricioespinosa84-dotcom/TavariClient",
    [string]$Notes = "Release manual de Tavari Client.",
    [string]$BundleDir = "src-tauri\\target\\x86_64-pc-windows-msvc\\release\\bundle\\nsis",
    [string]$Target = "x86_64-pc-windows-msvc",
    [string]$OutDir,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$rootDir = Split-Path -Parent $PSScriptRoot
$packageJsonPath = Join-Path $rootDir "package.json"

if (-not $Version) {
    $packageJson = Get-Content $packageJsonPath -Raw | ConvertFrom-Json
    $Version = "$($packageJson.version)".Trim()
}

if ([string]::IsNullOrWhiteSpace($Version)) {
    throw "No se pudo resolver la version del release."
}

$exePath = Join-Path $rootDir "src-tauri\\target\\release\\tavari-client.exe"
$targetExePath = Join-Path $rootDir "src-tauri\\target\\$Target\\release\\tavari-client.exe"
$knownNsisPath = "C:\Program Files (x86)\NSIS"

if ((Test-Path (Join-Path $knownNsisPath "makensis.exe")) -and -not ($env:PATH -split ';' | Where-Object { $_ -eq $knownNsisPath })) {
    $env:PATH = "$knownNsisPath;$env:PATH"
}

if (-not $SkipBuild) {
    $bundleRoot = Join-Path $rootDir "src-tauri\\target\\release\\bundle"
    $targetBundleRoot = Join-Path $rootDir "src-tauri\\target\\$Target\\release\\bundle"

    if (Test-Path $bundleRoot) {
        Remove-Item $bundleRoot -Recurse -Force
    }

    if (Test-Path $targetBundleRoot) {
        Remove-Item $targetBundleRoot -Recurse -Force
    }

    if (Test-Path $exePath) {
        Remove-Item $exePath -Force
    }

    if (Test-Path $targetExePath) {
        Remove-Item $targetExePath -Force
    }

    $privateKey = $env:TAURI_SIGNING_PRIVATE_KEY
    if ([string]::IsNullOrWhiteSpace($privateKey)) {
        $keyPath = Join-Path $HOME ".tauri\\tavari-client.key"
        if (Test-Path $keyPath) {
            $privateKey = Get-Content $keyPath -Raw
            $env:TAURI_SIGNING_PRIVATE_KEY = $privateKey
        }
    }

    if ([string]::IsNullOrWhiteSpace($env:TAURI_UPDATER_PUBKEY)) {
        $pubPath = Join-Path $HOME ".tauri\\tavari-client.key.pub"
        if (Test-Path $pubPath) {
            $env:TAURI_UPDATER_PUBKEY = Get-Content $pubPath -Raw
        }
    }

    if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY)) {
        throw "Falta TAURI_SIGNING_PRIVATE_KEY para compilar el release manual."
    }

    if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) {
        throw "Falta TAURI_SIGNING_PRIVATE_KEY_PASSWORD para compilar el release manual."
    }

    if ([string]::IsNullOrWhiteSpace($env:TAURI_UPDATER_PUBKEY)) {
        throw "Falta TAURI_UPDATER_PUBKEY para compilar el release manual."
    }

    Push-Location $rootDir
    try {
        npm run tauri -- build --no-sign --bundles nsis --target $Target
        if ($LASTEXITCODE -ne 0) {
            throw "El build manual fallo con codigo $LASTEXITCODE."
        }
    }
    finally {
        Pop-Location
    }
}

$bundlePath = Join-Path $rootDir $BundleDir
if (-not (Test-Path $bundlePath)) {
    throw "No existe la carpeta de bundle: $bundlePath"
}

$setup = Get-ChildItem -Path $bundlePath -Filter "*-setup.exe" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

if (-not $setup) {
    throw "No se encontro un instalador NSIS en $bundlePath"
}

$setupVersionInfo = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($setup.FullName)
$setupVersion = $setupVersionInfo.ProductVersion
if ([string]::IsNullOrWhiteSpace($setupVersion)) {
    $setupVersion = $setupVersionInfo.FileVersion
}

if ($setupVersion -ne $Version) {
    throw "El instalador generado tiene version '$setupVersion' y no coincide con '$Version'. No subas este release."
}

$signatureSource = "$($setup.FullName).sig"
if (-not (Test-Path $signatureSource)) {
    Push-Location $rootDir
    try {
        npm run tauri -- signer sign -- "$($setup.FullName)"
        if ($LASTEXITCODE -ne 0) {
            throw "La firma manual del instalador fallo con codigo $LASTEXITCODE."
        }
    }
    finally {
        Pop-Location
    }
}

if (-not (Test-Path $signatureSource)) {
    throw "No se encontro ni se pudo generar la firma del instalador: $signatureSource"
}

if (-not $OutDir) {
    $OutDir = Join-Path $rootDir ("release-manual\\" + $Version)
}

New-Item -ItemType Directory -Path $OutDir -Force | Out-Null

$assetName = "tavari-client_${Version}_windows_x86_64-setup.exe"
$assetPath = Join-Path $OutDir $assetName
$assetSignaturePath = "$assetPath.sig"
$latestPath = Join-Path $OutDir "latest.json"

Copy-Item $setup.FullName $assetPath -Force
Copy-Item $signatureSource $assetSignaturePath -Force

$signature = (Get-Content $assetSignaturePath -Raw).Trim()
$releaseUrl = "https://github.com/$Repo/releases/download/v$Version/$assetName"

$latestJson = [ordered]@{
    version = $Version
    notes = $Notes
    pub_date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    platforms = [ordered]@{
        "windows-x86_64" = [ordered]@{
            signature = $signature
            url = $releaseUrl
        }
    }
} | ConvertTo-Json -Depth 6

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($latestPath, "$latestJson`n", $utf8NoBom)

Write-Host ""
Write-Host "Release manual preparado."
Write-Host "Sube estos 3 archivos a GitHub Release v${Version}:"
Write-Host " - $assetPath"
Write-Host " - $assetSignaturePath"
Write-Host " - $latestPath"
Write-Host ""
Write-Host "Archivo para usuarios nuevos:"
Write-Host " - $assetPath"
Write-Host ""
Write-Host "Usuarios que ya tienen el launcher:"
Write-Host " - no necesitan el .sig ni latest.json; el launcher usa esos archivos desde GitHub para auto-actualizarse."
