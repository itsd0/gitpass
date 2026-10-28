use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const MAX_FAVICON_SIZE: usize = 100 * 1024; // 100 KB
pub const FAVICON_TTL_SECS: u64 = 30 * 24 * 60 * 60; // 30 days

#[derive(Serialize, Deserialize, Debug)]
struct CachedFavicon {
    ts: u64,
    data_url: Option<String>,
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn default_favicon_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("gitpass")
        .join("favicons")
}

pub fn host_hash(host: &str) -> String {
    gitpass_core::store::hash_bytes(host.trim().to_lowercase().as_bytes())
}

fn cache_file_path(cache_dir: &Path, host: &str) -> PathBuf {
    cache_dir.join(format!("{}.json", host_hash(host)))
}

fn get_cached(cache_dir: &Path, host: &str) -> Option<Option<String>> {
    let p = cache_file_path(cache_dir, host);
    let bytes = std::fs::read(&p).ok()?;
    let entry: CachedFavicon = serde_json::from_slice(&bytes).ok()?;
    let now = now_unix_secs();
    if now.saturating_sub(entry.ts) <= FAVICON_TTL_SECS {
        Some(entry.data_url)
    } else {
        let _ = std::fs::remove_file(p);
        None
    }
}

fn save_cached(cache_dir: &Path, host: &str, data_url: Option<String>) -> Result<()> {
    std::fs::create_dir_all(cache_dir)?;
    let p = cache_file_path(cache_dir, host);
    let entry = CachedFavicon {
        ts: now_unix_secs(),
        data_url,
    };
    let json = serde_json::to_vec(&entry)?;
    std::fs::write(p, json)?;
    Ok(())
}

pub fn clear_favicon_cache(cache_dir: &Path) -> Result<()> {
    if cache_dir.exists() {
        for entry in std::fs::read_dir(cache_dir)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_file() {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    Ok(())
}

pub fn detect_image_format(bytes: &[u8], content_type: Option<&str>) -> Option<&'static str> {
    // PNG magic bytes
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    // ICO magic bytes
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Some("image/x-icon");
    }
    // SVG text check
    if let Ok(text) = std::str::from_utf8(bytes) {
        let trimmed = text.trim();
        let looks_like_svg = trimmed.starts_with("<?xml")
            || trimmed.starts_with("<svg")
            || content_type.is_some_and(|ct| ct.contains("svg"));
        if looks_like_svg && trimmed.contains("</svg>") {
            return Some("image/svg+xml");
        }
    }
    None
}

fn read_capped_body(resp: ureq::Response, max_size: usize) -> Result<Vec<u8>> {
    if let Some(len) = resp.header("Content-Length").and_then(|l| l.parse::<usize>().ok()) {
        if len > max_size {
            bail!("Content-Length {} exceeds maximum cap {}", len, max_size);
        }
    }
    let mut reader = resp.into_reader().take((max_size + 1) as u64);
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    if buf.len() > max_size {
        bail!("Response body exceeded maximum cap {}", max_size);
    }
    Ok(buf)
}

fn extract_icon_href_from_html(html: &str) -> Option<String> {
    // Basic scanner for <link rel="(shortcut )?icon" href="...">
    let lower = html.to_lowercase();
    let mut start = 0;
    while let Some(link_idx) = lower[start..].find("<link") {
        let abs_link = start + link_idx;
        let end_idx = match lower[abs_link..].find('>') {
            Some(i) => abs_link + i,
            None => break,
        };
        let tag = &html[abs_link..=end_idx];
        let tag_lower = &lower[abs_link..=end_idx];
        start = end_idx + 1;

        let is_icon = tag_lower.contains("rel=\"icon\"")
            || tag_lower.contains("rel='icon'")
            || tag_lower.contains("rel=\"shortcut icon\"")
            || tag_lower.contains("rel='shortcut icon'")
            || tag_lower.contains("rel=\"apple-touch-icon\"")
            || tag_lower.contains("rel='apple-touch-icon'");

        if is_icon {
            // Find href
            if let Some(href_idx) = tag_lower.find("href=") {
                let rest = &tag[href_idx + 5..];
                let quote = rest.chars().next()?;
                if quote == '"' || quote == '\'' {
                    let rest_val = &rest[1..];
                    if let Some(close_idx) = rest_val.find(quote) {
                        return Some(rest_val[..close_idx].to_string());
                    }
                }
            }
        }
    }
    None
}

pub fn fetch_and_cache(
    raw_url: &str,
    cache_dir: &Path,
    allow_insecure_for_test: bool,
) -> Result<Option<String>> {
    let parsed = if let Ok(u) = url::Url::parse(raw_url) {
        u
    } else if let Ok(u) = url::Url::parse(&format!("https://{}", raw_url)) {
        u
    } else {
        return Ok(None);
    };

    let scheme = parsed.scheme();
    if !allow_insecure_for_test && scheme != "https" {
        bail!("non-HTTPS refused: {}", scheme);
    }

    let host = match parsed.host_str() {
        Some(h) if !h.is_empty() => h.to_string(),
        _ => return Ok(None),
    };

    // Check disk cache first
    if let Some(cached) = get_cached(cache_dir, &host) {
        return Ok(cached);
    }

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .build();

    let port_part = parsed.port().map(|p| format!(":{}", p)).unwrap_or_default();
    let base_url = format!("{}://{}{}", scheme, host, port_part);
    let favicon_ico_url = format!("{}/favicon.ico", base_url);

    // 1. Try /favicon.ico
    if let Ok(resp) = agent.get(&favicon_ico_url).call() {
        if resp.status() == 200 {
            let ct = resp.header("Content-Type").map(|s| s.to_string());
            if let Ok(bytes) = read_capped_body(resp, MAX_FAVICON_SIZE) {
                if let Some(mime) = detect_image_format(&bytes, ct.as_deref()) {
                    let data_url = format!("data:{};base64,{}", mime, B64.encode(&bytes));
                    let _ = save_cached(cache_dir, &host, Some(data_url.clone()));
                    return Ok(Some(data_url));
                }
            }
        }
    }

    // 2. Try fetching the root HTML page to find <link rel="icon">
    if let Ok(resp) = agent.get(&format!("{}/", base_url)).call() {
        if resp.status() == 200 {
            if let Ok(bytes) = read_capped_body(resp, MAX_FAVICON_SIZE) {
                if let Ok(html) = std::str::from_utf8(&bytes) {
                    if let Some(href) = extract_icon_href_from_html(html) {
                        let base_parsed = url::Url::parse(&format!("{}/", base_url))?;
                        if let Ok(resolved) = base_parsed.join(&href) {
                            if allow_insecure_for_test || resolved.scheme() == "https" {
                                if let Ok(icon_resp) = agent.get(resolved.as_str()).call() {
                                    if icon_resp.status() == 200 {
                                        let ct = icon_resp.header("Content-Type").map(|s| s.to_string());
                                        if let Ok(icon_bytes) = read_capped_body(icon_resp, MAX_FAVICON_SIZE) {
                                            if let Some(mime) = detect_image_format(&icon_bytes, ct.as_deref()) {
                                                let data_url = format!("data:{};base64,{}", mime, B64.encode(&icon_bytes));
                                                let _ = save_cached(cache_dir, &host, Some(data_url.clone()));
                                                return Ok(Some(data_url));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Save negative cache to prevent immediate repeated network attempts
    let _ = save_cached(cache_dir, &host, None);
    Ok(None)
}
