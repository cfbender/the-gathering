//! Plug.Crypto-compatible signing and encryption, so cookies, socket tokens, and stored
//! credentials written by the Elixir server stay readable (and vice versa).
//!
//! * [`derive_key`]: `Plug.Crypto.KeyGenerator` (PBKDF2-HMAC-SHA256, 1000 iterations, 32 bytes).
//! * [`sign`]/[`verify`]: `Plug.Crypto.MessageVerifier` (`SFMyNTY.<payload>.<mac>`).
//! * [`encrypt`]/[`decrypt`]: `Plug.Crypto.encrypt/4` and `decrypt/4`
//!   (XChaCha20-Poly1305 `XCP.` tokens around `term_to_binary({data, signed_at_ms, max_age})`).

use base64::Engine as _;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use eetf::{Atom, BigInteger, Binary, FixInteger, Term, Tuple};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// `MessageEncryptor`'s default additional data, which `Plug.Crypto.encrypt/4` uses.
const DEFAULT_AAD: &[u8] = b"A128GCM";

/// `Plug.Crypto.KeyGenerator.generate(secret_key_base, salt)`.
pub fn derive_key(secret_key_base: &str, salt: &str) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(secret_key_base.as_bytes(), salt.as_bytes(), 1000, &mut key);
    key
}

fn hmac(key: &[u8], message: &[u8]) -> Vec<u8> {
    // HMAC accepts keys of any length, so construction cannot fail.
    match <HmacSha256 as Mac>::new_from_slice(key) {
        Ok(mut mac) => {
            mac.update(message);
            mac.finalize().into_bytes().to_vec()
        }
        Err(_) => Vec::new(),
    }
}

/// `MessageVerifier.sign(payload, key)` with SHA-256.
pub fn sign(payload: &[u8], key: &[u8]) -> String {
    let plain = format!("SFMyNTY.{}", URL_SAFE_NO_PAD.encode(payload));
    let signature = hmac(key, plain.as_bytes());
    format!("{plain}.{}", URL_SAFE_NO_PAD.encode(signature))
}

/// `MessageVerifier.verify(signed, key)`; only SHA-256 tokens are accepted.
pub fn verify(signed: &str, key: &[u8]) -> Option<Vec<u8>> {
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
    let challenge = hmac(key, format!("{protected}.{payload}").as_bytes());
    secure_compare(&challenge, &signature).then_some(decoded)
}

/// Constant-time equality (`Plug.Crypto.secure_compare/2`).
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

/// URL-safe base64 with padding (`Base.url_encode64/1`).
pub fn url_encode64(bytes: &[u8]) -> String {
    URL_SAFE.encode(bytes)
}

/// URL-safe base64 without padding (`Base.url_encode64(bytes, padding: false)`).
pub fn url_encode64_unpadded(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decodes URL-safe base64 with padding.
pub fn url_decode64(value: &str) -> Option<Vec<u8>> {
    URL_SAFE.decode(value).ok()
}

/// `MessageEncryptor.encrypt(message, aad, secret)` (the `XCP.` format).
pub fn encrypt_message(message: &[u8], aad: &[u8], secret: &[u8; 32]) -> String {
    let iv: [u8; 24] = random_bytes();
    let cipher = XChaCha20Poly1305::new(secret.into());
    let Ok(sealed) = cipher.encrypt(XNonce::from_slice(&iv), Payload { msg: message, aad }) else {
        return String::new();
    };
    // RustCrypto appends the tag; Plug puts it between the IV and the cipher text.
    let split = sealed.len().saturating_sub(16);
    let (cipher_text, tag) = sealed.split_at(split);
    let mut token = Vec::with_capacity(24 + sealed.len());
    token.extend_from_slice(&iv);
    token.extend_from_slice(tag);
    token.extend_from_slice(cipher_text);
    format!("XCP.{}", URL_SAFE_NO_PAD.encode(token))
}

/// `MessageEncryptor.decrypt(encrypted, aad, secret)` for `XCP.` tokens.
pub fn decrypt_message(encrypted: &str, aad: &[u8], secret: &[u8; 32]) -> Option<Vec<u8>> {
    let raw = URL_SAFE_NO_PAD
        .decode(encrypted.strip_prefix("XCP.")?)
        .ok()?;
    let iv = raw.get(..24)?;
    let tag = raw.get(24..40)?;
    let cipher_text = raw.get(40..)?;
    let mut sealed = Vec::with_capacity(cipher_text.len() + 16);
    sealed.extend_from_slice(cipher_text);
    sealed.extend_from_slice(tag);
    XChaCha20Poly1305::new(secret.into())
        .decrypt(XNonce::from_slice(iv), Payload { msg: &sealed, aad })
        .ok()
}

fn now_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis()),
    )
    .unwrap_or(i64::MAX)
}

