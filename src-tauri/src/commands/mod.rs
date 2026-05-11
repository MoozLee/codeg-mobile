pub mod acp;
pub mod chat_channel;
pub mod conversations;
pub mod experts;
#[cfg(all(feature = "tauri-runtime", not(any(target_os = "ios", target_os = "android"))))]
pub mod file_io;
pub mod folder_commands;
pub mod folders;
pub mod mcp;
#[cfg(feature = "relay-client")]
pub mod mobile_pairing;
pub mod model_provider;
#[cfg(all(feature = "tauri-runtime", not(any(target_os = "ios", target_os = "android"))))]
pub mod notification;
pub mod pet;
pub mod project_boot;
pub mod quick_messages;
pub mod system_settings;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod terminal;
pub mod version_control;
#[cfg(all(feature = "tauri-runtime", not(any(target_os = "ios", target_os = "android"))))]
pub mod windows;
pub mod workspace_state;
