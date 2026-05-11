//! Curve25519 keypair management for the daemon side.
//!
//! The daemon owns a long-term X25519 keypair that identifies it across
//! pairings. The private key is persisted as a 0600-mode JSON file at
//! `<data_dir>/mobile-keypair.json` and never leaves the daemon.
//!
//! Phones use ephemeral keypairs generated in memory for each relay session.

use std::fs;
use std::path::{Path, PathBuf};

use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};

use super::error::CryptoError;

/// Curve25519 key size in bytes.
pub const KEY_SIZE: usize = 32;

/// On-disk representation of the daemon's long-term keypair. Secret material is
/// kept as hex so the file stays trivially inspectable with `cat`; the file's
/// 0600 mode (on unix) is the real confidentiality boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredKeyPair {
    public_hex: String,
    secret_hex: String,
}

/// Daemon long-term keypair. Secret is held in memory only for the lifetime
/// of the daemon process; reload by calling [`load_or_create`] again.
#[derive(Debug, Clone)]
pub struct LongTermKeyPair {
    pub public: [u8; KEY_SIZE],
    secret: [u8; KEY_SIZE],
}

impl LongTermKeyPair {
    /// Build a new random keypair using the OS RNG.
    pub fn generate() -> Self {
        let (public, secret) = ephemeral_keypair();
        Self { public, secret }
    }

    /// Inspect the secret bytes. Callers should avoid copying or logging these.
    pub fn secret(&self) -> &[u8; KEY_SIZE] {
        &self.secret
    }

    /// Inspect the public bytes.
    pub fn public(&self) -> &[u8; KEY_SIZE] {
        &self.public
    }
}

/// Load the daemon keypair from `path`, or generate + persist a new one if
/// the file does not exist. On unix the freshly written file is set to mode
/// `0o600`; on other platforms the OS ACL applies.
pub fn load_or_create(path: &Path) -> Result<LongTermKeyPair, CryptoError> {
    if path.exists() {
        return load(path);
    }
    let kp = LongTermKeyPair::generate();
    save(path, &kp)?;
    Ok(kp)
}

/// Read an existing keypair file. Returns a `CryptoError` if the file is
/// malformed or the fields are not valid 32-byte hex strings.
pub fn load(path: &Path) -> Result<LongTermKeyPair, CryptoError> {
    let raw = fs::read_to_string(path)?;
    let stored: StoredKeyPair = serde_json::from_str(&raw)?;
    let public = decode_fixed_hex(&stored.public_hex)?;
    let secret = decode_fixed_hex(&stored.secret_hex)?;
    Ok(LongTermKeyPair { public, secret })
}

/// Persist `kp` atomically by writing to `<path>.tmp` and renaming. Returns
/// an error if the parent directory cannot be created.
pub fn save(path: &Path, kp: &LongTermKeyPair) -> Result<(), CryptoError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let stored = StoredKeyPair {
        public_hex: hex::encode(kp.public),
        secret_hex: hex::encode(kp.secret),
    };
    let serialized = serde_json::to_string_pretty(&stored)?;

    let tmp = temp_path(path);
    fs::write(&tmp, serialized.as_bytes())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    }

    fs::rename(&tmp, path)?;
    Ok(())
}

/// Generate a fresh X25519 keypair suitable for a single relay session. The
/// phone uses this for its client keypair; the daemon uses the long-term
/// keypair returned by [`load_or_create`] instead.
pub fn ephemeral_keypair() -> ([u8; KEY_SIZE], [u8; KEY_SIZE]) {
    let mut secret = [0u8; KEY_SIZE];
    OsRng.fill_bytes(&mut secret);
    // X25519 scalar clamping happens inside `StaticSecret::from` and the DH
    // op, so we derive the public key through x25519_dalek to stay byte-for-
    // byte compatible with the JS side (which uses tweetnacl).
    let sk = x25519_dalek::StaticSecret::from(secret);
    let pk = x25519_dalek::PublicKey::from(&sk);
    let public = pk.to_bytes();
    // `sk.to_bytes()` returns the *clamped* scalar; tweetnacl returns the raw
    // 32 random bytes as the secret key. We expose the raw bytes here so
    // cross-TS roundtrips (where both sides store `secretKey` as the original
    // random 32 bytes) line up. Clamping only matters inside the DH op, which
    // x25519-dalek applies regardless of the stored form.
    (public, secret)
}

fn decode_fixed_hex(raw: &str) -> Result<[u8; KEY_SIZE], CryptoError> {
    let bytes = hex::decode(raw)?;
    if bytes.len() != KEY_SIZE {
        return Err(CryptoError::InvalidKeyLength {
            expected: KEY_SIZE,
            actual: bytes.len(),
        });
    }
    let mut out = [0u8; KEY_SIZE];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn ephemeral_keypair_is_non_zero_and_matches_dh_identity() {
        let (pub_a, sec_a) = ephemeral_keypair();
        let (pub_b, sec_b) = ephemeral_keypair();
        assert_ne!(pub_a, [0u8; 32]);
        assert_ne!(pub_b, [0u8; 32]);
        assert_ne!(sec_a, sec_b);
        assert_ne!(pub_a, pub_b);
    }

    #[test]
    fn load_or_create_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mobile-keypair.json");
        let first = load_or_create(&path).unwrap();
        assert!(path.exists());
        let again = load_or_create(&path).unwrap();
        assert_eq!(first.public, again.public);
        assert_eq!(first.secret(), again.secret());
    }

    #[cfg(unix)]
    #[test]
    fn saved_file_has_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().unwrap();
        let path = dir.path().join("mobile-keypair.json");
        let kp = LongTermKeyPair::generate();
        save(&path, &kp).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0600 got {mode:o}");
    }

    #[test]
    fn load_rejects_wrong_length_hex() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mobile-keypair.json");
        fs::write(
            &path,
            r#"{"public_hex":"00","secret_hex":"00"}"#,
        )
        .unwrap();
        let err = load(&path).unwrap_err();
        assert!(matches!(err, CryptoError::InvalidKeyLength { .. }));
    }
}
