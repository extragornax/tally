use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time;

#[derive(Debug, Clone)]
pub struct HitRecord {
    pub site_id: String,
    pub path: String,
    pub referrer_domain: String,
    pub country: String,
    pub device: String,
    pub visitor_hash: String,
    pub is_unique: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub enum Period {
    D7,
    D30,
    All,
}

impl Period {
    pub fn parse(s: &str) -> Self {
        match s {
            "7d" => Period::D7,
            "all" => Period::All,
            _ => Period::D30,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Period::D7 => "7d",
            Period::D30 => "30d",
            Period::All => "all",
        }
    }

    pub fn since(&self) -> DateTime<Utc> {
        match self {
            Period::D7 => Utc::now() - Duration::days(7),
            Period::D30 => Utc::now() - Duration::days(30),
            Period::All => DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
        }
    }

    pub fn prev_window(&self) -> (DateTime<Utc>, DateTime<Utc>) {
        let now = Utc::now();
        match self {
            Period::D7 => (now - Duration::days(14), now - Duration::days(7)),
            Period::D30 => (now - Duration::days(60), now - Duration::days(30)),
            Period::All => (
                DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
                DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
            ),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SiteCount {
    pub site_id: String,
    pub views: i64,
    pub uniques: i64,
}

#[derive(Debug, Serialize)]
pub struct DailyCount {
    pub date: String,
    pub views: i64,
    pub uniques: i64,
}

#[derive(Debug, Serialize)]
pub struct RefCount {
    pub domain: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct CountryCount {
    pub code: String,
    pub count: i64,
}

#[derive(Debug, Serialize, Default)]
pub struct DeviceCounts {
    pub desktop: i64,
    pub mobile: i64,
    pub tablet: i64,
}

#[derive(Debug, Serialize)]
pub struct PageCount {
    pub path: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub period: String,
    pub total_views: i64,
    pub total_uniques: i64,
    pub prev_period_views: i64,
    pub prev_period_uniques: i64,
    pub by_site: Vec<SiteCount>,
    pub daily: Vec<DailyCount>,
    pub referrers: Vec<RefCount>,
    pub countries: Vec<CountryCount>,
    pub devices: DeviceCounts,
}

#[derive(Debug, Serialize)]
pub struct SiteStats {
    pub site_id: String,
    pub period: String,
    pub total_views: i64,
    pub total_uniques: i64,
    pub prev_period_views: i64,
    pub prev_period_uniques: i64,
    pub daily: Vec<DailyCount>,
    pub referrers: Vec<RefCount>,
    pub countries: Vec<CountryCount>,
    pub devices: DeviceCounts,
    pub top_pages: Vec<PageCount>,
    pub by_hour: Vec<i64>,
}

pub fn open(path: &str) -> Result<Connection> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).context("create db parent dir")?;
        }
    }
    let conn = Connection::open(path).context("open sqlite")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    init_schema(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS hits (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            site_id         TEXT    NOT NULL,
            path            TEXT    NOT NULL DEFAULT '/',
            referrer_domain TEXT    NOT NULL DEFAULT '',
            country         TEXT    NOT NULL DEFAULT '??',
            device          TEXT    NOT NULL DEFAULT 'desktop',
            visitor_hash    TEXT    NOT NULL,
            is_unique       INTEGER NOT NULL DEFAULT 0,
            created_at      TEXT    NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_hits_site_date ON hits(site_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_hits_date ON hits(created_at);
        CREATE INDEX IF NOT EXISTS idx_hits_unique ON hits(site_id, visitor_hash, created_at);
        "#,
    )?;
    Ok(())
}

pub async fn batch_writer(mut rx: mpsc::Receiver<HitRecord>, db: Arc<Mutex<Connection>>) {
    let mut buffer: Vec<HitRecord> = Vec::with_capacity(100);
    let mut interval = time::interval(time::Duration::from_secs(5));
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            maybe_hit = rx.recv() => {
                match maybe_hit {
                    Some(hit) => {
                        buffer.push(hit);
                        if buffer.len() >= 100 {
                            if let Err(e) = flush(&db, &mut buffer) {
                                tracing::error!(error=?e, "batch flush failed");
                            }
                        }
                    }
                    None => {
                        if !buffer.is_empty() {
                            if let Err(e) = flush(&db, &mut buffer) {
                                tracing::error!(error=?e, "final flush failed");
                            }
                        }
                        break;
                    }
                }
            }
            _ = interval.tick() => {
                if !buffer.is_empty() {
                    if let Err(e) = flush(&db, &mut buffer) {
                        tracing::error!(error=?e, "interval flush failed");
                    }
                }
            }
        }
    }
}

fn flush(db: &Arc<Mutex<Connection>>, buffer: &mut Vec<HitRecord>) -> Result<()> {
    let mut conn = db.lock().unwrap();
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO hits (site_id, path, referrer_domain, country, device, visitor_hash, is_unique, created_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        )?;
        for hit in buffer.drain(..) {
            stmt.execute(params![
                hit.site_id,
                hit.path,
                hit.referrer_domain,
                hit.country,
                hit.device,
                hit.visitor_hash,
                hit.is_unique as i64,
                hit.created_at.to_rfc3339(),
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn cleanup_old(db: &Arc<Mutex<Connection>>, days: i64) -> Result<usize> {
    let cutoff = (Utc::now() - Duration::days(days)).to_rfc3339();
    let conn = db.lock().unwrap();
    let n = conn.execute("DELETE FROM hits WHERE created_at < ?1", params![cutoff])?;
    Ok(n)
}

fn totals_global(conn: &Connection, since: &str) -> Result<(i64, i64)> {
    let row: (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COUNT(DISTINCT visitor_hash) FROM hits \
         WHERE created_at >= ?1 AND device != 'bot'",
        params![since],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(row)
}

fn totals_global_range(conn: &Connection, since: &str, until: &str) -> Result<(i64, i64)> {
    let row: (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COUNT(DISTINCT visitor_hash) FROM hits \
         WHERE created_at >= ?1 AND created_at < ?2 AND device != 'bot'",
        params![since, until],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(row)
}

fn totals_site(conn: &Connection, since: &str, site: &str) -> Result<(i64, i64)> {
    let row: (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COUNT(DISTINCT visitor_hash) FROM hits \
         WHERE created_at >= ?1 AND site_id = ?2 AND device != 'bot'",
        params![since, site],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(row)
}

fn totals_site_range(conn: &Connection, since: &str, until: &str, site: &str) -> Result<(i64, i64)> {
    let row: (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COUNT(DISTINCT visitor_hash) FROM hits \
         WHERE created_at >= ?1 AND created_at < ?2 AND site_id = ?3 AND device != 'bot'",
        params![since, until, site],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(row)
}

fn by_site(conn: &Connection, since: &str) -> Result<Vec<SiteCount>> {
    let mut stmt = conn.prepare(
        "SELECT site_id, COUNT(*), COUNT(DISTINCT visitor_hash) FROM hits \
         WHERE created_at >= ?1 AND device != 'bot' \
         GROUP BY site_id ORDER BY 2 DESC",
    )?;
    let rows = stmt
        .query_map(params![since], |r| {
            Ok(SiteCount {
                site_id: r.get(0)?,
                views: r.get(1)?,
                uniques: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn daily_global(conn: &Connection, since: &str) -> Result<Vec<DailyCount>> {
    let mut stmt = conn.prepare(
        "SELECT DATE(created_at) AS day, COUNT(*), COUNT(DISTINCT visitor_hash) \
         FROM hits WHERE created_at >= ?1 AND device != 'bot' \
         GROUP BY day ORDER BY day",
    )?;
    let rows = stmt
        .query_map(params![since], |r| {
            Ok(DailyCount {
                date: r.get(0)?,
                views: r.get(1)?,
                uniques: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn daily_site(conn: &Connection, since: &str, site: &str) -> Result<Vec<DailyCount>> {
    let mut stmt = conn.prepare(
        "SELECT DATE(created_at) AS day, COUNT(*), COUNT(DISTINCT visitor_hash) \
         FROM hits WHERE created_at >= ?1 AND site_id = ?2 AND device != 'bot' \
         GROUP BY day ORDER BY day",
    )?;
    let rows = stmt
        .query_map(params![since, site], |r| {
            Ok(DailyCount {
                date: r.get(0)?,
                views: r.get(1)?,
                uniques: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn referrers_global(conn: &Connection, since: &str) -> Result<Vec<RefCount>> {
    let mut stmt = conn.prepare(
        "SELECT referrer_domain, COUNT(*) FROM hits \
         WHERE created_at >= ?1 AND device != 'bot' \
         GROUP BY referrer_domain ORDER BY 2 DESC LIMIT 10",
    )?;
    let rows = stmt
        .query_map(params![since], |r| {
            Ok(RefCount {
                domain: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn referrers_site(conn: &Connection, since: &str, site: &str) -> Result<Vec<RefCount>> {
    let mut stmt = conn.prepare(
        "SELECT referrer_domain, COUNT(*) FROM hits \
         WHERE created_at >= ?1 AND site_id = ?2 AND device != 'bot' \
         GROUP BY referrer_domain ORDER BY 2 DESC LIMIT 10",
    )?;
    let rows = stmt
        .query_map(params![since, site], |r| {
            Ok(RefCount {
                domain: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn countries_global(conn: &Connection, since: &str) -> Result<Vec<CountryCount>> {
    let mut stmt = conn.prepare(
        "SELECT country, COUNT(*) FROM hits \
         WHERE created_at >= ?1 AND device != 'bot' \
         GROUP BY country ORDER BY 2 DESC LIMIT 10",
    )?;
    let rows = stmt
        .query_map(params![since], |r| {
            Ok(CountryCount {
                code: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn countries_site(conn: &Connection, since: &str, site: &str) -> Result<Vec<CountryCount>> {
    let mut stmt = conn.prepare(
        "SELECT country, COUNT(*) FROM hits \
         WHERE created_at >= ?1 AND site_id = ?2 AND device != 'bot' \
         GROUP BY country ORDER BY 2 DESC LIMIT 10",
    )?;
    let rows = stmt
        .query_map(params![since, site], |r| {
            Ok(CountryCount {
                code: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn devices_query(conn: &Connection, since: &str, site: Option<&str>) -> Result<DeviceCounts> {
    let rows: Vec<(String, i64)> = if let Some(s) = site {
        let mut stmt = conn.prepare(
            "SELECT device, COUNT(*) FROM hits \
             WHERE created_at >= ?1 AND site_id = ?2 AND device != 'bot' \
             GROUP BY device",
        )?;
        stmt.query_map(params![since, s], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT device, COUNT(*) FROM hits \
             WHERE created_at >= ?1 AND device != 'bot' \
             GROUP BY device",
        )?;
        stmt.query_map(params![since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut d = DeviceCounts::default();
    for (k, v) in rows {
        match k.as_str() {
            "desktop" => d.desktop = v,
            "mobile" => d.mobile = v,
            "tablet" => d.tablet = v,
            _ => {}
        }
    }
    Ok(d)
}

fn top_pages(conn: &Connection, since: &str, site: &str) -> Result<Vec<PageCount>> {
    let mut stmt = conn.prepare(
        "SELECT path, COUNT(*) FROM hits \
         WHERE created_at >= ?1 AND site_id = ?2 AND device != 'bot' \
         GROUP BY path ORDER BY 2 DESC LIMIT 10",
    )?;
    let rows = stmt
        .query_map(params![since, site], |r| {
            Ok(PageCount {
                path: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn by_hour(conn: &Connection, since: &str, site: &str) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT CAST(strftime('%H', created_at) AS INTEGER) AS h, COUNT(*) FROM hits \
         WHERE created_at >= ?1 AND site_id = ?2 AND device != 'bot' \
         GROUP BY h",
    )?;
    let rows = stmt
        .query_map(params![since, site], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = vec![0i64; 24];
    for (h, c) in rows {
        if (0..24).contains(&h) {
            out[h as usize] = c;
        }
    }
    Ok(out)
}

pub fn stats_global(db: &Arc<Mutex<Connection>>, period: Period) -> Result<Stats> {
    let conn = db.lock().unwrap();
    let since = period.since().to_rfc3339();
    let (prev_lo, prev_hi) = period.prev_window();
    let (total_views, total_uniques) = totals_global(&conn, &since)?;
    let (prev_views, prev_uniques) = if matches!(period, Period::All) {
        (0, 0)
    } else {
        totals_global_range(&conn, &prev_lo.to_rfc3339(), &prev_hi.to_rfc3339())?
    };
    Ok(Stats {
        period: period.as_str().to_string(),
        total_views,
        total_uniques,
        prev_period_views: prev_views,
        prev_period_uniques: prev_uniques,
        by_site: by_site(&conn, &since)?,
        daily: daily_global(&conn, &since)?,
        referrers: referrers_global(&conn, &since)?,
        countries: countries_global(&conn, &since)?,
        devices: devices_query(&conn, &since, None)?,
    })
}

pub fn stats_site(db: &Arc<Mutex<Connection>>, site_id: &str, period: Period) -> Result<SiteStats> {
    let conn = db.lock().unwrap();
    let since = period.since().to_rfc3339();
    let (prev_lo, prev_hi) = period.prev_window();
    let (total_views, total_uniques) = totals_site(&conn, &since, site_id)?;
    let (prev_views, prev_uniques) = if matches!(period, Period::All) {
        (0, 0)
    } else {
        totals_site_range(&conn, &prev_lo.to_rfc3339(), &prev_hi.to_rfc3339(), site_id)?
    };
    Ok(SiteStats {
        site_id: site_id.to_string(),
        period: period.as_str().to_string(),
        total_views,
        total_uniques,
        prev_period_views: prev_views,
        prev_period_uniques: prev_uniques,
        daily: daily_site(&conn, &since, site_id)?,
        referrers: referrers_site(&conn, &since, site_id)?,
        countries: countries_site(&conn, &since, site_id)?,
        devices: devices_query(&conn, &since, Some(site_id))?,
        top_pages: top_pages(&conn, &since, site_id)?,
        by_hour: by_hour(&conn, &since, site_id)?,
    })
}

pub fn list_sites(db: &Arc<Mutex<Connection>>) -> Result<Vec<SiteCount>> {
    let conn = db.lock().unwrap();
    let since = DateTime::<Utc>::from_timestamp(0, 0).unwrap().to_rfc3339();
    by_site(&conn, &since)
}
