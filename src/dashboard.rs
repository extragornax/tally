use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::sync::Arc;

use crate::AppState;
use crate::auth::AdminAuth;
use crate::db::{self, Period};

const DASHBOARD_HTML: &str = include_str!("../templates/dashboard.html");
const SITE_HTML: &str = include_str!("../templates/site.html");

#[derive(Deserialize)]
pub struct PeriodQuery {
    #[serde(default)]
    pub period: Option<String>,
}

fn html(body: &'static str) -> Response {
    let mut resp = Response::new(axum::body::Body::from(body));
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    resp
}

pub async fn dashboard_page(_auth: AdminAuth) -> Response {
    html(DASHBOARD_HTML)
}

pub async fn site_page(_auth: AdminAuth, Path(_site): Path<String>) -> Response {
    html(SITE_HTML)
}

fn parse_period(q: &PeriodQuery) -> Period {
    Period::parse(q.period.as_deref().unwrap_or("30d"))
}

pub async fn api_stats(
    _auth: AdminAuth,
    State(state): State<Arc<AppState>>,
    Query(q): Query<PeriodQuery>,
) -> Result<Json<db::Stats>, (StatusCode, String)> {
    let period = parse_period(&q);
    db::stats_global(&state.db, period)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub async fn api_stats_site(
    _auth: AdminAuth,
    State(state): State<Arc<AppState>>,
    Path(site_id): Path<String>,
    Query(q): Query<PeriodQuery>,
) -> Result<Json<db::SiteStats>, (StatusCode, String)> {
    let period = parse_period(&q);
    db::stats_site(&state.db, &site_id, period)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub async fn api_sites(
    _auth: AdminAuth,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<db::SiteCount>>, (StatusCode, String)> {
    db::list_sites(&state.db)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}
