# lighty-java

Java runtime management for [LightyLauncher](https://crates.io/crates/lighty-launcher).

## Note

This is an internal crate for the LightyLauncher ecosystem. Most users should use the main [`lighty-launcher`](https://crates.io/crates/lighty-launcher) crate instead.

## Features

- **Automatic JRE Download**: Download and install Java runtimes on demand
- **Multiple Distributions**: Support for Temurin, GraalVM, Zulu, and Liberica
- **Version Detection**: Detect required Java version for Minecraft
- **Cross-Platform**: Windows, Linux, and macOS support

## Usage

```toml
[dependencies]
lighty-java = "0.6.3"
```

```rust
use lighty_java::{JavaDistribution, jre_downloader};
use directories::ProjectDirs;

#[tokio::main]
async fn main() {
    let launcher_dir = ProjectDirs::from("com", "MyLauncher", "").unwrap();

    // Download Java 21 (Temurin)
    let java_path = jre_downloader::jre_download(
        21,
        JavaDistribution::Temurin,
        &launcher_dir
    ).await?;

    println!("Java installed at: {}", java_path.display());
}
```

## Structure

```
lighty-java/
└── src/
    ├── lib.rs              # Module declarations and re-exports
    ├── distribution/       # Java distribution providers
    │   ├── mod.rs          # Distribution types and main API
    │   ├── api_models.rs   # API response structures
    │   ├── utils.rs        # Shared utilities
    │   └── providers/      # Individual distribution providers
    │       ├── temurin.rs  # Adoptium Temurin provider
    │       ├── graalvm.rs  # Oracle GraalVM provider
    │       ├── zulu.rs     # Azul Zulu provider
    │       └── liberica.rs # BellSoft Liberica provider
    ├── jre_downloader.rs   # Download and install JRE
    ├── runtime.rs          # Java version detection and validation
    └── errors.rs           # Error types for Java operations
```

## Supported Distributions

### Temurin (Recommended)

Eclipse Temurin - OpenJDK builds from the Adoptium project.

```rust
use lighty_java::JavaDistribution;

let distribution = JavaDistribution::Temurin;
```

**Supported Versions**: 8, 11, 17, 21
**Best for**: General use, maximum compatibility

### GraalVM

GraalVM - High-performance JDK with advanced optimizations.

```rust
use lighty_java::JavaDistribution;

let distribution = JavaDistribution::GraalVM;
```

**Supported Versions**: 17, 21
**Best for**: Modern Minecraft versions (1.17+), maximum performance

### Zulu

Azul Zulu - OpenJDK builds with enterprise support available.

```rust
use lighty_java::JavaDistribution;

let distribution = JavaDistribution::Zulu;
```

**Supported Versions**: 8, 11, 17, 21
**Best for**: Enterprise environments, certified builds

### Liberica

BellSoft Liberica - Lightweight OpenJDK distribution.

```rust
use lighty_java::JavaDistribution;

let distribution = JavaDistribution::Liberica;
```

**Supported Versions**: 8, 11, 17, 21
**Best for**: Resource-constrained systems, lightweight deployments

## Platform Support

| Platform | Architectures | Status |
|----------|---------------|--------|
| Windows  | x64, ARM64    | Tested |
| Linux    | x64, ARM64    | Tested |
| macOS    | x64, ARM64    | Tested |

## License

MIT

## Links

- **Main Package**: [lighty-launcher](https://crates.io/crates/lighty-launcher)
- **Repository**: [GitHub](https://github.com/Lighty-Launcher/LightyLauncherLib)
- **Documentation**: [docs.rs/lighty-java](https://docs.rs/lighty-java)
