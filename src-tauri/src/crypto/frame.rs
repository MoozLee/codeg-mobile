//! Encrypted frame encoding and decoding.
//!
//! A frame is `[24-byte nonce || ciphertext]` base64-encoded. The ciphertext
//! is produced by XSalsa20-Poly1305 keyed with a precomputed 32-byte shared
//! secret (see [`derive_shared`]).
//!
//! The shared-secret derivation is byte-for-byte compatible with
//! `nacl.box.before(theirPub, mySecret)` / `crypto_box_beforenm` from
//! libsodium and TweetNaCl. The frame encoding matches `nacl.box.after`.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use crypto_secretbox::aead::{generic_array::GenericArray, Aead, KeyInit};
use crypto_secretbox::XSalsa20Poly1305;
use rand_core::{OsRng, RngCore};
use salsa20::hsalsa;
use salsa20::cipher::consts::U10;
use x25519_dalek::{PublicKey, StaticSecret};

use super::error::CryptoError;

/// Size of the shared symmetric key produced by [`derive_shared`].
pub const SHARED_KEY_SIZE: usize = 32;

/// Size of the XSalsa20-Poly1305 nonce used for every frame.
pub const NONCE_SIZE: usize = 24;

/// Derive the NaCl `crypto_box_beforenm` precomputed key from a peer's
/// X25519 public key and the local X25519 secret.
///
/// This is the same construction used by libsodium/TweetNaCl:
///
/// ```text
/// shared = X25519(my_secret, their_pub)
/// key    = HSalsa20(shared, nonce=[0u8; 16])
/// ```
///
/// The 32-byte `key` can be fed straight into [`encrypt_frame`] /
/// [`decrypt_frame`].
pub fn derive_shared(
    their_pub: &[u8; 32],
    my_secret: &[u8; 32],
) -> [u8; SHARED_KEY_SIZE] {
    let sk = StaticSecret::from(*my_secret);
    let pk = PublicKey::from(*their_pub);
    let dh = sk.diffie_hellman(&pk);

    let dh_key = GenericArray::clone_from_slice(dh.as_bytes());
    let zero_nonce: GenericArray<u8, _> = GenericArray::default();
    let out = hsalsa::<U10>(&dh_key, &zero_nonce);

    let mut key = [0u8; SHARED_KEY_SIZE];
    key.copy_from_slice(out.as_slice());
    key
}

/// Encrypt `plaintext` with the given 32-byte shared key. Returns a base64
/// frame of `[24-byte nonce || ciphertext]`.
///
/// Pass `nonce` to force a specific 24-byte nonce (used for deterministic
/// test vectors). Pass `None` for a freshly generated random nonce, which is
/// the correct choice at runtime.
pub fn encrypt_frame(
    shared: &[u8; SHARED_KEY_SIZE],
    plaintext: &[u8],
    nonce: Option<[u8; NONCE_SIZE]>,
) -> String {
    let cipher = XSalsa20Poly1305::new(GenericArray::from_slice(shared));
    let nonce_bytes = nonce.unwrap_or_else(|| {
        let mut n = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut n);
        n
    });
    let nonce_array = GenericArray::from_slice(&nonce_bytes);
    // `XSalsa20Poly1305::encrypt` is infallible in practice for in-memory
    // slices (AEAD tag computation cannot fail), so we unwrap.
    let ciphertext = cipher
        .encrypt(nonce_array, plaintext)
        .expect("xsalsa20poly1305 encrypt is infallible for in-memory input");

    let mut frame = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    frame.extend_from_slice(&nonce_bytes);
    frame.extend_from_slice(&ciphertext);
    B64.encode(frame)
}

/// Decode and decrypt a base64 frame produced by [`encrypt_frame`]. Returns
/// [`CryptoError::DecryptionFailed`] if the tag does not verify, which is
/// the expected outcome for tampered or replayed ciphertexts, wrong keys,
/// and wrong nonces alike.
pub fn decrypt_frame(
    shared: &[u8; SHARED_KEY_SIZE],
    frame_b64: &str,
) -> Result<Vec<u8>, CryptoError> {
    let raw = B64.decode(frame_b64.as_bytes())?;
    if raw.len() < NONCE_SIZE {
        return Err(CryptoError::FrameTooShort { actual: raw.len() });
    }

    let (nonce_bytes, ciphertext) = raw.split_at(NONCE_SIZE);
    let cipher = XSalsa20Poly1305::new(GenericArray::from_slice(shared));
    let nonce_array = GenericArray::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce_array, ciphertext)
        .map_err(|_| CryptoError::DecryptionFailed)
}

#[cfg(test)]
mod tests {
    use super::super::keypair::ephemeral_keypair;
    use super::*;

    #[test]
    fn derive_shared_is_symmetric() {
        let (pub_a, sec_a) = ephemeral_keypair();
        let (pub_b, sec_b) = ephemeral_keypair();
        let shared_ab = derive_shared(&pub_b, &sec_a);
        let shared_ba = derive_shared(&pub_a, &sec_b);
        assert_eq!(shared_ab, shared_ba);
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let (pub_a, sec_a) = ephemeral_keypair();
        let (pub_b, sec_b) = ephemeral_keypair();
        let shared_a = derive_shared(&pub_b, &sec_a);
        let shared_b = derive_shared(&pub_a, &sec_b);
        let plaintext = b"hello codeg mobile";

        let frame = encrypt_frame(&shared_a, plaintext, None);
        let decoded = decrypt_frame(&shared_b, &frame).unwrap();
        assert_eq!(decoded, plaintext);
    }

    #[test]
    fn tampered_frame_fails_decrypt() {
        let (pub_a, sec_a) = ephemeral_keypair();
        let (pub_b, sec_b) = ephemeral_keypair();
        let shared_a = derive_shared(&pub_b, &sec_a);
        let shared_b = derive_shared(&pub_a, &sec_b);
        let frame = encrypt_frame(&shared_a, b"payload", None);

        // Flip one byte in the base64 to corrupt the ciphertext/tag. We pick
        // the last character so we tamper with the Poly1305 tag, not the
        // nonce prefix.
        let mut bytes = frame.into_bytes();
        let last = bytes.last_mut().unwrap();
        *last = if *last == b'A' { b'B' } else { b'A' };
        let tampered = String::from_utf8(bytes).unwrap();

        let err = decrypt_frame(&shared_b, &tampered).unwrap_err();
        assert!(matches!(err, CryptoError::DecryptionFailed));
    }

    #[test]
    fn short_frame_rejected() {
        let shared = [0u8; 32];
        let err = decrypt_frame(&shared, &B64.encode([0u8; 10])).unwrap_err();
        assert!(matches!(err, CryptoError::FrameTooShort { .. }));
    }

    #[test]
    fn deterministic_with_fixed_nonce() {
        // Same inputs + same nonce must always produce the same frame.
        let shared = [7u8; 32];
        let nonce = [9u8; 24];
        let a = encrypt_frame(&shared, b"abc", Some(nonce));
        let b = encrypt_frame(&shared, b"abc", Some(nonce));
        assert_eq!(a, b);
    }
}
