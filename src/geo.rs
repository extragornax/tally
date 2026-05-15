#[cfg(feature = "geoip")]
use std::net::IpAddr;

#[cfg(feature = "geoip")]
pub struct GeoReader {
    reader: maxminddb::Reader<Vec<u8>>,
}

#[cfg(not(feature = "geoip"))]
pub struct GeoReader;

#[cfg(feature = "geoip")]
pub fn open(path: &str) -> anyhow::Result<GeoReader> {
    let reader = maxminddb::Reader::open_readfile(path)?;
    Ok(GeoReader { reader })
}

#[cfg(not(feature = "geoip"))]
pub fn open(_path: &str) -> anyhow::Result<GeoReader> {
    anyhow::bail!("geoip feature not compiled in")
}

#[cfg(feature = "geoip")]
pub fn resolve(reader: Option<&GeoReader>, ip: &str) -> String {
    let Some(r) = reader else { return "??".to_string(); };
    let Ok(addr) = ip.parse::<IpAddr>() else { return "??".to_string(); };
    match r.reader.lookup::<maxminddb::geoip2::Country>(addr) {
        Ok(c) => c
            .country
            .and_then(|c| c.iso_code)
            .map(|s| s.to_string())
            .unwrap_or_else(|| "??".to_string()),
        Err(_) => "??".to_string(),
    }
}

#[cfg(not(feature = "geoip"))]
pub fn resolve(_reader: Option<&GeoReader>, _ip: &str) -> String {
    "??".to_string()
}
