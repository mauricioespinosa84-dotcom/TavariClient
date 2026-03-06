// Copyright (c) 2025 Hamadi
// Licensed under the MIT License

use lighty_loaders::types::version_metadata::Version;
use lighty_loaders::types::VersionInfo;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

// Constantes publiques pour les clés de HashMap
pub const KEY_AUTH_PLAYER_NAME: &str = "auth_player_name";
pub const KEY_AUTH_UUID: &str = "auth_uuid";
pub const KEY_AUTH_ACCESS_TOKEN: &str = "auth_access_token";
pub const KEY_AUTH_XUID: &str = "auth_xuid";
pub const KEY_CLIENT_ID: &str = "clientid";
pub const KEY_USER_TYPE: &str = "user_type";
pub const KEY_USER_PROPERTIES: &str = "user_properties";
pub const KEY_VERSION_NAME: &str = "version_name";
pub const KEY_VERSION_TYPE: &str = "version_type";
pub const KEY_GAME_DIRECTORY: &str = "game_directory";
pub const KEY_ASSETS_ROOT: &str = "assets_root";
pub const KEY_NATIVES_DIRECTORY: &str = "natives_directory";
pub const KEY_LIBRARY_DIRECTORY: &str = "library_directory";
pub const KEY_ASSETS_INDEX_NAME: &str = "assets_index_name";
pub const KEY_LAUNCHER_NAME: &str = "launcher_name";
pub const KEY_LAUNCHER_VERSION: &str = "launcher_version";
pub const KEY_CLASSPATH: &str = "classpath";
pub const KEY_CLASSPATH_SEPARATOR: &str = "classpath_separator";

// Constantes pour les valeurs
const DEFAULT_ACCESS_TOKEN: &str = "0";
const DEFAULT_XUID: &str = "0";
const DEFAULT_CLIENT_ID: &str = "{client-id}";
const DEFAULT_USER_TYPE: &str = "legacy";
const DEFAULT_USER_PROPERTIES: &str = "{}";
const DEFAULT_VERSION_TYPE: &str = "release";
const LAUNCHER_NAME: &str = "LightyLauncher";
const LAUNCHER_VERSION: &str = "1.0.0";
const CP_FLAG: &str = "-cp";

pub trait Arguments {
    fn build_arguments(
        &self,
        builder: &Version,
        username: &str,
        uuid: &str,
        arg_overrides: &HashMap<String, String>,
        arg_removals: &HashSet<String>,
        jvm_overrides: &HashMap<String, String>,
        jvm_removals: &HashSet<String>,
        raw_args: &[String],
    ) -> Vec<String>;
}

impl<T: VersionInfo> Arguments for T {
    fn build_arguments(
        &self,
        builder: &Version,
        username: &str,
        uuid: &str,
        arg_overrides: &HashMap<String, String>,
        arg_removals: &HashSet<String>,
        jvm_overrides: &HashMap<String, String>,
        jvm_removals: &HashSet<String>,
        raw_args: &[String],
    ) -> Vec<String> {
        // Créer la HashMap avec toutes les variables
        let mut variables = create_variable_map(self, builder, username, uuid);

        // Appliquer les overrides sur les variables
        for (key, value) in arg_overrides {
            variables.insert(key.clone(), value.clone());
        }

        // Remplacer les variables dans les arguments
        let game_args = replace_variables_in_vec(&builder.arguments.game, &variables);

        let mut jvm_args = builder.arguments.jvm
            .as_ref()
            .map(|jvm| replace_variables_in_vec(jvm, &variables))
            .unwrap_or_else(|| build_default_jvm_args(&variables));

        // S'assurer que les arguments JVM critiques sont toujours présents

        // 1. java.library.path (pour les natives LWJGL)
        if !jvm_args.iter().any(|arg| arg.starts_with("-Djava.library.path=")) {
            let natives_dir = variables.get(KEY_NATIVES_DIRECTORY).cloned().unwrap_or_default();
            jvm_args.insert(0, format!("-Djava.library.path={}", natives_dir));
        }

        // 2. Launcher brand et version
        if !jvm_args.iter().any(|arg| arg.starts_with("-Dminecraft.launcher.brand=")) {
            let launcher_name = variables.get(KEY_LAUNCHER_NAME).cloned().unwrap_or_default();
            jvm_args.insert(0, format!("-Dminecraft.launcher.brand={}", launcher_name));
        }

        if !jvm_args.iter().any(|arg| arg.starts_with("-Dminecraft.launcher.version=")) {
            let launcher_version = variables.get(KEY_LAUNCHER_VERSION).cloned().unwrap_or_default();
            jvm_args.insert(0, format!("-Dminecraft.launcher.version={}", launcher_version));
        }

        // 3. Classpath (doit être en dernier avant la mainClass)
        if !jvm_args.contains(&CP_FLAG.to_string()) {
            let classpath = variables.get(KEY_CLASSPATH).cloned().unwrap_or_default();
            jvm_args.push(CP_FLAG.into());
            jvm_args.push(classpath);
        }

        // 4. Appliquer les JVM overrides
        apply_jvm_overrides(&mut jvm_args, jvm_overrides);

        // 5. Appliquer les JVM removals
        apply_jvm_removals(&mut jvm_args, jvm_removals);

        // 6. Appliquer les arg removals (filtrer les arguments de jeu)
        let game_args = apply_arg_removals(game_args, arg_removals);

        // Construire le Vec complet : JVM + MainClass + Game + Raw Args
        let mut full_args = jvm_args;
        full_args.push(builder.main_class.main_class.clone());
        full_args.extend(game_args);

        // Ajouter les arguments bruts à la fin
        full_args.extend_from_slice(raw_args);

        lighty_core::trace_debug!(args = ?full_args, "Launch arguments built");

        full_args
    }
}

