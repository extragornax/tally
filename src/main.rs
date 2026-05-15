mod auth;
mod dashboard;
mod db;
mod geo;
mod tracker;
mod ua;

use anyhow::{Context, Result};
use axum::{Router, routing::get};
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::signal;
use tokio::sync::mpsc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::db::HitRecord;
use crate::geo::GeoReader;
use crate::tracker::UniqueSet;

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub tx: mpsc::Sender<HitRecord>,
    pub admin_token: String,
    pub sites: Option<HashSet<String>>,
    pub geo: Option<GeoReader>,
    pub geo_header: String,
    pub uniques: UniqueSet,
}

#[tokio::main]
async fn main() -> Result<()> {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info,tally=info".into());
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
        .with_target(false)
        .init();

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".into())
        .parse()
        .context("invalid PORT")?;
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "data/tally.db".into());
    let admin_token = std::env::var("ADMIN_TOKEN").context("ADMIN_TOKEN env var required")?;
    if admin_token.is_empty() {
        anyhow::bail!("ADMIN_TOKEN must not be empty");
    }
    let sites_raw = std::env::var("SITES").unwrap_or_default();
    let sites: Option<HashSet<String>> = if sites_raw.trim().is_empty() {
        None
    } else {
        Some(
            sites_raw
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        )
    };

    let geo = match std::env::var("GEOIP_DB").ok().filter(|s| !s.is_empty()) {
        Some(path) => match geo::open(&path) {
            Ok(r) => {
                tracing::info!(path = %path, "geoip loaded");
                Some(r)
            }
            Err(e) => {
                tracing::warn!(error = ?e, path = %path, "geoip load failed; continuing without");
                None
            }
        },
        None => None,
    };

    let conn = db::open(&db_path).context("db init")?;
    let db = Arc::new(Mutex::new(conn));

    let (tx, rx) = mpsc::channel::<HitRecord>(10_000);

    let geo_header = std::env::var("GEO_HEADER")
        .ok()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "cf-ipcountry".to_string());

    let state = Arc::new(AppState {
        db: db.clone(),
        tx,
        admin_token,
        sites,
        geo,
        geo_header,
        uniques: Mutex::new(HashMap::new()),
    });

    let writer_db = db.clone();
    let writer_handle = tokio::spawn(async move {
        db::batch_writer(rx, writer_db).await;
    });

    let cleanup_db = db.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(24 * 3600));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match db::cleanup_old(&cleanup_db, 365) {
                Ok(n) if n > 0 => tracing::info!(deleted = n, "cleanup ran"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = ?e, "cleanup failed"),
            }
        }
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(dashboard::health))
        .route("/t/{site_id}", get(tracker::pixel))
        .route("/", get(dashboard::dashboard_page))
        .route("/site/{site_id}", get(dashboard::site_page))
        .route("/api/stats", get(dashboard::api_stats))
        .route("/api/stats/{site_id}", get(dashboard::api_stats_site))
        .route("/api/sites", get(dashboard::api_sites))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(%addr, "tally listening");
    let listener = tokio::net::TcpListener::bind(addr).await.context("bind")?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("serve")?;

    drop(state);
    let _ = writer_handle.await;
    tracing::info!("bye");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = signal::unix::signal(signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };

    #[cfg(not(unix))]
    let term = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
    tracing::info!("shutdown signal received");
}
