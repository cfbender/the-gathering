//! The signed cookie session (`Plug.Session` with the cookie store) and CSRF protection.
//!
//! The cookie is byte-compatible with the Elixir server: an Erlang external-term map signed
//! with `MessageVerifier` under the endpoint's signing salt, so switching servers keeps
//! everyone signed in.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use eetf::{Atom, Binary, Map, Term};

use crate::crypto;
use crate::error::ApiError;
use crate::state::AppState;

/// The session cookie's name.
pub const COOKIE: &str = "_the_gathering_key";
const SIGNING_SALT: &str = "sQwWhYdP";
const MAX_AGE_SECONDS: i64 = 60 * 60 * 24 * 14;
const CSRF_KEY: &str = "_csrf_token";

#[derive(Debug, Default)]
struct Inner {
    data: HashMap<String, Term>,
    changed: bool,
    masked_csrf: Option<String>,
}

/// The request's session. Handlers read and write it; the middleware writes the cookie back
/// when anything changed.
#[derive(Clone, Debug, Default)]
pub struct Session(Arc<Mutex<Inner>>);

impl Session {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Decodes a signed session cookie; invalid cookies give an empty session.
    pub fn from_cookie(cookie: &str, secret_key_base: &str) -> Self {
        let key = crypto::derive_key(secret_key_base, SIGNING_SALT);
        let data = crypto::verify(cookie, &key)
            .and_then(|payload| crypto::binary_to_term(&payload))
            .and_then(|term| match term {
                Term::Map(map) => Some(
                    map.map
                        .into_iter()
                        .filter_map(|(key, value)| term_string(&key).map(|key| (key, value)))
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        Self(Arc::new(Mutex::new(Inner { data, changed: false, masked_csrf: None })))
    }

    /// Signs the session into a cookie value.
    pub fn to_cookie(&self, secret_key_base: &str) -> String {
        let inner = self.lock();
        let map: std::collections::HashMap<Term, Term> = inner
            .data
            .iter()
            .map(|(key, value)| (Term::Binary(Binary::from(key.as_bytes())), value.clone()))
            .collect();
        let payload = crypto::term_to_binary(&Term::Map(Map::from(map)));
        crypto::sign(&payload, &crypto::derive_key(secret_key_base, SIGNING_SALT))
    }

    /// A raw value.
    pub fn get(&self, key: &str) -> Option<Term> {
        self.lock().data.get(key).cloned()
    }

    /// A binary value (`get_session(conn, key)` for binaries and strings).
    pub fn get_bytes(&self, key: &str) -> Option<Vec<u8>> {
        match self.get(key)? {
            Term::Binary(binary) => Some(binary.bytes),
            _ => None,
        }
    }

    /// A UTF-8 string value.
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.get_bytes(key).and_then(|bytes| String::from_utf8(bytes).ok())
    }

    /// Stores a value.
    pub fn put(&self, key: &str, value: Term) {
        let mut inner = self.lock();
        inner.data.insert(key.to_owned(), value);
        inner.changed = true;
    }

    /// Stores a binary.
    pub fn put_bytes(&self, key: &str, value: &[u8]) {
        self.put(key, Term::Binary(Binary::from(value)));
    }

    /// Removes a value.
    pub fn delete(&self, key: &str) {
        let mut inner = self.lock();
        if inner.data.remove(key).is_some() {
            inner.changed = true;
        }
    }

    /// `clear_session/1` plus `configure_session(renew: true)`.
    pub fn clear(&self) {
        let mut inner = self.lock();
        inner.data.clear();
        inner.masked_csrf = None;
        inner.changed = true;
    }

    /// `get_csrf_token/0`: a masked form of the session's CSRF token, creating one if needed.
    /// Repeated calls within a request return the same masked token.
    pub fn csrf_token(&self) -> String {
        let mut inner = self.lock();
        if let Some(masked) = &inner.masked_csrf {
            return masked.clone();
        }
        let token = match inner.data.get(CSRF_KEY).and_then(term_string).filter(|token| token.len() == 24) {
            Some(token) => token,
            None => {
                let token = crypto::csrf::generate();
                inner.data.insert(CSRF_KEY.to_owned(), Term::Binary(Binary::from(token.as_bytes())));
                inner.changed = true;
                token
            }
        };
        let masked = crypto::csrf::mask(&token);
        inner.masked_csrf = Some(masked.clone());
        masked
    }

    /// `delete_csrf_token/0`: the next `csrf_token` call starts a new token.
    pub fn delete_csrf_token(&self) {
        let mut inner = self.lock();
        inner.masked_csrf = None;
        if inner.data.remove(CSRF_KEY).is_some() {
            inner.changed = true;
        }
    }

    fn csrf_valid(&self, submitted: &str) -> bool {
        self.lock()
            .data
            .get(CSRF_KEY)
            .and_then(term_string)
            .is_some_and(|token| crypto::csrf::valid(&token, submitted))
    }

    fn changed(&self) -> bool {
        self.lock().changed
    }
}

/// A binary or atom term as a string.
pub fn term_string(term: &Term) -> Option<String> {
    match term {
        Term::Binary(binary) => String::from_utf8(binary.bytes.clone()).ok(),
        Term::Atom(Atom { name }) => Some(name.clone()),
        _ => None,
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Session {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<Session>().cloned().ok_or(ApiError::Internal(anyhow::anyhow!("session layer missing")))
    }
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| {
            let (key, value) = pair.trim().split_once('=')?;
            (key == name).then(|| value.to_owned())
        })
}

/// Loads the session, runs the request, and writes the cookie back when it changed.
pub async fn session_layer(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let session = read_cookie(request.headers(), COOKIE)
        .map(|cookie| Session::from_cookie(&cookie, &state.config.secret_key_base))
        .unwrap_or_default();
    request.extensions_mut().insert(session.clone());
    let mut response = next.run(request).await;
    if session.changed() {
        let expires = time::OffsetDateTime::now_utc() + time::Duration::seconds(MAX_AGE_SECONDS);
        let expires = expires
            .format(&time::format_description::well_known::Rfc2822)
            .unwrap_or_default()
            .replace("+0000", "GMT");
        let cookie = format!(
            "{COOKIE}={}; path=/; expires={expires}; max-age={MAX_AGE_SECONDS}; HttpOnly; SameSite=Lax",
            session.to_cookie(&state.config.secret_key_base)
        );
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}

/// `protect_from_forgery`: state-changing requests must carry the masked token in
/// `x-csrf-token`. Failures are Phoenix's JSON 403.
pub async fn csrf_layer(request: Request, next: Next) -> Response {
    if matches!(*request.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return next.run(request).await;
    }
    let valid = match (request.extensions().get::<Session>(), request.headers().get("x-csrf-token")) {
        (Some(session), Some(token)) => token.to_str().is_ok_and(|token| session.csrf_valid(token)),
        _ => false,
    };
    if valid { next.run(request).await } else { ApiError::Forbidden.into_response() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_the_cookie() {
        let secret = "BxVic2xATqUYX7g8UgEVQl/MU+DF57PUsRKqbop07yJKwjbf0bLH69WiPtbXtkHl";
        let session = Session::default();
        session.put_bytes("user_token", &[7; 32]);
        let masked = session.csrf_token();
        let restored = Session::from_cookie(&session.to_cookie(secret), secret);
        assert_eq!(restored.get_bytes("user_token").unwrap(), vec![7; 32]);
        assert!(restored.csrf_valid(&masked));
    }
}