fn integer(value: i64) -> Term {
    match i32::try_from(value) {
        Ok(small) => Term::FixInteger(FixInteger { value: small }),
        Err(_) => Term::BigInteger(BigInteger::from(value)),
    }
}

fn term_i64(term: &Term) -> Option<i64> {
    match term {
        Term::FixInteger(FixInteger { value }) => Some(i64::from(*value)),
        Term::BigInteger(big) => i64::try_from(&big.value).ok(),
        _ => None,
    }
}

/// Erlang `term_to_binary/1`.
pub fn term_to_binary(term: &Term) -> Vec<u8> {
    let mut out = Vec::new();
    // Encoding into a Vec only fails for unencodable terms, which we never build.
    let _ = term.encode(&mut out);
    out
}

/// `Plug.Crypto.non_executable_binary_to_term/1`: refuses functions, pids, ports, and refs.
pub fn binary_to_term(bytes: &[u8]) -> Option<Term> {
    let term = Term::decode(bytes).ok()?;
    executable_free(&term).then_some(term)
}

fn executable_free(term: &Term) -> bool {
    match term {
        Term::ExternalFun(_)
        | Term::InternalFun(_)
        | Term::Pid(_)
        | Term::Port(_)
        | Term::Reference(_) => false,
        Term::List(list) => list.elements.iter().all(executable_free),
        Term::ImproperList(list) => {
            list.elements.iter().all(executable_free) && executable_free(&list.last)
        }
        Term::Tuple(tuple) => tuple.elements.iter().all(executable_free),
        Term::Map(map) => map
            .map
            .iter()
            .all(|(key, value)| executable_free(key) && executable_free(value)),
        _ => true,
    }
}

/// `Plug.Crypto.encrypt(secret_key_base, salt, data)` for a binary payload, valid for
/// `max_age_seconds` (Plug's default is one day).
pub fn encrypt(secret_key_base: &str, salt: &str, data: &[u8], max_age_seconds: i64) -> String {
    let payload = Term::Tuple(Tuple::from(vec![
        Term::Binary(Binary::from(data)),
        integer(now_ms()),
        integer(max_age_seconds),
    ]));
    encrypt_message(
        &term_to_binary(&payload),
        DEFAULT_AAD,
        &derive_key(secret_key_base, salt),
    )
}

/// `Plug.Crypto.decrypt(secret_key_base, salt, token, max_age: ...)` for binary payloads.
/// `max_age_seconds` of `None` uses the age stored in the token (`:infinity` when `Some(i64::MAX)`).
pub fn decrypt(
    secret_key_base: &str,
    salt: &str,
    token: &str,
    max_age_seconds: Option<i64>,
) -> Option<Vec<u8>> {
    let plain = decrypt_message(token, DEFAULT_AAD, &derive_key(secret_key_base, salt))?;
    let Term::Tuple(Tuple { elements }) = binary_to_term(&plain)? else {
        return None;
    };
    let [Term::Binary(data), signed, stored_max_age] = elements.as_slice() else {
        return None;
    };
    let signed = term_i64(signed)?;
    let max_age = match max_age_seconds {
        Some(age) => age,
        None => match stored_max_age {
            Term::Atom(Atom { name }) if name == "infinity" => i64::MAX,
            other => term_i64(other)?,
        },
    };
    if max_age == i64::MAX {
        return Some(data.bytes.clone());
    }
    if max_age <= 0 || signed.saturating_add(max_age.saturating_mul(1000)) < now_ms() {
        return None;
    }
    Some(data.bytes.clone())
}

/// Plug's masked CSRF tokens. The session stores an unmasked 24-character token; pages get
/// `url_encode64(token XOR mask) <> mask`, a fresh mask each time.
pub mod csrf {
    use super::{URL_SAFE, random_bytes, secure_compare};
    use base64::Engine as _;

    /// A new unmasked session token (`Base.url_encode64(:crypto.strong_rand_bytes(18))`).
    pub fn generate() -> String {
        URL_SAFE.encode(random_bytes::<18>())
    }

