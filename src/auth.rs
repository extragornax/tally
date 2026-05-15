use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use std::sync::Arc;

use crate::AppState;

pub struct AdminAuth;

impl FromRequestParts<Arc<AppState>> for AdminAuth {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        match extract_token(parts) {
            Some(t) if constant_time_eq(t.as_bytes(), state.admin_token.as_bytes()) => Ok(AdminAuth),
            _ => Err((StatusCode::UNAUTHORIZED, "unauthorized")),
        }
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

fn extract_token(parts: &Parts) -> Option<String> {
    if let Some(q) = parts.uri.query() {
        for kv in q.split('&') {
            let mut it = kv.splitn(2, '=');
            if let (Some(k), Some(v)) = (it.next(), it.next()) {
                if k == "token" {
                    return Some(urldecode(v));
                }
            }
        }
    }
    if let Some(cookie) = parts.headers.get(axum::http::header::COOKIE) {
        if let Ok(s) = cookie.to_str() {
            for part in s.split(';') {
                let part = part.trim();
                if let Some(rest) = part.strip_prefix("tally_admin=") {
                    return Some(rest.to_string());
                }
            }
        }
    }
    None
}

fn urldecode(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(b);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}
