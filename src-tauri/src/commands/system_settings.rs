#[cfg(feature = "tauri-runtime")]
use std::path::{Component, Path, PathBuf};
#[cfg(feature = "tauri-runtime")]
use std::{collections::BTreeMap, sync::OnceLock};

use sea_orm::DatabaseConnection;
#[cfg(feature = "tauri-runtime")]
use tauri::State;

use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;
#[cfg(feature = "tauri-runtime")]
use crate::db::AppDatabase;
use crate::models::{
    AvailableTerminalShells, SystemFontFamily, SystemFontFamilyList, SystemFontFamilySource,
    SystemFontSettings, SystemLanguageSettings, SystemOpenTargetSettings, SystemProxySettings,
    SystemTerminalSettings, TerminalShellOption,
};
#[cfg(feature = "tauri-runtime")]
use crate::models::{SystemOpenTarget, SystemRenderingSettings};
#[cfg(feature = "tauri-runtime")]
use crate::network::proxy;
#[cfg(feature = "tauri-runtime")]
use crate::preferences;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::terminal::manager::resolve_shell;

pub(crate) const SYSTEM_PROXY_SETTINGS_KEY: &str = "system_proxy_settings";
pub(crate) const SYSTEM_LANGUAGE_SETTINGS_KEY: &str = "system_language_settings";
pub(crate) const SYSTEM_OPEN_TARGET_SETTINGS_KEY: &str = "system_open_target_settings";
pub(crate) const SYSTEM_TERMINAL_SETTINGS_KEY: &str = "system_terminal_settings";
const APPEARANCE_FONT_SETTINGS_KEY: &str = "appearance_font_settings";
pub(crate) const LANGUAGE_SETTINGS_UPDATED_EVENT: &str = "app://language-settings-updated";
pub(crate) const TERMINAL_SETTINGS_UPDATED_EVENT: &str = "app://terminal-settings-updated";
pub(crate) const TERMINAL_SHELL_OPTION_SYSTEM: &str = "system";
pub(crate) const TERMINAL_SHELL_OPTION_CUSTOM: &str = "custom";
const MAX_FONT_FAMILY_LENGTH: usize = 128;
#[cfg(feature = "tauri-runtime")]
const MAX_FONT_FAMILIES: usize = 512;
const FALLBACK_FONT_FAMILIES: [(&str, bool); 10] = [
    ("system-ui", false),
    ("ui-sans-serif", false),
    ("Arial", false),
    ("Helvetica", false),
    ("sans-serif", false),
    ("ui-monospace", true),
    ("Menlo", true),
    ("Monaco", true),
    ("Courier New", true),
    ("monospace", true),
];

#[cfg(feature = "tauri-runtime")]
static SYSTEM_FONT_FAMILY_CACHE: OnceLock<SystemFontFamilyList> = OnceLock::new();

#[cfg(feature = "tauri-runtime")]
fn sanitize_font_family_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('.')
        || trimmed.chars().count() > MAX_FONT_FAMILY_LENGTH
        || trimmed.chars().any(char::is_control)
    {
        return None;
    }
    Some(trimmed.to_string())
}

#[cfg(feature = "tauri-runtime")]
fn insert_font_family(
    families: &mut BTreeMap<String, SystemFontFamily>,
    family: String,
    monospace: bool,
) {
    let key = family.to_lowercase();
    families
        .entry(key)
        .and_modify(|existing| {
            existing.monospace = existing.monospace || monospace;
        })
        .or_insert(SystemFontFamily { family, monospace });
}

pub(crate) fn fallback_system_font_families() -> SystemFontFamilyList {
    let families = FALLBACK_FONT_FAMILIES
        .iter()
        .map(|(family, monospace)| SystemFontFamily {
            family: (*family).to_string(),
            monospace: *monospace,
        })
        .collect();

    SystemFontFamilyList {
        families,
        source: SystemFontFamilySource::Fallback,
    }
}