/// Crée la HashMap avec toutes les variables de lancement
fn create_variable_map<T: VersionInfo>(
    version: &T,
    builder: &Version,
    username: &str,
    uuid: &str,
) -> HashMap<String, String> {
        let mut map = HashMap::new();

        #[cfg(target_os = "windows")]
        let classpath_separator = ";";
        #[cfg(not(target_os = "windows"))]
        let classpath_separator = ":";

        // Authentification
        map.insert(KEY_AUTH_PLAYER_NAME.into(), username.into());
        map.insert(KEY_AUTH_UUID.into(), uuid.into());
        map.insert(KEY_AUTH_ACCESS_TOKEN.into(), DEFAULT_ACCESS_TOKEN.into());
        map.insert(KEY_AUTH_XUID.into(), DEFAULT_XUID.into());
        map.insert(KEY_CLIENT_ID.into(), DEFAULT_CLIENT_ID.into());
        map.insert(KEY_USER_TYPE.into(), DEFAULT_USER_TYPE.into());
        map.insert(KEY_USER_PROPERTIES.into(), DEFAULT_USER_PROPERTIES.into());

        // Version
        map.insert(KEY_VERSION_NAME.into(), version.name().into());
        map.insert(KEY_VERSION_TYPE.into(), DEFAULT_VERSION_TYPE.into());

        // Directories
        map.insert(KEY_GAME_DIRECTORY.into(), version.game_dirs().join("runtime").display().to_string());
        map.insert(KEY_ASSETS_ROOT.into(), version.game_dirs().join("assets").display().to_string());
        map.insert(KEY_NATIVES_DIRECTORY.into(), version.game_dirs().join("natives").display().to_string());
        map.insert(KEY_LIBRARY_DIRECTORY.into(), version.game_dirs().join("libraries").display().to_string());

        // Assets index
        let assets_index_name = builder.assets_index
            .as_ref()
            .map(|idx| idx.id.clone())
            .unwrap_or_else(|| version.minecraft_version().into());
        map.insert(KEY_ASSETS_INDEX_NAME.into(), assets_index_name);

        // Launcher
        map.insert(KEY_LAUNCHER_NAME.into(), LAUNCHER_NAME.into());
        map.insert(KEY_LAUNCHER_VERSION.into(), LAUNCHER_VERSION.into());

        // Classpath
        let classpath = build_classpath(version, &builder.libraries);
        map.insert(KEY_CLASSPATH.into(), classpath);
        map.insert(KEY_CLASSPATH_SEPARATOR.into(), classpath_separator.to_string());

        map
}

/// Construit le classpath à partir des libraries
fn build_classpath<T: VersionInfo>(version: &T, libraries: &[lighty_loaders::types::version_metadata::Library]) -> String {
        #[cfg(target_os = "windows")]
        let separator = ";";
        #[cfg(not(target_os = "windows"))]
        let separator = ":";

        let lib_dir = version.game_dirs().join("libraries");

        let mut classpath_entries: Vec<String> = libraries
            .iter()
            .filter_map(|lib| {
                lib.path.as_ref().map(|path| {
                    lib_dir.join(path).display().to_string()
                })
            })
            .collect();

        // Ajouter le client.jar à la fin
        classpath_entries.push(
            version.game_dirs().join(format!("{}.jar", version.name())).display().to_string()
        );

        classpath_entries.join(separator)
}

/// Arguments JVM par défaut (pour anciennes versions sans JVM args)
fn build_default_jvm_args(variables: &HashMap<String, String>) -> Vec<String> {
        let natives_dir = variables.get(KEY_NATIVES_DIRECTORY).cloned().unwrap_or_default();
        let launcher_name = variables.get(KEY_LAUNCHER_NAME).cloned().unwrap_or_default();
        let launcher_version = variables.get(KEY_LAUNCHER_VERSION).cloned().unwrap_or_default();
        let classpath = variables.get(KEY_CLASSPATH).cloned().unwrap_or_default();

        vec![
            "-Xms1024M".into(),
            "-Xmx2048M".into(),
            format!("-Djava.library.path={}", natives_dir),
            format!("-Dminecraft.launcher.brand={}", launcher_name),
            format!("-Dminecraft.launcher.version={}", launcher_version),
            CP_FLAG.into(),
            classpath,
        ]
}


