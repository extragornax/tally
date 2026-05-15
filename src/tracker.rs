use axum::{
    body::Body,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use chrono::{NaiveDate, Utc};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::AppState;
use crate::db::HitRecord;
use crate::geo;
use crate::ua;

const TRANSPARENT_GIF: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xFF, 0xFF, 0xFF,
    0x00, 0x00, 0x00, 0x21, 0xF9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2C, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3B,
];

pub type UniqueSet = std::sync::Mutex<HashMap<(String, NaiveDate), HashSet<String>>>;

pub fn pixel_response() -> Response {
    let mut resp = Response::new(Body::from(TRANSPARENT_GIF));
    let headers = resp.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/gif"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate, private"),
    );
    headers.insert(header::EXPIRES, HeaderValue::from_static("Thu, 01 Jan 1970 00:00:00 GMT"));
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    *resp.status_mut() = StatusCode::OK;
    resp
}

fn extract_ip(headers: &HeaderMap, addr: SocketAddr) -> String {
    if let Some(v) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = v.split(',').next() {
            let s = first.trim();
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    if let Some(v) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let s = v.trim();
        if !s.is_empty() {
            return s.to_string();
        }
    }
    addr.ip().to_string()
}

pub async fn pixel(
    Path(site_id): Path<String>,
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if let Some(allowed) = &state.sites {
        if !allowed.contains(&site_id) {
            let mut resp = pixel_response();
            *resp.status_mut() = StatusCode::NOT_FOUND;
            return resp;
        }
    }

    let referrer = headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let device = ua::classify_device(user_agent);
    if device == "bot" {
        return pixel_response();
    }

    let ip = extract_ip(&headers, addr);
    let now = Utc::now();
    let today = now.date_naive();
    let hash = ua::visitor_hash(&ip, &site_id, &today.to_string());

    let is_unique = {
        let mut map = state.uniques.lock().unwrap();
        map.retain(|(_, d), _| *d == today);
        let set = map.entry((site_id.clone(), today)).or_default();
        set.insert(hash.clone())
    };

    let country = geo::resolve_from_header(&headers, &state.geo_header)
        .unwrap_or_else(|| geo::resolve(state.geo.as_ref(), &ip));
    let referrer_domain = ua::extract_referrer_domain(referrer);
    let path = ua::extract_referrer_path(referrer);

    let rec = HitRecord {
        site_id,
        path,
        referrer_domain,
        country,
        device: device.to_string(),
        visitor_hash: hash,
        is_unique,
        created_at: now,
    };

    if let Err(e) = state.tx.try_send(rec) {
        tracing::warn!(error=?e, "hit channel full or closed; dropping hit");
    }

    pixel_response()
}