    fn xor(left: &[u8], right: &[u8]) -> Vec<u8> {
        left.iter().zip(right).map(|(a, b)| a ^ b).collect()
    }

    /// Masks a session token for a page or response header.
    pub fn mask(token: &str) -> String {
        let mask = generate();
        format!(
            "{}{mask}",
            URL_SAFE.encode(xor(token.as_bytes(), mask.as_bytes()))
        )
    }

    /// Whether `submitted` is a masked form of `session_token`.
    pub fn valid(session_token: &str, submitted: &str) -> bool {
        if session_token.len() != 24 || submitted.len() != 56 {
            return false;
        }
        let (Some(masked), Some(mask)) = (submitted.get(..32), submitted.get(32..)) else {
            return false;
        };
        let Ok(user_token) = URL_SAFE.decode(masked) else {
            return false;
        };
        user_token.len() == 24
            && secure_compare(&xor(session_token.as_bytes(), &user_token), mask.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "BxVic2xATqUYX7g8UgEVQl/MU+DF57PUsRKqbop07yJKwjbf0bLH69WiPtbXtkHl";

    #[test]
    fn signs_and_verifies() {
        let key = derive_key(SECRET, "sQwWhYdP");
        let signed = sign(b"hello", &key);
        assert!(signed.starts_with("SFMyNTY."));
        assert_eq!(verify(&signed, &key).unwrap(), b"hello");
        assert!(verify(&signed.replace("SFMyNTY.", "SFMyNTY.x"), &key).is_none());
    }

    #[test]
    fn encrypts_round_trip_and_expires() {
        let token = encrypt(SECRET, "salt", b"secret", 60);
        assert_eq!(decrypt(SECRET, "salt", &token, None).unwrap(), b"secret");
        assert!(decrypt(SECRET, "other", &token, None).is_none());
        assert!(decrypt(SECRET, "salt", &token, Some(0)).is_none());
    }

    /// Vectors produced by Elixir: `KeyGenerator.generate("password", "salt")`,
    /// `Plug.Crypto.encrypt(secret, "the_gathering.accounts.encrypted_string", "mv-key")`, and a
    /// Plug session cookie signed with the endpoint's signing salt.
    #[test]
    fn reads_tokens_written_by_elixir() {
        assert_eq!(
            hex(&derive_key("password", "salt")),
            "632c2812e46d4604102ba7618e9d6d7d2f8128f6266b4a03264d2a0460b7dcb3"
        );
        let stored = "XCP.AMB52kLujURW-VRD3fCoZ-IaLvueQYDiiEVPAKF_dWuq7QCoKbEj8syhxNrdBVZE5BxgIxr1ekz1vZuRihpd6tyyQPg";
        assert_eq!(
            decrypt(
                SECRET,
                "the_gathering.accounts.encrypted_string",
                stored,
                Some(i64::MAX)
            )
            .unwrap(),
            b"mv-key"
        );
        let cookie = "SFMyNTY.g3QAAAADbQAAAAtfY3NyZl90b2tlbm0AAAAYQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBbQAAAA1kaXNjb3JkX29hdXRodAAAAAN3CXJldHVybl90b20AAAABL3cPc3Vkb19kaXNjb3JkX2lkdwNuaWx3DnNlc3Npb25fcGFyYW1zdAAAAAF3BXN0YXRlbQAAAAJzdG0AAAAKdXNlcl90b2tlbm0AAAAgBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc.pUDs6sz-57hgvqOMKePxBo6BZ9Sax-r67oGkDY7iRP4";
        let payload = verify(cookie, &derive_key(SECRET, "sQwWhYdP")).unwrap();
        let Term::Map(map) = binary_to_term(&payload).unwrap() else {
            panic!("not a map")
        };
        assert_eq!(map.map.len(), 3);
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::new(), |mut out, b| {
            use std::fmt::Write as _;
            let _ = write!(out, "{b:02x}");
            out
        })
    }

    #[test]
    fn csrf_masks_validate() {
        let token = csrf::generate();
        assert_eq!(token.len(), 24);
        let masked = csrf::mask(&token);
        assert_eq!(masked.len(), 56);
        assert!(csrf::valid(&token, &masked));
        assert!(!csrf::valid(&csrf::generate(), &masked));
    }
}