/// Replaces variables in a vector of arguments efficiently
fn replace_variables_in_vec(args: &[String], variables: &HashMap<String, String>) -> Vec<String> {
    args.iter()
        .map(|arg| replace_variables_cow(arg, variables).into_owned())
        .collect()
}

/// Efficient variable replacement using Cow (Copy-on-Write)
/// Only allocates when replacements are actually needed
fn replace_variables_cow<'a>(
    input: &'a str,
    variables: &HashMap<String, String>
) -> Cow<'a, str> {
    // Fast path: no variables to replace
    if !input.contains("${") {
        return Cow::Borrowed(input); // Zero allocation!
    }

    // Pre-allocate with extra capacity for replacements
    let mut result = String::with_capacity(input.len() + 128);
    let mut last_end = 0;

    // Find all ${...} patterns
    for (start, _) in input.match_indices("${") {
        if let Some(end_offset) = input[start..].find('}') {
            let end = start + end_offset;
            let key = &input[start + 2..end];

            // Append text before the variable
            result.push_str(&input[last_end..start]);

            // Replace with value or keep original if not found
            if let Some(value) = variables.get(key) {
                result.push_str(value);
            } else {
                result.push_str(&input[start..=end]);
            }

            last_end = end + 1;
        }
    }

    // Append remaining text
    result.push_str(&input[last_end..]);
    Cow::Owned(result)
}

/// Applique les JVM overrides en ajoutant automatiquement le préfixe `-`
///
/// Formate automatiquement les options JVM :
/// - `Xmx` → `-Xmx`
/// - `XX:+UseG1GC` → `-XX:+UseG1GC`
/// - `Djava.library.path` → `-Djava.library.path`
fn apply_jvm_overrides(jvm_args: &mut Vec<String>, jvm_overrides: &HashMap<String, String>) {
    for (key, value) in jvm_overrides {
        let formatted_option = format_jvm_option(key, value);

        // Vérifier si l'option existe déjà et la remplacer
        let key_prefix = format!("-{}", key.split('=').next().unwrap_or(key));
        if let Some(pos) = jvm_args.iter().position(|arg| arg.starts_with(&key_prefix)) {
            jvm_args[pos] = formatted_option;
        } else {
            // Insérer avant le classpath (-cp)
            if let Some(cp_pos) = jvm_args.iter().position(|arg| arg == CP_FLAG) {
                jvm_args.insert(cp_pos, formatted_option);
            } else {
                jvm_args.push(formatted_option);
            }
        }
    }
}

/// Formate une option JVM avec le préfixe `-` et la valeur
///
/// # Exemples
/// - `("Xmx", "4G")` → `-Xmx4G`
/// - `("Xms", "2G")` → `-Xms2G`
/// - `("XX:+UseG1GC", "")` → `-XX:+UseG1GC`
/// - `("Djava.library.path", "/path")` → `-Djava.library.path=/path`
fn format_jvm_option(key: &str, value: &str) -> String {
    if value.is_empty() {
        format!("-{}", key)
    } else if key.starts_with('X') && !key.contains(':') && !key.contains('=') {
        // Options -Xmx, -Xms, etc. (pas de séparateur)
        format!("-{}{}", key, value)
    } else {
        // Options -D, -XX:, etc. (avec =)
        format!("-{}={}", key, value)
    }
}

/// Supprime les options JVM qui correspondent aux clés dans jvm_removals
fn apply_jvm_removals(jvm_args: &mut Vec<String>, jvm_removals: &HashSet<String>) {
    jvm_args.retain(|arg| {
        // Extraire la clé de l'argument (sans le '-' et sans la valeur)
        let arg_key = if let Some(stripped) = arg.strip_prefix('-') {
            // Gérer les cas -Xmx4G, -Djava.library.path=/path, -XX:+UseG1GC
            stripped.split('=').next().unwrap_or(stripped)
                .split(|c: char| c.is_numeric()).next().unwrap_or(stripped)
        } else {
            return true; // Garder les arguments qui ne commencent pas par '-'
        };

        // Garder l'argument si sa clé n'est pas dans jvm_removals
        !jvm_removals.contains(arg_key)
    });
}

/// Filtre les arguments de jeu en supprimant ceux qui correspondent à arg_removals
fn apply_arg_removals(game_args: Vec<String>, arg_removals: &HashSet<String>) -> Vec<String> {
    game_args.into_iter()
        .filter(|arg| {
            // Supprimer l'argument s'il correspond exactement ou s'il commence par la clé
            !arg_removals.iter().any(|removal| {
                arg == removal || arg.starts_with(&format!("--{}", removal))
            })
        })
        .collect()
}
