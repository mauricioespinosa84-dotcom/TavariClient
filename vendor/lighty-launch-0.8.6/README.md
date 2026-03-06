# lighty-launch

Minecraft launch logic for [LightyLauncher](https://crates.io/crates/lighty-launcher).

## Note

This is an internal crate for the LightyLauncher ecosystem. Most users should use the main [`lighty-launcher`](https://crates.io/crates/lighty-launcher) crate instead.

## Features

- **Game Launching**: Launch Minecraft with proper arguments
- **Asset Installation**: Download and install game assets and libraries
- **JVM Arguments**: Generate optimized JVM arguments
- **Process Management**: Manage Minecraft process lifecycle

## Usage

```toml
[dependencies]
lighty-launch = "0.6.3"
```

```rust
use lighty_launch::launch::Launch;
use lighty_java::JavaDistribution;

#[tokio::main]
async fn main() {
    // Assuming you have a Version object from lighty-loaders
    let mut version = /* ... */;

    // Launch the game
    version.launch(
        "PlayerName",
        "player-uuid",
        JavaDistribution::Temurin
    ).await?;
}
```

## Structure

```
lighty-launch/
└── src/
    ├── lib.rs          # Module declarations
    ├── launch.rs       # Launch trait and implementation
    ├── installer/      # Assets and libraries installation
    │   ├── mod.rs      # Installer trait
    │   ├── assets.rs   # Assets installation
    │   └── libraries.rs # Libraries installation
    ├── arguments.rs    # JVM and game arguments generation
    └── errors.rs       # Error types (InstallerError, InstallerResult)
```

## Components

### Launch Trait

The `Launch` trait defines the interface for launching Minecraft:

```rust
use async_trait::async_trait;
use lighty_java::JavaDistribution;
use lighty_launch::errors::InstallerResult;

#[async_trait]
pub trait Launch {
    async fn launch(
        &mut self,
        username: &str,
        uuid: &str,
        java_distribution: JavaDistribution
    ) -> InstallerResult<()>;
}
```

### Installer Trait

Handles downloading and installing game files (assets, libraries):

```rust
use lighty_launch::installer::Installer;

#[async_trait]
pub trait Installer {
    async fn install(&self, builder: &VersionBuilder) -> InstallerResult<()>;
}
```

### Arguments

Generates optimized JVM and game arguments:

```rust
use lighty_launch::arguments::Arguments;

let args = Arguments::new(&version_metadata, &java_path);
let jvm_args = args.get_jvm_arguments();
let game_args = args.get_game_arguments("PlayerName", "uuid");
```

**Features**:
- Memory optimization based on system RAM
- Platform-specific arguments
- Library path resolution
- Asset index handling

## Error Handling

All operations return `InstallerResult<T>` with detailed error information:

```rust
use lighty_launch::errors::{InstallerError, InstallerResult};

match version.launch(username, uuid, java_dist).await {
    Ok(()) => println!("Game launched successfully"),
    Err(InstallerError::DownloadFailed(url)) => {
        eprintln!("Failed to download: {}", url);
    }
    Err(e) => eprintln!("Launch error: {:?}", e),
}
```

## License

MIT

## Links

- **Main Package**: [lighty-launcher](https://crates.io/crates/lighty-launcher)
- **Repository**: [GitHub](https://github.com/Lighty-Launcher/LightyLauncherLib)
- **Documentation**: [docs.rs/lighty-launch](https://docs.rs/lighty-launch)
