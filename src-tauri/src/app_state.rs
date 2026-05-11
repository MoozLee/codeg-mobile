use std::path::PathBuf;
use std::sync::Arc;

use crate::acp::manager::ConnectionManager;
use crate::chat_channel::manager::ChatChannelManager;
#[cfg(feature = "relay-client")]
use crate::crypto::keypair::LongTermKeyPair;
use crate::db::AppDatabase;
use crate::pet_state_mapper::PetStateHandle;
#[cfg(feature = "relay-client")]
use crate::relay::PairingCoordinator;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::terminal::manager::TerminalManager;
use crate::web::event_bridge::{EventEmitter, WebEventBroadcaster};
use crate::web::WebServerState;

pub struct AppState {
    pub db: AppDatabase,
    pub connection_manager: ConnectionManager,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub terminal_manager: TerminalManager,
    pub event_broadcaster: Arc<WebEventBroadcaster>,
    pub emitter: EventEmitter,
    pub data_dir: PathBuf,
    pub web_server_state: WebServerState,
    pub chat_channel_manager: ChatChannelManager,
    /// Latest ambient `PetState` written by `pet_state_subscriber_task`.
    /// Read by `pet_get_current_state` so a freshly-opened pet window can
    /// pick up the current state without waiting for the next transition.
    pub pet_state: PetStateHandle,
    /// Coordinator for mobile pairing sessions. `Some` on any build with
    /// the `relay-client` feature (desktop + mobile); `codeg-server`
    /// currently builds without that feature and gets `None` so the
    /// existing web-only deployment path compiles untouched.
    #[cfg(feature = "relay-client")]
    pub pairing_coordinator: Arc<PairingCoordinator>,
}

pub fn default_connection_manager() -> ConnectionManager {
    ConnectionManager::new()
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn default_terminal_manager() -> TerminalManager {
    TerminalManager::new()
}

pub fn default_chat_channel_manager() -> ChatChannelManager {
    ChatChannelManager::new()
}

/// Path at which the daemon persists its long-term Curve25519 keypair.
/// Lives under the resolved `data_dir`, alongside `codeg.db`.
#[cfg(feature = "relay-client")]
pub fn default_mobile_keypair_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("mobile-keypair.json")
}

/// Load the long-term keypair or create one if missing. Any filesystem /
/// parse error falls back to an ephemeral in-memory keypair so a broken
/// key file cannot brick the desktop app (the user can re-pair from the
/// Settings page afterwards).
#[cfg(feature = "relay-client")]
pub fn load_or_create_mobile_keypair(data_dir: &std::path::Path) -> Arc<LongTermKeyPair> {
    let path = default_mobile_keypair_path(data_dir);
    match crate::crypto::keypair::load_or_create(&path) {
        Ok(kp) => Arc::new(kp),
        Err(err) => {
            eprintln!(
                "[Mobile] failed to load/create keypair at {}: {err}; using ephemeral",
                path.display()
            );
            Arc::new(LongTermKeyPair::generate())
        }
    }
}

/// Build a ready-to-use pairing coordinator from a data_dir. Callers
/// normally do this once when constructing `AppState`.
#[cfg(feature = "relay-client")]
pub fn default_pairing_coordinator(data_dir: &std::path::Path) -> Arc<PairingCoordinator> {
    let kp = load_or_create_mobile_keypair(data_dir);
    Arc::new(PairingCoordinator::new(kp))
}
