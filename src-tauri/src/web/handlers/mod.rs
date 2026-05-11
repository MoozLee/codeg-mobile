pub mod acp;
pub mod chat_channel;
pub mod conversations;
mod error;
pub mod experts;
pub mod files;
pub mod folder_commands;
pub mod folders;
pub mod git;
pub mod mcp;
#[cfg(feature = "relay-client")]
pub mod mobile_pairing;
pub mod model_provider;
pub mod pet;
pub mod project_boot;
pub mod quick_messages;
pub mod system_settings;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod terminal;
pub mod version_control;
pub mod web_server;
pub mod workspace_state;