#[cfg(all(feature = "tauri-runtime", not(any(target_os = "ios", target_os = "android"))))]
pub(crate) fn list_system_font_families_core() -> SystemFontFamilyList {
    SYSTEM_FONT_FAMILY_CACHE
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();

            let mut families = BTreeMap::new();
            for face in db.faces() {
                for (family, _language) in &face.families {
                    if let Some(safe_family) = sanitize_font_family_name(family) {
                        insert_font_family(&mut families, safe_family, face.monospaced);
                    }
                }
            }

            let families = families
                .into_values()
                .take(MAX_FONT_FAMILIES)
                .collect::<Vec<_>>();

            if families.is_empty() {
                fallback_system_font_families()
            } else {
                SystemFontFamilyList {
                    families,
                    source: SystemFontFamilySource::System,
                }
            }
        })
        .clone()
}

fn normalize_proxy_settings(
    settings: SystemProxySettings,
) -> Result<SystemProxySettings, AppCommandError> {
    if !settings.enabled {
        let proxy_url = settings
            .proxy_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        return Ok(SystemProxySettings {
            enabled: false,
            proxy_url,
        });
    }

    let proxy_url = settings
        .proxy_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppCommandError::configuration_missing("Proxy URL is required when proxy is enabled")
        })?;

    reqwest::Proxy::all(proxy_url).map_err(|e| {
        AppCommandError::configuration_invalid("Invalid proxy URL").with_detail(e.to_string())
    })?;

    Ok(SystemProxySettings {
        enabled: true,
        proxy_url: Some(proxy_url.to_string()),
    })
}

fn normalize_font_family_preference(value: Option<String>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('.')
        || trimmed.chars().count() > MAX_FONT_FAMILY_LENGTH
        || trimmed.chars().any(char::is_control)
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn normalize_font_settings(settings: SystemFontSettings) -> SystemFontSettings {
    SystemFontSettings {
        ui_font_family: normalize_font_family_preference(settings.ui_font_family),
        code_font_family: normalize_font_family_preference(settings.code_font_family),
    }
}

#[cfg(feature = "tauri-runtime")]
struct ResolvedWorkspacePath {
    root: PathBuf,
    target: PathBuf,
}

#[cfg(feature = "tauri-runtime")]
fn resolve_workspace_relative_path(
    folder_path: &str,
    relative_path: &str,
) -> Result<ResolvedWorkspacePath, AppCommandError> {
    let root = PathBuf::from(folder_path);
    if !root.exists() || !root.is_dir() {
        return Err(AppCommandError::not_found("Folder does not exist"));
    }

    let rel = Path::new(relative_path);
    if rel.is_absolute() {
        return Err(AppCommandError::invalid_input("Path must be relative"));
    }

    for component in rel.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => {
                return Err(AppCommandError::invalid_input("Path cannot contain '..'"));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(AppCommandError::invalid_input("Invalid path component"));
            }
        }
    }

    let target = root.join(rel);
    if !target.exists() {
        return Err(AppCommandError::not_found("File does not exist"));
    }
    if !target.is_file() {
        return Err(AppCommandError::invalid_input("Path is not a file"));
    }

    let canonical_root = std::fs::canonicalize(&root).map_err(AppCommandError::io)?;
    let canonical_target = std::fs::canonicalize(&target).map_err(AppCommandError::io)?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(AppCommandError::invalid_input(
            "Path is outside workspace root",
        ));
    }

    Ok(ResolvedWorkspacePath {
        root: canonical_root,
        target: canonical_target,
    })
}

#[cfg(feature = "tauri-runtime")]
fn spawn_code_cli(root: &Path, target: &Path) -> Result<(), std::io::Error> {
    let mut command = crate::process::std_command("code");
    command.arg("--new-window").arg(root).arg(target);
    command.spawn().map(|_| ())
}

#[cfg(all(feature = "tauri-runtime", target_os = "macos"))]
fn spawn_platform_vscode(root: &Path, target: &Path) -> Result<(), std::io::Error> {
    let app_cli = Path::new("/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code");
    if app_cli.exists() {
        let mut command = crate::process::std_command(app_cli);
        command.arg("--new-window").arg(root).arg(target);
        if command.spawn().is_ok() {
            return Ok(());
        }
    }

    let mut command = crate::process::std_command("open");
    command
        .arg("-n")
        .arg("-a")
        .arg("Visual Studio Code")
        .arg("--args")
        .arg("--new-window")
        .arg(root)
        .arg(target);
    match command.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("VS Code application could not be opened: {status}"),
        )),
        Err(err) => Err(err),
    }
}

