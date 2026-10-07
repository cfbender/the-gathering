//! The browser session and CSRF protection.
//!
//! The session is a typed [`SessionData`] stored as JSON in an encrypted, authenticated
//! cookie (axum-extra's private cookie jar). Signing in stores a `users_tokens` token there;
//! the token, not the cookie, is what grants access, so logging out or an administrator's
//! revocation ends a session even if the browser keeps the cookie.
//!
//! State-changing requests must echo the session's CSRF token in `x-csrf-token`. The SPA
//! shell renders the token into a meta tag, and sign-in and sign-out responses carry the
//! new one in that header.

use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Cookie, Key, PrivateCookieJar, SameSite};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

use crate::crypto;
use crate::error::ApiError;
use crate::legacy;
use crate::state::AppState;

/// The session cookie's name.
pub const COOKIE: &str = "the_gathering_session";
const MAX_AGE: time::Duration = time::Duration::days(14);

/// What a browser session holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionData {
    /// The CSRF token state-changing requests must echo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub csrf_token: Option<String>,
    /// The signed-in user's `users_tokens` token.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "base64_bytes"
    )]
    pub user_token: Option<Vec<u8>>,
    /// A registration invitation the visitor opened, as its hash.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "base64_bytes"
    )]
    pub registration_invite_hash: Option<Vec<u8>>,
    /// A Discord sign-in in progress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discord_oauth: Option<DiscordOAuthAttempt>,
}

/// A Discord OAuth sign-in between the redirect to Discord and its callback.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscordOAuthAttempt {
    /// The `state` parameter the callback must return.
    pub state: String,
    /// Where to go after signing in.
    pub return_to: String,
    /// For re-verification (sudo), the Discord account that must sign in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sudo_discord_id: Option<String>,
    /// The invitation the visitor opened before starting, if any.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "base64_bytes"
    )]
    pub registration_invite_hash: Option<Vec<u8>>,
}

mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    #[allow(clippy::ref_option)]
    pub fn serialize<S: Serializer>(
        bytes: &Option<Vec<u8>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match bytes {
            Some(bytes) => serializer.serialize_str(&crate::crypto::url_encode64_unpadded(bytes)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Vec<u8>>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|text| {
                crate::crypto::url_decode64_unpadded(&text)
                    .ok_or_else(|| serde::de::Error::custom("invalid base64"))
            })
            .transpose()
    }
}

/// The key for the session cookie: SHA-512 over a purpose label and the server secret.
pub fn cookie_key(secret: &str) -> anyhow::Result<Key> {
    let mut hasher = Sha512::new();
    hasher.update(b"the-gathering.session.v1\0");
    hasher.update(secret.as_bytes());
    Ok(Key::try_from(hasher.finalize().as_slice())?)
}

/// Encrypts session data into a cookie value.
pub fn encode_cookie(key: &Key, data: &SessionData) -> String {
    let json = serde_json::to_string(data).unwrap_or_default();
    let response = PrivateCookieJar::new(key.clone())
        .add(Cookie::new(COOKIE, json))
        .into_response();
    response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .and_then(|pair| pair.strip_prefix(&format!("{COOKIE}=")))
        .unwrap_or_default()
        .to_owned()
}

/// Decrypts a cookie value as sent in a `Cookie` header; `None` when it was altered or
/// written under another key.
pub fn decode_cookie(key: &Key, value: &str) -> Option<SessionData> {
    let cookie = Cookie::parse_encoded(format!("{COOKIE}={value}")).ok()?;
    let cookie = PrivateCookieJar::new(key.clone()).decrypt(cookie.into_owned())?;
    serde_json::from_str(cookie.value()).ok()
}

#[derive(Debug, Default)]
struct Inner {
    data: SessionData,
    changed: bool,
}

/// The request's session. Handlers read and change it; the middleware writes the cookie
/// back when anything changed.
#[derive(Clone, Debug, Default)]
pub struct Session(Arc<Mutex<Inner>>);

