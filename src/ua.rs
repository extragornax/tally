use sha2::{Digest, Sha256};

pub fn classify_device(ua: &str) -> &'static str {
    let s = ua.to_lowercase();
    if s.contains("bot")
        || s.contains("crawl")
        || s.contains("spider")
        || s.contains("slurp")
        || s.contains("feed")
    {
        return "bot";
    }
    if s.contains("tablet") || s.contains("ipad") {
        return "tablet";
    }
    if s.contains("mobile") || s.contains("android") || s.contains("iphone") {
        return "mobile";
    }
    "desktop"
}

pub fn extract_referrer_domain(referrer: &str) -> String {
    if referrer.is_empty() {
        return "(direct)".to_string();
    }
    url::Url::parse(referrer)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| "(direct)".to_string())
}

pub fn extract_referrer_path(referrer: &str) -> String {
    if referrer.is_empty() {
        return "/".to_string();
    }
    url::Url::parse(referrer)
        .ok()
        .map(|u| {
            let p = u.path();
            if p.is_empty() { "/".to_string() } else { p.to_string() }
        })
        .unwrap_or_else(|| "/".to_string())
}

pub fn visitor_hash(ip: &str, site_id: &str, date: &str) -> String {
    let mut h = Sha256::new();
    h.update(format!("{ip}:{site_id}:{date}"));
    let result = h.finalize();
    hex::encode(&result[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_classification() {
        assert_eq!(classify_device("Googlebot/2.1"), "bot");
        assert_eq!(classify_device("Mozilla/5.0 (iPad; ...)"), "tablet");
        assert_eq!(classify_device("Mozilla/5.0 (iPhone; ...)"), "mobile");
        assert_eq!(classify_device("Mozilla/5.0 (Macintosh; ...)"), "desktop");
    }

    #[test]
    fn referrer_parse() {
        assert_eq!(extract_referrer_domain(""), "(direct)");
        assert_eq!(extract_referrer_domain("https://google.com/search?q=x"), "google.com");
        assert_eq!(extract_referrer_path("https://example.com/foo/bar"), "/foo/bar");
        assert_eq!(extract_referrer_path(""), "/");
    }

    #[test]
    fn hash_stable() {
        let a = visitor_hash("1.2.3.4", "gpx", "2026-05-15");
        let b = visitor_hash("1.2.3.4", "gpx", "2026-05-15");
        let c = visitor_hash("1.2.3.4", "gpx", "2026-05-16");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 16);
    }
}