#[cfg(all(feature = "tauri-runtime", target_os = "windows"))]
fn spawn_platform_vscode(root: &Path, target: &Path) -> Result<(), std::io::Error> {
    let mut candidates = Vec::new();
    if let Some(base) = std::env::var_os("LOCALAPPDATA") {
        let base = PathBuf::from(base);
        candidates.push(
            base.join("Programs")
                .join("Microsoft VS Code")
                .join("Code.exe"),
        );
        candidates.push(base.join("Microsoft VS Code").join("Code.exe"));
    }
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(base) = std::env::var_os(key) {
            candidates.push(
                PathBuf::from(base)
                    .join("Microsoft VS Code")
                    .join("Code.exe"),
            );
        }
    }

    let mut last_error = None;
    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }
        let mut command = crate::process::std_command(&candidate);
        command.arg("--new-window").arg(root).arg(target);
        match command.spawn() {
            Ok(_) => return Ok(()),
            Err(err) => last_error = Some(err),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "VS Code executable not found")
    }))
}

#[cfg(all(
    feature = "tauri-runtime",
    not(any(target_os = "macos", target_os = "windows"))
))]
fn spawn_platform_vscode(_root: &Path, _target: &Path) -> Result<(), std::io::Error> {
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "VS Code executable not found",
    ))
}

#[cfg(feature = "tauri-runtime")]
fn open_path_in_vscode(root: &Path, target: &Path) -> Result<(), AppCommandError> {
    match spawn_code_cli(root, target) {
        Ok(()) => Ok(()),
        Err(code_err) => match spawn_platform_vscode(root, target) {
            Ok(()) => Ok(()),
            Err(fallback_err) => {
                let detail = format!("code: {code_err}; fallback: {fallback_err}");
                if code_err.kind() == std::io::ErrorKind::NotFound
                    && fallback_err.kind() == std::io::ErrorKind::NotFound
                {
                    Err(AppCommandError::dependency_missing(
                        "VS Code was not found. Install VS Code and enable the 'code' command in PATH.",
                    )
                    .with_detail(detail))
                } else {
                    Err(AppCommandError::external_command(
                        "Failed to open file in VS Code",
                        detail,
                    ))
                }
            }
        },
    }
}

pub(crate) async fn load_system_proxy_settings(
    conn: &DatabaseConnection,
) -> Result<SystemProxySettings, AppCommandError> {
    let raw = app_metadata_service::get_value(conn, SYSTEM_PROXY_SETTINGS_KEY)
        .await
        .map_err(AppCommandError::from)?;

    let Some(raw) = raw else {
        return Ok(SystemProxySettings::default());
    };

    let parsed = serde_json::from_str::<SystemProxySettings>(&raw).map_err(|e| {
        AppCommandError::configuration_invalid("Failed to parse stored proxy settings")
            .with_detail(e.to_string())
    })?;
    normalize_proxy_settings(parsed)
}

pub(crate) async fn load_system_language_settings(
    conn: &DatabaseConnection,
) -> Result<SystemLanguageSettings, AppCommandError> {
    let raw = app_metadata_service::get_value(conn, SYSTEM_LANGUAGE_SETTINGS_KEY)
        .await
        .map_err(AppCommandError::from)?;

    let Some(raw) = raw else {
        return Ok(SystemLanguageSettings::default());
    };

    serde_json::from_str::<SystemLanguageSettings>(&raw).map_err(|e| {
        AppCommandError::configuration_invalid("Failed to parse stored language settings")
            .with_detail(e.to_string())
    })
}