impl Session {
    /// A session holding `data`, unchanged so far.
    pub fn new(data: SessionData) -> Self {
        Self(Arc::new(Mutex::new(Inner {
            data,
            changed: false,
        })))
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A copy of the data.
    pub fn data(&self) -> SessionData {
        self.lock().data.clone()
    }

    /// Changes the data.
    pub fn update<T>(&self, change: impl FnOnce(&mut SessionData) -> T) -> T {
        let mut inner = self.lock();
        inner.changed = true;
        change(&mut inner.data)
    }

    /// The signed-in user's session token.
    pub fn user_token(&self) -> Option<Vec<u8>> {
        self.lock().data.user_token.clone()
    }

    /// Starts over with an empty session (and so a new CSRF token).
    pub fn renew(&self) {
        self.update(|data| *data = SessionData::default());
    }

    /// The session's CSRF token, creating one if needed.
    pub fn csrf_token(&self) -> String {
        let mut inner = self.lock();
        if let Some(token) = &inner.data.csrf_token {
            return token.clone();
        }
        let token = crypto::url_encode64_unpadded(&crypto::random_bytes::<32>());
        inner.data.csrf_token = Some(token.clone());
        inner.changed = true;
        token
    }

    fn csrf_valid(&self, submitted: &str) -> bool {
        self.lock()
            .data
            .csrf_token
            .as_deref()
            .is_some_and(|token| crypto::secure_compare(token.as_bytes(), submitted.as_bytes()))
    }

    fn changed(&self) -> bool {
        self.lock().changed
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Session {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(
            parts
                .extensions
                .get::<Session>()
                .cloned()
                .ok_or(ApiError::Internal(anyhow::anyhow!("session layer missing"))),
        )
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
///
/// A browser that still holds the cookie of an earlier release keeps its sign-in: the
/// session token is carried into a new cookie and the old one is expired.
pub async fn session_layer(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let jar = PrivateCookieJar::from_headers(request.headers(), state.session_key.clone());
    let current = jar
        .get(COOKIE)
        .and_then(|cookie| serde_json::from_str::<SessionData>(cookie.value()).ok());
    let legacy_cookie = read_cookie(request.headers(), legacy::SESSION_COOKIE);
    let session = match current {
        Some(data) => Session::new(data),
        None => {
            let session = Session::default();
            if let Some(token) = legacy_cookie
                .as_deref()
                .and_then(|cookie| legacy::session_user_token(cookie, &state.config.secret_key))
            {
                session.update(|data| data.user_token = Some(token));
            }
            session
        }
    };
    request.extensions_mut().insert(session.clone());
    let mut response = next.run(request).await;
    if session.changed() {
        let json = serde_json::to_string(&session.data()).unwrap_or_default();
        let cookie = Cookie::build((COOKIE, json))
            .path("/")
            .http_only(true)
            .same_site(SameSite::Lax)
            .max_age(MAX_AGE);
        response = (jar.add(cookie), response).into_response();
    }
    if legacy_cookie.is_some() {
        let removal = Cookie::build((legacy::SESSION_COOKIE, ""))
            .path("/")
            .max_age(time::Duration::ZERO);
        if let Ok(value) = HeaderValue::from_str(&removal.to_string()) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}

/// State-changing requests must carry the session's CSRF token in `x-csrf-token`;
/// anything else is a JSON 403.
pub async fn csrf_layer(request: Request, next: Next) -> Response {
    if matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) {
        return next.run(request).await;
    }
    let valid = match (
        request.extensions().get::<Session>(),
        request.headers().get("x-csrf-token"),
    ) {
        (Some(session), Some(token)) => token.to_str().is_ok_and(|token| session.csrf_valid(token)),
        _ => false,
    };
    if valid {
        next.run(request).await
    } else {
        ApiError::Forbidden.into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "BxVic2xATqUYX7g8UgEVQl/MU+DF57PUsRKqbop07yJKwjbf0bLH69WiPtbXtkHl";

    #[test]
    fn round_trips_through_the_cookie() {
        let key = cookie_key(SECRET).unwrap();
        let data = SessionData {
            csrf_token: Some("token".into()),
            user_token: Some(vec![7; 32]),
            registration_invite_hash: None,
            discord_oauth: Some(DiscordOAuthAttempt {
                state: "state".into(),
                return_to: "/games".into(),
                sudo_discord_id: Some("123".into()),
                registration_invite_hash: Some(vec![1, 2, 3]),
            }),
        };
        let cookie = encode_cookie(&key, &data);
        assert!(!cookie.contains("token"));
        assert_eq!(decode_cookie(&key, &cookie).unwrap(), data);
        assert!(decode_cookie(&cookie_key("another secret").unwrap(), &cookie).is_none());
        assert!(decode_cookie(&key, &format!("x{cookie}")).is_none());
    }

    #[test]
    fn checks_csrf_tokens_exactly() {
        let session = Session::default();
        let token = session.csrf_token();
        assert_eq!(session.csrf_token(), token);
        assert!(session.csrf_valid(&token));
        assert!(!session.csrf_valid(&format!("{token}x")));
        session.renew();
        assert!(!session.csrf_valid(&token));
    }
}
