//! Read-only decoders for data written by releases up to 0.2 (and the Elixir server before
//! them), kept so upgrades neither sign anyone out nor lose stored credentials:
//!
//! * the `_the_gathering_key` session cookie, a Plug `MessageVerifier` token (`SFMyNTY.`)
//!   around an Erlang external-term map, whose `user_token` is carried into the new session
//!   cookie on the first request after the upgrade;
//! * `XCP.` encrypted credentials (`Plug.Crypto.encrypt/4`: XChaCha20-Poly1305 around an
//!   Erlang `{data, signed_at, max_age}` tuple), re-encrypted to the native format at boot.
//!
//! Nothing here writes these formats. Removing this module is tracked as DRAFT-2 in the
//! backlog, once installs have upgraded through a release that contains it.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use eetf::{Binary, Term, Tuple};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::crypto::secure_compare;

/// The legacy session cookie's name.
pub const SESSION_COOKIE: &str = "_the_gathering_key";
const SESSION_SIGNING_SALT: &str = "sQwWhYdP";
const SECRET_SALT: &str = "the_gathering.accounts.encrypted_string";
/// The additional data Plug's encryptor authenticated.
const ENCRYPTION_AAD: &[u8] = b"A128GCM";

/// Plug's key generator: PBKDF2-HMAC-SHA256, 1000 iterations, 32 bytes.
fn derive_key(secret: &str, salt: &str) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(secret.as_bytes(), salt.as_bytes(), 1000, &mut key);
    key
}

/// Verifies an `SFMyNTY.<payload>.<mac>` token and returns the payload.
fn verify(signed: &str, key: &[u8]) -> Option<Vec<u8>> {
    let mut parts = signed.split('.');
    let (Some(protected), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    if protected != "SFMyNTY" {
        return None;
    }
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let signature = URL_SAFE_NO_PAD.decode(signature).ok()?;
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).ok()?;
    mac.update(format!("{protected}.{payload}").as_bytes());
    secure_compare(&mac.finalize().into_bytes(), &signature).then_some(decoded)
}

/// Decodes an Erlang term, refusing functions, pids, ports, and references.
fn decode_term(bytes: &[u8]) -> Option<Term> {
    fn inert(term: &Term) -> bool {
        match term {
            Term::ExternalFun(_)
            | Term::InternalFun(_)
            | Term::Pid(_)
            | Term::Port(_)
            | Term::Reference(_) => false,
            Term::List(list) => list.elements.iter().all(inert),
            Term::ImproperList(list) => list.elements.iter().all(inert) && inert(&list.last),
            Term::Tuple(tuple) => tuple.elements.iter().all(inert),
            Term::Map(map) => map
                .map
                .iter()
                .all(|(key, value)| inert(key) && inert(value)),
            _ => true,
        }
    }
    let term = Term::decode(bytes).ok()?;
    inert(&term).then_some(term)
}

/// The session token in a legacy session cookie, if the cookie verifies under `secret`.
pub fn session_user_token(cookie: &str, secret: &str) -> Option<Vec<u8>> {
    let payload = verify(cookie, &derive_key(secret, SESSION_SIGNING_SALT))?;
    let Term::Map(map) = decode_term(&payload)? else {
        return None;
    };
    match map
        .map
        .get(&Term::Binary(Binary::from(b"user_token".as_slice())))?
    {
        Term::Binary(token) => Some(token.bytes.clone()),
        _ => None,
    }
}

/// Whether `stored` is a legacy encrypted credential.
pub fn is_legacy_secret(stored: &str) -> bool {
    stored.starts_with("XCP.")
}

/// Decrypts a legacy `XCP.` credential. Stored credentials never expired, so the signing
/// time and age in the token are ignored.
pub fn decrypt_secret(stored: &str, secret: &str) -> Option<Vec<u8>> {
    let raw = URL_SAFE_NO_PAD.decode(stored.strip_prefix("XCP.")?).ok()?;
    // Plug's layout is IV (24 bytes), tag (16), then cipher text; RustCrypto wants the tag last.
    let iv = raw.get(..24)?;
    let tag = raw.get(24..40)?;
    let cipher_text = raw.get(40..)?;
    let mut sealed = Vec::with_capacity(cipher_text.len() + tag.len());
    sealed.extend_from_slice(cipher_text);
    sealed.extend_from_slice(tag);
    let key = derive_key(secret, SECRET_SALT);
    let plain = XChaCha20Poly1305::new(&key.into())
        .decrypt(
            XNonce::from_slice(iv),
            Payload {
                msg: &sealed,
                aad: ENCRYPTION_AAD,
            },
        )
        .ok()?;
    let Term::Tuple(Tuple { elements }) = decode_term(&plain)? else {
        return None;
    };
    match elements.first()? {
        Term::Binary(data) => Some(data.bytes.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "BxVic2xATqUYX7g8UgEVQl/MU+DF57PUsRKqbop07yJKwjbf0bLH69WiPtbXtkHl";

    /// A session cookie the Elixir server signed: `_csrf_token`, `discord_oauth`, and a
    /// `user_token` of 32 bytes of 7.
    pub const ELIXIR_COOKIE: &str = "SFMyNTY.g3QAAAADbQAAAAtfY3NyZl90b2tlbm0AAAAYQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBbQAAAA1kaXNjb3JkX29hdXRodAAAAAN3CXJldHVybl90b20AAAABL3cPc3Vkb19kaXNjb3JkX2lkdwNuaWx3DnNlc3Npb25fcGFyYW1zdAAAAAF3BXN0YXRlbQAAAAJzdG0AAAAKdXNlcl90b2tlbm0AAAAgBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc.pUDs6sz-57hgvqOMKePxBo6BZ9Sax-r67oGkDY7iRP4";

    fn hex(bytes: &[u8]) -> String {
        use std::fmt::Write as _;
        bytes.iter().fold(String::new(), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
    }

    #[test]
    fn derives_plug_keys() {
        let key = derive_key("password", "salt");
        assert_eq!(
            hex(&key),
            "632c2812e46d4604102ba7618e9d6d7d2f8128f6266b4a03264d2a0460b7dcb3"
        );
    }

    #[test]
    fn reads_the_user_token_from_an_elixir_cookie() {
        assert_eq!(
            session_user_token(ELIXIR_COOKIE, SECRET).unwrap(),
            vec![7; 32]
        );
        assert!(session_user_token(ELIXIR_COOKIE, "another secret").is_none());
        assert!(session_user_token(&ELIXIR_COOKIE.replace(".pUD", ".xUD"), SECRET).is_none());
    }

    #[test]
    fn decrypts_an_elixir_credential() {
        let stored = "XCP.AMB52kLujURW-VRD3fCoZ-IaLvueQYDiiEVPAKF_dWuq7QCoKbEj8syhxNrdBVZE5BxgIxr1ekz1vZuRihpd6tyyQPg";
        assert!(is_legacy_secret(stored));
        assert_eq!(decrypt_secret(stored, SECRET).unwrap(), b"mv-key");
        assert!(decrypt_secret(stored, "another secret").is_none());
    }
}