pub(crate) async fn load_system_open_target_settings(
    conn: &DatabaseConnection,
) -> Result<SystemOpenTargetSettings, AppCommandError> {
    let raw = app_metadata_service::get_value(conn, SYSTEM_OPEN_TARGET_SETTINGS_KEY)
        .await
        .map_err(AppCommandError::from)?;

    let Some(raw) = raw else {
        return Ok(SystemOpenTargetSettings::default());
    };

    serde_json::from_str::<SystemOpenTargetSettings>(&raw).map_err(|e| {
        AppCommandError::configuration_invalid("Failed to parse stored open target settings")
            .with_detail(e.to_string())
    })
}

pub(crate) async fn load_system_font_settings(
    conn: &DatabaseConnection,
) -> Result<SystemFontSettings, AppCommandError> {
    let raw = app_metadata_service::get_value(conn, APPEARANCE_FONT_SETTINGS_KEY)
        .await
        .map_err(AppCommandError::from)?;

    let Some(raw) = raw else {
        return Ok(SystemFontSettings::default());
    };

    let parsed = serde_json::from_str::<SystemFontSettings>(&raw).map_err(|e| {
        AppCommandError::configuration_invalid("Failed to parse stored font settings")
            .with_detail(e.to_string())
    })?;
    Ok(normalize_font_settings(parsed))
}

pub(crate) async fn update_system_font_settings_core(
    conn: &DatabaseConnection,
    settings: SystemFontSettings,
) -> Result<SystemFontSettings, AppCommandError> {
    let normalized = normalize_font_settings(settings);
    let serialized = serde_json::to_string(&normalized).map_err(|e| {
        AppCommandError::invalid_input("Failed to serialize font settings")
            .with_detail(e.to_string())
    })?;

    app_metadata_service::upsert_value(conn, APPEARANCE_FONT_SETTINGS_KEY, &serialized)
        .await
        .map_err(AppCommandError::from)?;

    Ok(normalized)
}

pub(crate) async fn update_system_open_target_settings_core(
    conn: &DatabaseConnection,
    settings: SystemOpenTargetSettings,
) -> Result<SystemOpenTargetSettings, AppCommandError> {
    let serialized = serde_json::to_string(&settings).map_err(|e| {
        AppCommandError::invalid_input("Failed to serialize open target settings")
            .with_detail(e.to_string())
    })?;

    app_metadata_service::upsert_value(conn, SYSTEM_OPEN_TARGET_SETTINGS_KEY, &serialized)
        .await
        .map_err(AppCommandError::from)?;

    Ok(settings)
}

#[cfg(feature = "tauri-runtime")]
pub(crate) async fn open_path_with_target_core(
    folder_path: String,
    relative_path: String,
    target: Option<SystemOpenTarget>,
    conn: &DatabaseConnection,
) -> Result<(), AppCommandError> {
    let target = match target {
        Some(target) => target,
        None => load_system_open_target_settings(conn).await?.target,
    };

    match target {
        SystemOpenTarget::Vscode => {
            let resolved_path = resolve_workspace_relative_path(&folder_path, &relative_path)?;
            open_path_in_vscode(&resolved_path.root, &resolved_path.target)?;
            Ok(())
        }
        SystemOpenTarget::FileManager => Err(AppCommandError::invalid_input(
            "The open_path_with_target command only supports VS Code. Use file manager actions from the file tree instead.",
        )),
        SystemOpenTarget::Terminal => Err(AppCommandError::invalid_input(
            "The open_path_with_target command does not support opening terminals.",
        )),
    }
}

/// Whether `value` resolves to an executable on the current host. Used to
/// drive the "not installed" badge in the picker; never used to *block* a
/// selection — users may legitimately preconfigure a shell before installing it.
fn shell_exists(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }

    let path = std::path::Path::new(trimmed);
    let looks_like_path = path.is_absolute()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || path.components().count() > 1;

    if looks_like_path {
        return path.is_file();
    }

    which::which(trimmed).is_ok()
}

/// Trim and drop empty-only. We deliberately do **not** filter by host
/// platform: the Settings UI's custom-path field lets users type any shell
/// they want, and silently rewriting their input is more confusing than
/// letting `terminal_spawn` surface the failure if the path is wrong.
pub(crate) fn normalize_terminal_settings(
    settings: SystemTerminalSettings,
) -> SystemTerminalSettings {
    SystemTerminalSettings {
        default_shell: settings
            .default_shell
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    }
}

