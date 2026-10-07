//! Cryptographic primitives: random tokens, constant-time comparison, digests, and sealed
//! (encrypted and authenticated) values.
//!
//! [`seal`]/[`open`] use XChaCha20-Poly1305 with a key derived from the server secret and a
//! purpose label, so a value sealed for one purpose (a socket token) never opens as another
//! (a stored credential). Sealed values are `base64url(nonce || ciphertext || tag)`.

use base64::Engine as _;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

const NONCE_LEN: usize = 24;

/// Constant-time equality.
pub fn secure_compare(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && bool::from(left.ct_eq(right))
}

/// Random bytes.
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

/// SHA-256 digest.
pub fn sha256(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

/// URL-safe base64 with padding.
pub fn url_encode64(bytes: &[u8]) -> String {
    URL_SAFE.encode(bytes)
}

/// URL-safe base64 without padding.
pub fn url_encode64_unpadded(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decodes URL-safe base64 without padding.
pub fn url_decode64_unpadded(value: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(value).ok()
}

/// The key for `purpose`: SHA-256 over the purpose label, a separator, and the secret.
fn purpose_key(secret: &str, purpose: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(purpose.as_bytes());
    hasher.update([0]);
    hasher.update(secret.as_bytes());
    hasher.finalize().into()
}

/// Encrypts and authenticates `plain` for `purpose`.
pub fn seal(secret: &str, purpose: &str, plain: &[u8]) -> String {
    let nonce: [u8; NONCE_LEN] = random_bytes();
    let cipher = XChaCha20Poly1305::new(&purpose_key(secret, purpose).into());
    // Encryption only fails for messages beyond the cipher's length limit (256 GiB).
    let sealed = cipher
        .encrypt(XNonce::from_slice(&nonce), plain)
        .unwrap_or_default();
    let mut token = Vec::with_capacity(NONCE_LEN + sealed.len());
    token.extend_from_slice(&nonce);
    token.extend_from_slice(&sealed);
    URL_SAFE_NO_PAD.encode(token)
}

/// Opens a value sealed for `purpose`; `None` when it was altered, sealed for another
/// purpose, or under another secret.
pub fn open(secret: &str, purpose: &str, sealed: &str) -> Option<Vec<u8>> {
    let raw = URL_SAFE_NO_PAD.decode(sealed).ok()?;
    let nonce = raw.get(..NONCE_LEN)?;
    let body = raw.get(NONCE_LEN..)?;
    XChaCha20Poly1305::new(&purpose_key(secret, purpose).into())
        .decrypt(XNonce::from_slice(nonce), body)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "BxVic2xATqUYX7g8UgEVQl/MU+DF57PUsRKqbop07yJKwjbf0bLH69WiPtbXtkHl";

    #[test]
    fn seals_per_purpose() {
        let sealed = seal(SECRET, "one", b"hello");
        assert_eq!(open(SECRET, "one", &sealed).unwrap(), b"hello");
        assert!(open(SECRET, "two", &sealed).is_none());
        assert!(open("another secret", "one", &sealed).is_none());
        let mut tampered = sealed.into_bytes();
        let last = tampered.len() - 1;
        tampered[last] = if tampered[last] == b'A' { b'B' } else { b'A' };
        assert!(open(SECRET, "one", &String::from_utf8(tampered).unwrap()).is_none());
    }

    #[test]
    fn compares_in_constant_time() {
        assert!(secure_compare(b"abc", b"abc"));
        assert!(!secure_compare(b"abc", b"abd"));
        assert!(!secure_compare(b"abc", b"ab"));
    }
}
