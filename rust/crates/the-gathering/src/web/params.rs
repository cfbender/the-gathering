//! Request params as Phoenix merges them: the query string (with Plug's `a[b]=c` and
//! `a[]=c` nesting) overlaid by a JSON object body.

use axum::body::Bytes;
use axum::extract::{FromRequest, Request};
use axum::http::header;
use serde_json::{Map, Value};

use crate::error::ApiError;

/// Merged query and JSON body params.
#[derive(Clone, Debug, Default)]
pub struct Params(pub Value);

impl Params {
    /// A param.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    /// A string param.
    pub fn str(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(Value::as_str)
    }

    /// An object param, such as `%{"user" => attrs}`.
    pub fn object(&self, key: &str) -> Option<&Value> {
        self.0.get(key).filter(|value| value.is_object())
    }
}

/// `Plug.Conn.Query.decode/1`.
pub fn decode_query(query: &str) -> Map<String, Value> {
    let mut params = Map::new();
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        insert_nested(&mut params, &key, Value::String(value.into_owned()));
    }
    params
}

fn insert_nested(params: &mut Map<String, Value>, key: &str, value: Value) {
    let Some((head, rest)) = key.split_once('[') else {
        params.insert(key.to_owned(), value);
        return;
    };
    let Some((inner, tail)) = rest.split_once(']') else {
        params.insert(key.to_owned(), value);
        return;
    };
    if inner.is_empty() && tail.is_empty() {
        match params
            .entry(head.to_owned())
            .or_insert_with(|| Value::Array(Vec::new()))
        {
            Value::Array(items) => items.push(value),
            other => *other = Value::Array(vec![value]),
        }
        return;
    }
    let child = params
        .entry(head.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !child.is_object() {
        *child = Value::Object(Map::new());
    }
    if let Value::Object(child) = child {
        insert_nested(child, &format!("{inner}{tail}"), value);
    }
}

impl<S: Send + Sync> FromRequest<S> for Params {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let mut params = request.uri().query().map(decode_query).unwrap_or_default();
        let is_json = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json") || value.contains("+json"));
        let body = Bytes::from_request(request, state)
            .await
            .map_err(|_| ApiError::BadRequest)?;
        if is_json && !body.is_empty() {
            match serde_json::from_slice::<Value>(&body).map_err(|_| ApiError::BadRequest)? {
                Value::Object(object) => params.extend(object),
                // Plug wraps non-object JSON bodies in `_json`.
                other => {
                    params.insert("_json".to_owned(), other);
                }
            }
        }
        Ok(Self(Value::Object(params)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decodes_nested_queries() {
        assert_eq!(
            Value::Object(decode_query("a=1&b[c]=2&d[]=3&d[]=4&q=Sol+Ring")),
            json!({"a": "1", "b": {"c": "2"}, "d": ["3", "4"], "q": "Sol Ring"})
        );
    }
}