/// Build the per-platform option list shown in the "default shell" picker.
/// The frontend renders these verbatim, looking each `label_key` up under its
/// `SystemSettings` namespace — so adding a new shell here requires zero
/// frontend code changes (only a new translation key).
pub(crate) fn build_available_terminal_shells() -> AvailableTerminalShells {
    let mut options: Vec<TerminalShellOption> = Vec::new();

    options.push(TerminalShellOption {
        id: TERMINAL_SHELL_OPTION_SYSTEM.to_string(),
        label_key: "terminalSystemDefault".to_string(),
        value: None,
        // System default always "exists" — resolve_shell() has its own fallback chain.
        exists: true,
        accepts_custom_path: false,
    });

    if cfg!(target_os = "windows") {
        for (id, label_key) in [
            ("pwsh.exe", "terminalPowerShell7"),
            ("powershell.exe", "terminalWindowsPowerShell"),
            ("cmd.exe", "terminalCmd"),
        ] {
            options.push(TerminalShellOption {
                id: id.to_string(),
                label_key: label_key.to_string(),
                value: Some(id.to_string()),
                exists: shell_exists(id),
                accepts_custom_path: false,
            });
        }
    }

    options.push(TerminalShellOption {
        id: TERMINAL_SHELL_OPTION_CUSTOM.to_string(),
        label_key: "terminalShellCustom".to_string(),
        value: None,
        // The "custom" row itself is always available; the path the user
        // types is validated via probe_terminal_shell_path.
        exists: true,
        accepts_custom_path: true,
    });

    AvailableTerminalShells {
        options,
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        resolved_shell: resolve_shell(),
        #[cfg(any(target_os = "ios", target_os = "android"))]
        resolved_shell: String::new(),
    }
}

/// Probe whether a user-supplied shell path or command exists on the host.
/// Returns `false` for empty / whitespace-only input.
pub(crate) fn probe_terminal_shell_path_core(path: &str) -> bool {
    shell_exists(path)
}

pub(crate) async fn load_system_terminal_settings(
    conn: &DatabaseConnection,
) -> Result<SystemTerminalSettings, AppCommandError> {
    let raw = app_metadata_service::get_value(conn, SYSTEM_TERMINAL_SETTINGS_KEY)
        .await
        .map_err(AppCommandError::from)?;

    let Some(raw) = raw else {
        return Ok(SystemTerminalSettings::default());
    };

    let parsed = serde_json::from_str::<SystemTerminalSettings>(&raw).map_err(|e| {
        AppCommandError::configuration_invalid("Failed to parse stored terminal settings")
            .with_detail(e.to_string())
    })?;

    Ok(normalize_terminal_settings(parsed))
}

#[cfg(all(feature = "tauri-runtime", not(any(target_os = "ios", target_os = "android"))))]
#[cfg_attr(all(feature = "tauri-runtime", not(any(target_os = "ios", target_os = "android"))), tauri::command)]
pub async fn list_system_font_families() -> Result<SystemFontFamilyList, AppCommandError> {
    Ok(list_system_font_families_core())
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_system_font_settings(
    db: State<'_, AppDatabase>,
) -> Result<SystemFontSettings, AppCommandError> {
    load_system_font_settings(&db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn update_system_font_settings(
    settings: SystemFontSettings,
    db: State<'_, AppDatabase>,
) -> Result<SystemFontSettings, AppCommandError> {
    update_system_font_settings_core(&db.conn, settings).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_system_proxy_settings(
    db: State<'_, AppDatabase>,
) -> Result<SystemProxySettings, AppCommandError> {
    load_system_proxy_settings(&db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn update_system_proxy_settings(
    settings: SystemProxySettings,
    db: State<'_, AppDatabase>,
) -> Result<SystemProxySettings, AppCommandError> {
    let normalized = normalize_proxy_settings(settings)?;
    let serialized = serde_json::to_string(&normalized).map_err(|e| {
        AppCommandError::invalid_input("Failed to serialize proxy settings")
            .with_detail(e.to_string())
    })?;

    app_metadata_service::upsert_value(&db.conn, SYSTEM_PROXY_SETTINGS_KEY, &serialized)
        .await
        .map_err(AppCommandError::from)?;

    proxy::apply_system_proxy_settings(&normalized)?;
    Ok(normalized)
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_system_language_settings(
    db: State<'_, AppDatabase>,
) -> Result<SystemLanguageSettings, AppCommandError> {
    load_system_language_settings(&db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn update_system_language_settings(
    settings: SystemLanguageSettings,
    db: State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<SystemLanguageSettings, AppCommandError> {
    let serialized = serde_json::to_string(&settings).map_err(|e| {
        AppCommandError::invalid_input("Failed to serialize language settings")
            .with_detail(e.to_string())
    })?;

    app_metadata_service::upsert_value(&db.conn, SYSTEM_LANGUAGE_SETTINGS_KEY, &serialized)
        .await
        .map_err(AppCommandError::from)?;

    let emitter = crate::web::event_bridge::EventEmitter::Tauri(app);
    crate::web::event_bridge::emit_event(
        &emitter,
        LANGUAGE_SETTINGS_UPDATED_EVENT,
        settings.clone(),
    );

    Ok(settings)
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_system_open_target_settings(
    db: State<'_, AppDatabase>,
) -> Result<SystemOpenTargetSettings, AppCommandError> {
    load_system_open_target_settings(&db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn update_system_open_target_settings(
    settings: SystemOpenTargetSettings,
    db: State<'_, AppDatabase>,
) -> Result<SystemOpenTargetSettings, AppCommandError> {
    update_system_open_target_settings_core(&db.conn, settings).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn open_path_with_target(
    folder_path: String,
    relative_path: String,
    target: Option<SystemOpenTarget>,
    db: State<'_, AppDatabase>,
) -> Result<(), AppCommandError> {
    open_path_with_target_core(folder_path, relative_path, target, &db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_system_terminal_settings(
    db: State<'_, AppDatabase>,
) -> Result<SystemTerminalSettings, AppCommandError> {
    load_system_terminal_settings(&db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_available_terminal_shells() -> Result<AvailableTerminalShells, AppCommandError> {
    Ok(build_available_terminal_shells())
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn probe_terminal_shell_path(path: String) -> Result<bool, AppCommandError> {
    Ok(probe_terminal_shell_path_core(&path))
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn update_system_terminal_settings(
    settings: SystemTerminalSettings,
    db: State<'_, AppDatabase>,
    app: tauri::AppHandle,
) -> Result<SystemTerminalSettings, AppCommandError> {
    let normalized = normalize_terminal_settings(settings);
    let serialized = serde_json::to_string(&normalized).map_err(|e| {
        AppCommandError::invalid_input("Failed to serialize terminal settings")
            .with_detail(e.to_string())
    })?;

    app_metadata_service::upsert_value(&db.conn, SYSTEM_TERMINAL_SETTINGS_KEY, &serialized)
        .await
        .map_err(AppCommandError::from)?;

    let emitter = crate::web::event_bridge::EventEmitter::Tauri(app);
    crate::web::event_bridge::emit_event(
        &emitter,
        TERMINAL_SETTINGS_UPDATED_EVENT,
        normalized.clone(),
    );

    Ok(normalized)
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn get_system_rendering_settings() -> Result<SystemRenderingSettings, AppCommandError> {
    let prefs = preferences::load();
    Ok(SystemRenderingSettings {
        disable_hardware_acceleration: prefs.disable_hardware_acceleration,
    })
}

#[cfg(feature = "tauri-runtime")]
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
pub async fn update_system_rendering_settings(
    settings: SystemRenderingSettings,
) -> Result<SystemRenderingSettings, AppCommandError> {
    let mut prefs = preferences::load();
    prefs.disable_hardware_acceleration = settings.disable_hardware_acceleration;
    preferences::save(&prefs).map_err(|err| {
        AppCommandError::io_error("Failed to persist rendering settings")
            .with_detail(err.to_string())
    })?;
    Ok(settings)
}
